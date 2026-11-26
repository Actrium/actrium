import ActriumIPC
import AppKit

@MainActor
final class RendererFileExport {
    private struct Pending {
        let id: UUID
        let reloadKey: Int
        let window: NSWindow
        let authorizedWindow: (Int) -> NSWindow?
        let panel: NSSavePanel
    }

    private var pending: [UUID: Pending] = [:]

    static func isDeclared(manifest: String?, permissions: [String]) -> Bool {
        struct Module: Decodable { let rpc: [String]? }
        struct Manifest: Decodable { let hostModules: [String: Module] }
        guard permissions.contains(GuestFileExportCall.capability) || permissions.contains("host:file-export"),
              let manifest, let usage = try? JSONDecoder().decode(Manifest.self, from: Data(manifest.utf8)) else {
            return false
        }
        return usage.hostModules["file-export"]?.rpc?.contains("save") == true
    }

    func update(tabId: UUID, reloadKey: Int) {
        guard let request = pending[tabId] else { return }
        if request.reloadKey != reloadKey || request.authorizedWindow(reloadKey) !== request.window {
            cancel(tabId: tabId)
        }
    }

    func cancel(tabId: UUID) {
        pending.removeValue(forKey: tabId)?.panel.cancel(nil)
    }

    func reply(
        to call: GuestFileExportCall, tabId: UUID, authorizedWindow: @escaping (Int) -> NSWindow?
    ) async -> GuestFileExportReply {
        guard call.hasValidArguments, let window = authorizedWindow(call.reloadKey), window.isKeyWindow else {
            return GuestFileExportReply(id: call.id, error: "File export is unavailable for this tab or file.")
        }
        guard window.attachedSheet == nil, pending[tabId] == nil else {
            return GuestFileExportReply(id: call.id, error: "A file dialog is already open.")
        }
        let panel = NSSavePanel()
        panel.nameFieldStringValue = call.name
        panel.canCreateDirectories = true
        let requestID = call.id
        pending[tabId] = Pending(
            id: requestID, reloadKey: call.reloadKey, window: window, authorizedWindow: authorizedWindow, panel: panel
        )
        DispatchQueue.main.asyncAfter(deadline: .now() + .seconds(GuestFileExportCall.timeoutSeconds)) { [weak self] in
            guard let self, self.pending[tabId]?.id == requestID else { return }
            self.cancel(tabId: tabId)
        }
        let response = await panel.beginSheetModal(for: window)
        guard pending[tabId]?.id == requestID else {
            return GuestFileExportReply(id: call.id, error: "The save dialog was closed or timed out.")
        }
        pending.removeValue(forKey: tabId)
        guard response == .OK, let url = panel.url else { return GuestFileExportReply(id: call.id) }
        guard authorizedWindow(call.reloadKey) === window else {
            return GuestFileExportReply(id: call.id, error: "File export authorization changed.")
        }
        return await Self.write(call, to: url)
    }

    private nonisolated static func write(_ call: GuestFileExportCall, to url: URL) async -> GuestFileExportReply {
        await Task.detached(priority: .userInitiated) {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            do {
                try call.data.write(to: url, options: .atomic)
                return GuestFileExportReply(id: call.id, saved: true)
            } catch let error as CocoaError where error.code == .fileWriteOutOfSpace {
                return GuestFileExportReply(id: call.id, error: "There is not enough disk space to save the file.")
            } catch let error as CocoaError where error.code == .fileWriteNoPermission {
                return GuestFileExportReply(id: call.id, error: "The selected location does not allow writing this file.")
            } catch {
                return GuestFileExportReply(id: call.id, error: "Could not write the file to the selected location.")
            }
        }.value
    }
}
