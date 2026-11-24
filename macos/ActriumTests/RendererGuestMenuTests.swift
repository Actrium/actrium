import ActriumIPC
import AppKit
import Testing
@testable import Actrium

@MainActor
struct RendererGuestMenuTests {
    private let id = UUID()

    @Test func rejectsStaleWorkersAndInvalidRequestsWithTheOriginalID() throws {
        let invalid = #"{"id":"\#(id.uuidString)","content":null}"#
        let oversized = #"{"id":"\#(id.uuidString)","padding":"\#(String(repeating: "x", count: GuestMenuCall.maxEncodedRequestBytes))"}"#
        let cases = [
            (invalid, false, "The menu tab is no longer available."),
            (invalid, true, "Invalid menu request."),
            (oversized, true, "The menu request is too large."),
        ]
        for (json, isCurrent, error) in cases {
            let reply = try RendererGuestMenu.reply(to: json, isCurrent: { isCurrent }) { _ in
                Issue.record("A rejected request must not present a menu.")
                return GuestMenuReply(id: id)
            }
            #expect(reply.id == id)
            #expect(reply.error == error)
        }
    }

    @Test func rejectsASelectionWhenTheWorkerChangesWhileTheMenuIsOpen() throws {
        var isCurrent = true
        let reply = try RendererGuestMenu.reply(to: validRequest, isCurrent: { isCurrent }) { call in
            #expect(call.id == id)
            isCurrent = false
            return GuestMenuReply(id: call.id, value: "open")
        }
        #expect(reply.id == id)
        #expect(reply.error != nil)
        #expect(reply.value == nil)
    }

    @Test func anchorsToTheVisiblePartOfAnOffsetSurface() throws {
        let frame = NSRect(x: 0, y: 0, width: 240, height: 120)
        let window = NSWindow(contentRect: frame, styleMask: .borderless, backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        defer { window.close() }
        let clip = NSView(frame: frame)
        clip.clipsToBounds = true
        window.contentView = clip
        let surface = RendererSurfaceHostView(frame: NSRect(x: 80, y: 20, width: 200, height: 100))
        clip.addSubview(surface)
        let anchor = try JSONDecoder().decode(GuestMenuCall.self, from: Data(validRequest.utf8)).content.anchor
        #expect(RendererGuestMenu.anchorPoint(anchor, in: surface) == NSPoint(x: 150, y: 34))

        surface.setFrameOrigin(NSPoint(x: 100, y: 20))
        #expect(RendererGuestMenu.anchorPoint(anchor, in: surface) == nil)
    }

    @Test func closesOnlyTheMenuForAnInvalidatedTabOrReload() {
        let presenter = RendererGuestMenu()
        let tabId = UUID()
        var cancelCount = 0
        let cancelTracking = { cancelCount += 1 }
        let reloaded = presenter.present(tabId: tabId, reloadKey: 1, cancelTracking: cancelTracking) {
            presenter.update(tabId: tabId, reloadKey: 1)
            presenter.update(tabId: UUID(), reloadKey: 2)
            presenter.cancel(tabId: UUID())
            #expect(cancelCount == 0)

            presenter.update(tabId: tabId, reloadKey: 2)
            presenter.cancel(tabId: tabId)
            #expect(cancelCount == 1)
        }
        #expect(!reloaded)
        let closed = presenter.present(tabId: tabId, reloadKey: 2, cancelTracking: cancelTracking) {
            presenter.cancel(tabId: tabId)
        }
        #expect(!closed)
        #expect(cancelCount == 2)
        #expect(presenter.present(tabId: tabId, reloadKey: 2, cancelTracking: cancelTracking, show: {}))
    }

    private var validRequest: String {
        """
        {"id":"\(id.uuidString)","reloadKey":1,"content":{"component":"Menu",
         "anchor":{"x":150,"y":14,"width":30,"height":20},"items":[{"value":"open","label":"Open"}]}}
        """
    }
}
