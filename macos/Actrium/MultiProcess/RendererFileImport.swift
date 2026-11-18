import ActriumIPC
import AppKit
import PhotosUI
import UniformTypeIdentifiers

@MainActor
final class RendererFileImport {
    private struct Manifest: Decodable {
        struct Module: Decodable { let rpc: [String]? }
        let hostModules: [String: Module]
    }

    private var pendingTabs: Set<UUID> = []

    static func isDeclared(manifest: String?, permissions: [String]) -> Bool {
        guard permissions.contains(GuestFileImportCall.capability) || permissions.contains("host:file-import"),
              let manifest, let usage = try? JSONDecoder().decode(Manifest.self, from: Data(manifest.utf8)) else {
            return false
        }
        return usage.hostModules["file-import"]?.rpc?.contains("pick") == true
    }

    func reply(
        to json: String, tabId: UUID, authorizedWindow: (Int) -> NSWindow?
    ) async throws -> String? {
        guard json.utf8.count <= 32 * 1024 else { return nil }
        let call = try JSONDecoder().decode(GuestFileImportCall.self, from: Data(json.utf8))
        let reply = await pick(call, tabId: tabId) { authorizedWindow(call.reloadKey) }
        return try reply.encodedJSON()
    }

    private func pick(
        _ call: GuestFileImportCall, tabId: UUID, authorizedWindow: () -> NSWindow?
    ) async -> GuestFileImportReply {
        guard call.hasValidArguments, let window = authorizedWindow(), window.isKeyWindow else {
            return GuestFileImportReply(id: call.id, error: "File import is unavailable for this tab.")
        }
        guard window.attachedSheet == nil, pendingTabs.insert(tabId).inserted else {
            return GuestFileImportReply(id: call.id, error: "A selection dialog is already open.")
        }
        defer { pendingTabs.remove(tabId) }
        do {
            let files = try await NativeFilePicker.pick(call, in: window)
            guard authorizedWindow() === window else {
                return GuestFileImportReply(id: call.id, error: "File import authorization changed.")
            }
            return GuestFileImportReply(id: call.id, files: files)
        } catch {
            let message = (error as? NativeFilePicker.Failure)?.errorDescription ?? "Could not read the selected items."
            return GuestFileImportReply(id: call.id, error: message)
        }
    }
}

nonisolated private enum NativeFilePicker {
    private typealias File = GuestFileImportReply.File
    private static let maxCount = 10

    enum Failure: LocalizedError {
        case tooManyFiles, tooLarge, unsupportedFilter, unreadableItem

        var errorDescription: String? {
            switch self {
            case .tooManyFiles: return "Select at most 10 attachments at a time."
            case .tooLarge: return "Selected attachments exceed the 7 MiB limit."
            case .unsupportedFilter: return "The requested file type filter is invalid."
            case .unreadableItem: return "Could not read a selected item."
            }
        }
    }

    @MainActor
    static func pick(_ call: GuestFileImportCall, in window: NSWindow) async throws -> [GuestFileImportReply.File] {
        switch call.source {
        case .files: return try await pickFiles(call, in: window)
        case .photos: return try await pickPhotos(multiple: call.multiple, in: window)
        }
    }

    @MainActor
    private static func pickFiles(_ call: GuestFileImportCall, in window: NSWindow) async throws -> [File] {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = call.multiple
        panel.allowedContentTypes = try call.accept.map(contentType)
        guard await panel.beginSheetModal(for: window) == .OK else { return [] }
        let urls = panel.urls
        guard urls.count <= maxCount else { throw Failure.tooManyFiles }
        return try await Task.detached {
            var remaining = GuestFileImportCall.maxBytes
            return try urls.map { url in
                let file = try readFile(url, limit: remaining)
                remaining -= file.data.count
                return file
            }
        }.value
    }

    @MainActor
    private static func pickPhotos(multiple: Bool, in window: NSWindow) async throws -> [File] {
        var configuration = PHPickerConfiguration()
        configuration.filter = .images
        configuration.preferredAssetRepresentationMode = .current
        configuration.selectionLimit = multiple ? maxCount : 1
        if multiple { configuration.selection = .ordered }
        let picker = PHPickerViewController(configuration: configuration)
        let selection = PhotoSelection()
        let results = await selection.select(with: picker, in: window)
        guard results.count <= maxCount else { throw Failure.tooManyFiles }
        var files: [File] = []
        var remaining = GuestFileImportCall.maxBytes
        for result in results {
            let file = try await loadPhoto(result.itemProvider, limit: remaining)
            files.append(file)
            remaining -= file.data.count
        }
        return files
    }

    private static func loadPhoto(_ provider: NSItemProvider, limit: Int) async throws -> File {
        guard let type = provider.registeredTypeIdentifiers.first(where: { UTType($0)?.conforms(to: .image) == true }) else {
            throw Failure.unreadableItem
        }
        return try await withCheckedThrowingContinuation { continuation in
            _ = provider.loadFileRepresentation(forTypeIdentifier: type) { url, error in
                continuation.resume(with: Result {
                    if let error { throw error }
                    guard let url else { throw Failure.unreadableItem }
                    return try readFile(url, limit: limit)
                })
            }
        }
    }

    private static func contentType(for filter: String) throws -> UTType {
        switch filter {
        case "*/*": return .item
        case "image/*": return .image
        case "audio/*": return .audio
        case "video/*": return .movie
        default:
            let type = filter.hasPrefix(".")
                ? UTType(filenameExtension: String(filter.dropFirst()))
                : UTType(mimeType: filter)
            guard let type else { throw Failure.unsupportedFilter }
            return type
        }
    }

    private static func readFile(_ url: URL, limit: Int) throws -> File {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }

        let fd = open(url.path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK)
        guard fd >= 0 else { throw Failure.unreadableItem }
        let handle = FileHandle(fileDescriptor: fd, closeOnDealloc: true)
        var info = stat()
        guard fstat(fd, &info) == 0, info.st_mode & S_IFMT == S_IFREG else { throw Failure.unreadableItem }
        guard info.st_size <= limit else { throw Failure.tooLarge }

        var bytes = Data()
        while let block = try handle.read(upToCount: min(64 * 1024, limit - bytes.count + 1)), !block.isEmpty {
            guard block.count <= limit - bytes.count else { throw Failure.tooLarge }
            bytes.append(block)
        }
        return File(name: url.lastPathComponent, data: bytes)
    }
}

@MainActor
private final class PhotoSelection: NSObject, PHPickerViewControllerDelegate {
    private var results: [PHPickerResult] = []

    func select(with picker: PHPickerViewController, in window: NSWindow) async -> [PHPickerResult] {
        let sheet = NSWindow(contentViewController: picker)
        sheet.title = "Choose Photos"
        sheet.setContentSize(NSSize(width: 640, height: 480))
        picker.delegate = self
        _ = await window.beginSheet(sheet)
        picker.delegate = nil
        return results
    }

    func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
        self.results = results
        if let sheet = picker.view.window {
            sheet.sheetParent?.endSheet(sheet)
        }
    }
}
