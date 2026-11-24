import ActriumIPC
import AppKit

@MainActor
final class RendererGuestMenu {
    private struct Manifest: Decodable {
        let components: [String]
    }

    private struct RequestIdentity: Decodable {
        let id: UUID
    }

    private struct Presentation {
        let cancelTracking: () -> Void
        let tabId: UUID
        let reloadKey: Int
        var cancelled = false
    }

    private var active: Presentation?

    static func reply(
        to json: String, isCurrent: () -> Bool, present: (GuestMenuCall) -> GuestMenuReply
    ) throws -> GuestMenuReply {
        let data = Data(json.utf8)
        let decoder = JSONDecoder()
        let identity = try decoder.decode(RequestIdentity.self, from: data)
        let unavailable = GuestMenuReply(id: identity.id, error: "The menu tab is no longer available.")
        guard isCurrent() else { return unavailable }
        guard data.count <= GuestMenuCall.maxEncodedRequestBytes else {
            return GuestMenuReply(id: identity.id, error: "The menu request is too large.")
        }
        guard let call = try? decoder.decode(GuestMenuCall.self, from: data) else {
            return GuestMenuReply(id: identity.id, error: "Invalid menu request.")
        }
        let reply = present(call)
        return isCurrent() ? reply : unavailable
    }

    func show(_ call: GuestMenuCall, tabId: UUID) -> GuestMenuReply {
        guard active == nil, call.hasValidArguments,
              let surface = ViewBridgeCoordinatorManager.shared.existingRendererSurface(for: tabId),
              let point = menuPoint(for: call, on: surface) else {
            return GuestMenuReply(id: call.id, error: "The menu anchor is unavailable.")
        }
        let selection = GuestMenuSelection()
        let menu = makeMenu(call.content, target: selection)
        let wasCurrent = present(
            tabId: tabId, reloadKey: call.reloadKey, cancelTracking: menu.cancelTracking
        ) {
            menu.popUp(positioning: nil, at: point, in: surface)
        }
        guard wasCurrent, surface.tabId == tabId, menuPoint(for: call, on: surface) != nil else {
            return GuestMenuReply(id: call.id)
        }
        return GuestMenuReply(id: call.id, value: selection.value)
    }

    func present(tabId: UUID, reloadKey: Int, cancelTracking: @escaping () -> Void, show: () -> Void) -> Bool {
        guard active == nil else { return false }
        active = Presentation(cancelTracking: cancelTracking, tabId: tabId, reloadKey: reloadKey)
        defer { active = nil }
        show()
        return active?.cancelled == false
    }

    func update(tabId: UUID, reloadKey: Int) {
        guard let active, active.reloadKey != reloadKey else { return }
        cancel(tabId: tabId)
    }

    func cancel(tabId: UUID) {
        guard let active, active.tabId == tabId, !active.cancelled else { return }
        self.active?.cancelled = true
        active.cancelTracking()
    }

    private func menuPoint(for call: GuestMenuCall, on surface: RendererSurfaceHostView) -> NSPoint? {
        guard surface.allowsInput, surface.window?.isKeyWindow == true, !surface.isHiddenOrHasHiddenAncestor,
              let config = surface.lastConfig, (config["reloadKey"] as? Int ?? 0) == call.reloadKey,
              let json = config["capabilityManifestJSON"] as? String,
              let manifest = try? JSONDecoder().decode(Manifest.self, from: Data(json.utf8)),
              manifest.components.contains(call.content.component.rawValue) else { return nil }
        return Self.anchorPoint(call.content.anchor, in: surface)
    }

    static func anchorPoint(_ anchor: GuestMenuCall.Anchor, in surface: RendererSurfaceHostView) -> NSPoint? {
        // The remote layer fills the flipped surface's bounds.
        let rect = NSRect(
            x: surface.bounds.minX + anchor.x, y: surface.bounds.minY + anchor.y,
            width: anchor.width, height: anchor.height
        ).intersection(surface.visibleRect)
        guard !rect.isEmpty else { return nil }
        return NSPoint(x: rect.minX, y: rect.maxY)
    }

    private func makeMenu(_ content: GuestMenuCall.Content, target: GuestMenuSelection) -> NSMenu {
        let menu = NSMenu()
        menu.autoenablesItems = false
        for item in content.items {
            let entry = NSMenuItem(title: item.label, action: #selector(GuestMenuSelection.choose(_:)), keyEquivalent: "")
            entry.target = target
            entry.representedObject = item.value
            entry.isEnabled = item.disabled != true
            entry.state = item.value == content.selectedValue ? .on : .off
            menu.addItem(entry)
        }
        return menu
    }
}

@MainActor
private final class GuestMenuSelection: NSObject {
    private(set) var value: String?

    @objc func choose(_ item: NSMenuItem) {
        value = item.representedObject as? String
    }
}
