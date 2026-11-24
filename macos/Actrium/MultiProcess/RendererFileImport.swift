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

    private struct RequestIdentity: Decodable {
        let id: UUID
    }

    private struct Pending {
        let id: UUID
        let reloadKey: Int
        let window: NSWindow
        let authorizedWindow: () -> NSWindow?
        let picker: any FileImportPicking
        let task: Task<Void, Never>
        let complete: (GuestFileImportReply) -> Void
    }

    private let makePicker: () -> any FileImportPicking
    private let timeout: TimeInterval
    private var pending: [UUID: Pending] = [:]

    convenience init() {
        self.init(makePicker: { NativeFilePicker() })
    }

    init(
        timeout: TimeInterval = TimeInterval(GuestFileImportCall.timeoutSeconds),
        makePicker: @escaping () -> any FileImportPicking
    ) {
        self.timeout = timeout
        self.makePicker = makePicker
    }

    static func isDeclared(manifest: String?, permissions: [String]) -> Bool {
        guard permissions.contains(GuestFileImportCall.capability) || permissions.contains("host:file-import"),
              let manifest, let usage = try? JSONDecoder().decode(Manifest.self, from: Data(manifest.utf8)) else {
            return false
        }
        return usage.hostModules["file-import"]?.rpc?.contains("pick") == true
    }

    func update(tabId: UUID, reloadKey: Int) {
        guard let request = pending[tabId] else { return }
        if request.reloadKey != reloadKey || request.authorizedWindow() !== request.window {
            cancel(tabId: tabId)
        }
    }

    func cancel(tabId: UUID, message: String = "The file selection was cancelled.") {
        guard let request = pending.removeValue(forKey: tabId) else { return }
        request.task.cancel()
        request.picker.cancel()
        request.complete(GuestFileImportReply(id: request.id, error: message))
    }

    func reply(
        to json: String, tabId: UUID, authorizedWindow: @escaping (Int) -> NSWindow?
    ) async throws -> String? {
        let data = Data(json.utf8)
        guard data.count <= GuestFileImportCall.maxRequestBytes else {
            AppLogger.reactNative.warning("Rejected an oversized file import request.")
            let identity = try JSONDecoder().decode(RequestIdentity.self, from: data)
            return try GuestFileImportReply(
                id: identity.id, error: "The file import request exceeds 32 KiB."
            ).encodedJSON()
        }
        let call = try JSONDecoder().decode(GuestFileImportCall.self, from: data)
        let reply = await pick(call, tabId: tabId) { authorizedWindow(call.reloadKey) }
        return try reply.encodedJSON()
    }

    private func pick(
        _ call: GuestFileImportCall, tabId: UUID, authorizedWindow: @escaping () -> NSWindow?
    ) async -> GuestFileImportReply {
        guard call.hasValidArguments, let window = authorizedWindow(), window.isKeyWindow else {
            return GuestFileImportReply(id: call.id, error: "File import is unavailable for this tab.")
        }
        guard window.attachedSheet == nil, pending[tabId] == nil else {
            return GuestFileImportReply(id: call.id, error: "A selection dialog is already open.")
        }
        let picker = makePicker()
        return await withCheckedContinuation { continuation in
            let task = Task {
                let reply = await select(call, picker: picker, window: window, authorizedWindow: authorizedWindow)
                finish(tabId: tabId, reply: reply)
            }
            pending[tabId] = Pending(
                id: call.id, reloadKey: call.reloadKey, window: window, authorizedWindow: authorizedWindow,
                picker: picker, task: task, complete: { continuation.resume(returning: $0) }
            )
            let requestId = call.id
            DispatchQueue.main.asyncAfter(deadline: .now() + timeout) { [weak self] in
                guard let self, self.pending[tabId]?.id == requestId else { return }
                self.cancel(tabId: tabId, message: "The file selection timed out.")
            }
        }
    }

    private func select(
        _ call: GuestFileImportCall, picker: any FileImportPicking, window: NSWindow,
        authorizedWindow: () -> NSWindow?
    ) async -> GuestFileImportReply {
        do {
            let files = try await picker.pick(call, in: window)
            try Task.checkCancellation()
            guard authorizedWindow() === window else {
                return GuestFileImportReply(id: call.id, error: "File import authorization changed.")
            }
            return GuestFileImportReply(id: call.id, files: files)
        } catch {
            let message = (error as? NativeFilePicker.Failure)?.errorDescription ?? "Could not read the selected items."
            return GuestFileImportReply(id: call.id, error: message)
        }
    }

    private func finish(tabId: UUID, reply: GuestFileImportReply) {
        guard pending[tabId]?.id == reply.id else { return }
        pending.removeValue(forKey: tabId)?.complete(reply)
    }
}

