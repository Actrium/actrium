import ActriumIPC
import AppKit

@MainActor
final class RendererGuestMenu {
    private struct Manifest: Decodable {
        let components: [String]
    }

    private var isPresenting = false

    func show(_ call: GuestMenuCall, tabId: UUID) -> GuestMenuReply {
        guard !isPresenting, call.hasValidArguments,
              let surface = ViewBridgeCoordinatorManager.shared.existingRendererSurface(for: tabId),
              let point = menuPoint(for: call, on: surface) else {
            return GuestMenuReply(id: call.id, error: "The menu anchor is unavailable.")
        }
        isPresenting = true
        defer { isPresenting = false }
        let selection = GuestMenuSelection()
        let menu = makeMenu(call.content, target: selection)
        menu.popUp(positioning: nil, at: point, in: surface)
        guard surface.tabId == tabId, menuPoint(for: call, on: surface) != nil else {
            return GuestMenuReply(id: call.id)
        }
        return GuestMenuReply(id: call.id, value: selection.value)
    }

    private func menuPoint(for call: GuestMenuCall, on surface: RendererSurfaceHostView) -> NSPoint? {
        guard surface.allowsInput, surface.window?.isKeyWindow == true, !surface.isHiddenOrHasHiddenAncestor,
              let config = surface.lastConfig, (config["reloadKey"] as? Int ?? 0) == call.reloadKey,
              let json = config["capabilityManifestJSON"] as? String,
              let manifest = try? JSONDecoder().decode(Manifest.self, from: Data(json.utf8)),
              manifest.components.contains(call.content.component.rawValue) else { return nil }
        let anchor = call.content.anchor
        let rect = NSRect(x: anchor.x, y: anchor.y, width: anchor.width, height: anchor.height)
            .intersection(surface.visibleRect)
        guard !rect.isEmpty else { return nil }
        // Renderer coordinates and the surface are both top-down.
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