@MainActor
protocol FileImportPicking: AnyObject {
    func pick(_ call: GuestFileImportCall, in window: NSWindow) async throws -> [GuestFileImportReply.File]
    func cancel()
}

@MainActor
private final class NativeFilePicker: FileImportPicking {
    private typealias File = GuestFileImportReply.File
    private static let maxCount = 10
    private var cancelPresentation: (() -> Void)?
    private var loadingPhoto: CheckedContinuation<File, Error>?
    private var photoProgress: Progress?
    private var readingFiles: Task<[File], Error>?

    func cancel() {
        cancelPresentation?()
        readingFiles?.cancel()
        let progress = photoProgress
        // Consume the continuation first so a late provider callback cannot resume it twice.
        finishPhoto(.failure(CancellationError()))
        progress?.cancel()
    }

    nonisolated enum Failure: LocalizedError {
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

    func pick(_ call: GuestFileImportCall, in window: NSWindow) async throws -> [GuestFileImportReply.File] {
        try Task.checkCancellation()
        switch call.source {
        case .files: return try await pickFiles(call, in: window)
        case .photos: return try await pickPhotos(multiple: call.multiple, in: window)
        }
    }

    private func pickFiles(_ call: GuestFileImportCall, in window: NSWindow) async throws -> [File] {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = call.multiple
        panel.allowedContentTypes = try call.accept.map(Self.contentType)
        cancelPresentation = { panel.cancel(nil) }
        defer { cancelPresentation = nil }
        guard await panel.beginSheetModal(for: window) == .OK else { return [] }
        try Task.checkCancellation()
        let urls = panel.urls
        guard urls.count <= Self.maxCount else { throw Failure.tooManyFiles }
        let reading = Task.detached {
            var remaining = GuestFileImportCall.maxBytes
            return try urls.map { url in
                let file = try Self.readFile(url, limit: remaining)
                remaining -= file.data.count
                return file
            }
        }
        readingFiles = reading
        defer { readingFiles = nil }
        return try await reading.value
    }

    private func pickPhotos(multiple: Bool, in window: NSWindow) async throws -> [File] {
        var configuration = PHPickerConfiguration()
        configuration.filter = .images
        configuration.preferredAssetRepresentationMode = .current
        configuration.selectionLimit = multiple ? Self.maxCount : 1
        if multiple { configuration.selection = .ordered }
        let picker = PHPickerViewController(configuration: configuration)
        let selection = PhotoSelection()
        cancelPresentation = { selection.cancel() }
        defer { cancelPresentation = nil }
        let results = await selection.select(with: picker, in: window)
        try Task.checkCancellation()
        guard results.count <= Self.maxCount else { throw Failure.tooManyFiles }
        var files: [File] = []
        var remaining = GuestFileImportCall.maxBytes
        for result in results {
            let file = try await loadPhoto(result.itemProvider, limit: remaining)
            files.append(file)
            remaining -= file.data.count
        }
        return files
    }

    private func loadPhoto(_ provider: NSItemProvider, limit: Int) async throws -> File {
        try Task.checkCancellation()
        guard let type = provider.registeredTypeIdentifiers.first(where: { UTType($0)?.conforms(to: .image) == true }) else {
            throw Failure.unreadableItem
        }
        return try await withCheckedThrowingContinuation { continuation in
            loadingPhoto = continuation
            photoProgress = provider.loadFileRepresentation(forTypeIdentifier: type) { [weak self] url, error in
                let result = Result {
                    if let error { throw error }
                    guard let url else { throw Failure.unreadableItem }
                    return try Self.readFile(url, limit: limit)
                }
                Task { @MainActor in self?.finishPhoto(result) }
            }
        }
    }

    private func finishPhoto(_ result: Result<File, Error>) {
        let continuation = loadingPhoto
        loadingPhoto = nil
        photoProgress = nil
        continuation?.resume(with: result)
    }

    nonisolated private static func contentType(for filter: String) throws -> UTType {
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

    nonisolated private static func readFile(_ url: URL, limit: Int) throws -> File {
        try Task.checkCancellation()
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
            try Task.checkCancellation()
            guard block.count <= limit - bytes.count else { throw Failure.tooLarge }
            bytes.append(block)
        }
        let thumbnail = RendererAttachmentThumbnail.make(data: bytes, fileExtension: url.pathExtension)
        return File(name: url.lastPathComponent, data: bytes, thumbnail: thumbnail)
    }
}

@MainActor
private final class PhotoSelection: NSObject, PHPickerViewControllerDelegate {
    private var results: [PHPickerResult] = []
    private var sheet: NSWindow?

    func cancel() {
        if let sheet { sheet.sheetParent?.endSheet(sheet) }
    }

    func select(with picker: PHPickerViewController, in window: NSWindow) async -> [PHPickerResult] {
        let sheet = NSWindow(contentViewController: picker)
        self.sheet = sheet
        defer { self.sheet = nil }
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
