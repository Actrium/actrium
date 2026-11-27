import ActriumIPC
import AppKit
import Darwin
import Foundation
import QuartzCore
import React

/// JSON payload for Host-mirrored IME text operations.
struct IMETextPayload: Codable {
    var text: String
    var loc: Int = 0
    var len: Int = 0
}

/// JSON payload for Host-forwarded editing actions (menu key equivalents).
struct IMECommandPayload: Codable {
    var command: String
}

// MARK: - RN TextInput marked-text event gate

/// Restores iOS-style IME composition semantics to RN macOS's Fabric
/// `RCTTextInputComponentView`:
///
/// 1. `textInputDidChange` (the native origin of JS `onChangeText`) is
///    suppressed while the component is composing. RN macOS emits the event
///    for the FIRST marked-text update, polluting the JS controlled state
///    with the first composition letter (pinyin "nihao" → state stuck at
///    "n"). iOS emits no change events during composition. The mirror feeds
///    marked text via `applyMarkedText`, so composition membership is tracked
///    explicitly in `rnComposingComponents` rather than read back from
///    `markedRange` — the delegate callback fires while `setMarkedText:` is
///    still in flight, before the marked range (or even the backing view
///    resolution) is reliable.
/// 2. The commit path (`applyInsertText`, detected by marked text being
///    present on the editor right before the insert) clears RN's
///    `_ignoreNextTextInputCall` latch and re-drives the event explicitly.
///    Plain-text inserts never see pre-insert marked text, so ordinary
///    characters emit exactly one `onChange` from RN's own callback.
private var rnMarkedTextGateInstalled = false

/// Components with an in-flight IME composition, fed by `applyMarkedText`
/// and consumed by `applyInsertText`. Each entry records the editor the
/// component was composing in: a stale entry (recycled Fabric input, ended
/// field-editor session) never matches the component's current editor, so a
/// recycled `RCTTextInputComponentView` cannot carry composition state into
/// a new editor. Weak keys and values: a forgotten entry must not keep a
/// dead component or editor alive. The table is global to match the
/// swizzled method; window ownership is enforced by the prune paths.
private let rnComposingComponents = NSMapTable<AnyObject, AnyObject>.weakToWeakObjects()

private func installRNMarkedTextGate() {
    guard !rnMarkedTextGateInstalled,
          let cls = NSClassFromString("RCTTextInputComponentView")
    else { return }
    rnMarkedTextGateInstalled = true

    // method_setImplementation + direct old-IMP calls: an exchange-based
    // swizzle recursed here because Swift statically dispatched the
    // "original" call back into the probe, overflowing the stack.
    let changeSel = NSSelectorFromString("textInputDidChange")
    guard let method = class_getInstanceMethod(cls, changeSel) else { return }
    typealias ChangeIMP = @convention(c) (AnyObject, Selector) -> Void
    let oldIMP = method_getImplementation(method)
    let block: @convention(block) (AnyObject) -> Void = { obj in
        if let composingEditor = rnComposingComponents.object(forKey: obj) {
            // Suppress only while the component still edits through the same
            // editor the composition started in. A recycled Fabric input
            // re-mounted with a new editor must not inherit the stale entry.
            if let backing = (try? obj.value(forKey: "backedTextInputView")) as? NSView,
               rnEditorTextView(under: backing) === composingEditor {
                return
            }
            rnComposingComponents.removeObject(forKey: obj)
        }
        unsafeBitCast(oldIMP, to: ChangeIMP.self)(obj, changeSel)
    }
    method_setImplementation(method, imp_implementationWithBlock(block))
}

/// The Fabric `RCTTextInputComponentView` owning `resolved` — the text view
/// the renderer resolved as first responder. For a single-line input that
/// view is AppKit's shared field editor; its `delegate` is the owning
/// `RCTUITextField`, which exposes `textInputDelegate`. Returns nil unless
/// the delegate really is the expected Fabric component, so RN internals are
/// only touched through the documented key paths.
private func rnComponentView(for resolved: NSView?) -> NSObject? {
    guard let resolved else { return nil }
    let backing: NSView
    if let editor = resolved as? NSTextView,
       let field = editor.delegate as? NSTextField {
        backing = field
    } else {
        backing = resolved
    }
    guard let component = (try? backing.value(forKey: "textInputDelegate")) as? NSObject,
          let cls = NSClassFromString("RCTTextInputComponentView"),
          component.isKind(of: cls)
    else { return nil }
    return component
}

/// The text view a component edits through: the field's active field editor
/// for a single-line input, otherwise the NSTextView inside the backing
/// view hierarchy.
private func rnEditorTextView(under view: NSView) -> NSTextView? {
    if let field = view as? NSTextField {
        return field.currentEditor() as? NSTextView
    }
    if let textView = view as? NSTextView { return textView }
    var queue = view.subviews
    while !queue.isEmpty {
        let sub = queue.removeFirst()
        if let textView = sub as? NSTextView { return textView }
        queue.append(contentsOf: sub.subviews)
    }
    return nil
}

/// Drop composition state owned by `window` for every component whose
/// editor is not `activeEditor`. Editing ends or focus moves while a
/// composition is in flight (blur, click into another input) never reaches
/// the commit path, so without this the abandoned component would stay in
/// the gate's composing set and have ALL of its later change events
/// suppressed. A nil `activeEditor` drops every entry owned by the window
/// (e.g. a single-line blur with no replacement editor — `currentEditor()`
/// is nil for the old field, and `nil !== nil` would otherwise retain it).
/// Entries owned by OTHER windows are left untouched: a shared renderer
/// process serves several tabs, and a first-responder change in one tab
/// must not evict a live composition in another.
private func rnPruneComposingComponents(activeEditor: NSTextView?, inWindow window: NSWindow?) {
    guard let keys = rnComposingComponents.keyEnumerator().allObjects as? [AnyObject] else { return }
    for component in keys {
        guard let backing = (try? component.value(forKey: "backedTextInputView")) as? NSView else {
            rnComposingComponents.removeObject(forKey: component)
            continue
        }
        guard backing.window === window else { continue }
        // Keep an entry only for a non-nil active editor that is still the
        // editor the composition was recorded on: a single-line blur leaves
        // currentEditor() nil, and `nil !== nil` would otherwise retain it.
        if activeEditor == nil || rnComposingComponents.object(forKey: component) !== activeEditor {
            rnComposingComponents.removeObject(forKey: component)
        }
    }
}

/// Notification posted on the renderer's main queue after any
/// `NSWindow.makeFirstResponder(_:)` call, carrying the window.
let ActriumFirstResponderDidChangeNotification = Notification.Name(
    "ActriumFirstResponderDidChangeNotification"
)

extension NSWindow {
    /// Swizzled alongside the original `makeFirstResponder(_:)`; after the
    /// real implementation runs, surfaces report text-input focus changes to
    /// the Host so it can route IME composition there.
    @objc func actrium_makeFirstResponder(_ responder: NSResponder?) -> Bool {
        // JS focus() commands target RN's wrapper views (RCTWrappedTextView),
        // which are plain NSViews forwarding responder calls to an inner
        // RCTUITextView. AppKit only draws the caret and runs key handling
        // when the text view itself is first responder, so redirect the
        // focus to the inner text view — the same target click-to-focus uses.
        let target = responder.map { actrium_innerTextViewTarget($0) } ?? responder
        let accepted = actrium_makeFirstResponder(target)
        NotificationCenter.default.post(
            name: ActriumFirstResponderDidChangeNotification,
            object: self
        )
        return accepted
    }

    static func actrium_installFirstResponderHook() {
        let original = class_getInstanceMethod(NSWindow.self, #selector(makeFirstResponder(_:)))
        let swizzled = class_getInstanceMethod(NSWindow.self, #selector(actrium_makeFirstResponder(_:)))
        guard let original, let swizzled else { return }
        method_exchangeImplementations(original, swizzled)
    }
}

/// Resolves the responder a `makeFirstResponder` call should really target.
/// RN macOS multiline inputs wrap their `RCTUITextView` in a plain NSView
/// (`RCTWrappedTextView`); when that wrapper is passed in, return the inner
/// editable text view instead so AppKit treats it as the real first responder.
/// Only the wrapper itself is unwrapped — an arbitrary container (e.g. the
/// surface root during an explicit blur) must keep its own focus behavior.
private func actrium_innerTextViewTarget(_ responder: NSResponder) -> NSResponder {
    guard let wrapperClass = NSClassFromString("RCTWrappedTextView"),
          let view = responder as? NSView,
          view.isKind(of: wrapperClass)
    else { return responder }
    var queue = [view]
    while !queue.isEmpty {
        let next = queue.removeFirst()
        if let textView = next as? NSTextView, textView.isEditable { return textView }
        queue.append(contentsOf: next.subviews)
    }
    return responder
}

/// Entry-point and lifecycle owner for a manually spawned Renderer Helper.
@MainActor
final class RendererProcessController {
    nonisolated static let rendererIdArgument = "--renderer-id"
    nonisolated static let fingerprintArgument = "--fingerprint"
    nonisolated static let ipcStdinArgument = "--ipc-stdin"
    nonisolated static let hostPidArgument = "--host-pid"
    nonisolated static let attachmentCacheArgument = "--attachment-cache-directory"

    nonisolated static var isRendererProcess: Bool {
        ProcessInfo.processInfo.arguments.contains(rendererIdArgument)
    }

    /// Returns `true` when the caller is running in the Renderer process. In
    /// that case the legacy distributed-notification fallback must not run,
    /// even if the Broker has already disconnected.
    @discardableResult
    nonisolated static func sendHostEvent(
        name: String,
        userInfo: [String: Any] = [:],
        tabId: String = ""
    ) -> Bool {
        guard isRendererProcess else { return false }
        guard JSONSerialization.isValidJSONObject(userInfo),
              let data = try? JSONSerialization.data(withJSONObject: userInfo),
            let json = String(data: data, encoding: .utf8)
        else {
            return true
        }
        // React Native may call from its module queue. Only immutable values
        // cross to the renderer's main-actor lifecycle and callback state.
        DispatchQueue.main.async {
            guard let controller = RendererProcessLifetime.shared.controller else { return }
            do {
                try controller.connection.sendHostEvent(name: name, userInfoJSON: json, tabId: tabId)
            } catch {
                NSLog("ActriumRenderer host event failed: %@ (%@)", name, error.localizedDescription)
            }
        }
        return true
    }

    @discardableResult
    nonisolated static func sendTitleChanged(_ title: String, tabId: String, reloadKey: Int) -> Bool {
        guard isRendererProcess else { return false }
        // React Native may call from its module queue. Only immutable values
        // cross to the renderer's main-actor lifecycle and callback state.
        DispatchQueue.main.async {
            guard let controller = RendererProcessLifetime.shared.controller else { return }
            do {
                try controller.connection.sendTitleChanged(title, tabId: tabId, reloadKey: reloadKey)
            } catch {
                NSLog("ActriumRenderer title change failed: %@", error.localizedDescription)
            }
        }
        return true
    }

    @discardableResult
    nonisolated static func sendConsoleLog(level: String, source: String, message: String) -> Bool {
        guard isRendererProcess else { return false }
        DispatchQueue.main.async {
            guard let controller = RendererProcessLifetime.shared.controller else { return }
            do {
                try controller.connection.sendConsoleLog(
                    level: level,
                    message: message,
                    source: source
                )
            } catch {
                NSLog("ActriumRenderer console forwarding failed: %@", error.localizedDescription)
            }
        }
        return true
    }

    private struct Arguments {
        let rendererId: String
        let fingerprint: String
        let hostPid: Int32
        let attachmentDirectory: URL?

        static func parse(_ values: [String]) -> Arguments? {
            func value(after key: String) -> String? {
                guard let index = values.firstIndex(of: key),
                      values.indices.contains(index + 1) else {
                    return nil
                }
                return values[index + 1]
            }

            guard let rendererId = value(after: rendererIdArgument),
                  let fingerprint = value(after: fingerprintArgument),
                  values.contains(ipcStdinArgument),
                  let hostPidString = value(after: hostPidArgument),
                  let hostPid = Int32(hostPidString) else {
                return nil
            }

            let attachmentPath = value(after: attachmentCacheArgument)
            guard attachmentPath?.hasPrefix("/") != false else { return nil }
            return Arguments(
                rendererId: rendererId,
                fingerprint: fingerprint,
                hostPid: hostPid,
                attachmentDirectory: attachmentPath.map { URL(fileURLWithPath: $0, isDirectory: true) }
            )
        }
    }

    private let arguments: Arguments
    private let connection: IPCHelperConnection
    private var serviceCalls: [UUID: (RealmServiceReply) -> Void] = [:]
    private lazy var localStorage = GuestLocalStorageClient(connection: connection)

    static func callLocalStorage(
        _ call: GuestLocalStorageCall, tabId: String,
        reply: @escaping (GuestLocalStorageReply) -> Void
    ) {
        guard let controller = RendererProcessLifetime.shared.controller,
              controller.surfaces[tabId] != nil else {
            reply(GuestLocalStorageReply(id: call.id, error: "The local storage tab is unavailable."))
            return
        }
        controller.localStorage.send(call, tabId: tabId, reply: reply)
    }

    private var serviceEventSubscriptions: [UUID: (RealmServiceEventReply) -> Void] = [:]
    private var serviceEventTabs: [UUID: String] = [:]
    private var cancelledServiceEventSubscriptions: Set<UUID> = []
    private var fileImports: [String: (id: UUID, reloadKey: Int, reply: (GuestFileImportReply) -> Void)] = [:]
    private lazy var fileExport = GuestFileExportClient(connection: connection)
    private var guestMenus: [String: (call: GuestMenuCall, reply: (GuestMenuReply) -> Void)] = [:]
    private let attachmentStorage: (cache: RendererAttachmentCache, lock: FileHandle)?
    private var attachmentCache: RendererAttachmentCache? { attachmentStorage?.cache }
    private var attachmentPrepareReplies: [UUID: (tabId: String, reply: (RealmServiceAttachmentPrepareReply) -> Void)] = [:]
    private var attachmentChunkReplies: [UUID: (tabId: String, reply: (RealmServiceAttachmentChunkReply) -> Void)] = [:]
    private var attachmentUnregisterReplies: [UUID: (tabId: String, reply: (RealmServiceAttachmentUnregisterReply) -> Void)] = [:]
    private var attachmentAvailabilityReplies: [UUID: (tabId: String, reply: (RealmServiceAttachmentAvailabilityReply) -> Void)] = [:]

    static func showGuestMenu(
        _ call: GuestMenuCall, tabId: String, reply: @escaping (GuestMenuReply) -> Void
    ) {
        guard let controller = RendererProcessLifetime.shared.controller,
              controller.surfaces[tabId] != nil, controller.guestMenus[tabId] == nil else {
            reply(GuestMenuReply(id: call.id, error: "The tab cannot open a menu."))
            return
        }
        do {
            let encoder = JSONEncoder()
            encoder.outputFormatting = .withoutEscapingSlashes
            let data = try encoder.encode(call)
            guard data.count <= GuestMenuCall.maxEncodedRequestBytes else {
                reply(GuestMenuReply(id: call.id, error: "The menu request is too large."))
                return
            }
            try controller.connection.sendHostEvent(
                name: GuestMenuCall.requestEvent, userInfoJSON: String(decoding: data, as: UTF8.self), tabId: tabId
            )
            controller.guestMenus[tabId] = (call, reply)
        } catch {
            reply(GuestMenuReply(id: call.id, error: "Could not contact the menu host."))
        }
    }

    static func attachmentCache(for tabId: String) throws -> RendererAttachmentCache {
        guard let controller = RendererProcessLifetime.shared.controller, controller.surfaces[tabId] != nil,
              let cache = controller.attachmentCache else {
            throw RendererAttachmentCache.Failure.unavailable
        }
        return cache
    }

    static func pickFiles(
        _ call: GuestFileImportCall, tabId: String, reply: @escaping (GuestFileImportReply) -> Void
    ) {
        guard let controller = RendererProcessLifetime.shared.controller,
              controller.surfaces[tabId] != nil, controller.fileImports[tabId] == nil else {
            reply(GuestFileImportReply(id: call.id, error: "The picker tab is unavailable or already selecting files."))
            return
        }
        do {
            let data = try JSONEncoder().encode(call)
            try controller.connection.sendHostEvent(
                name: GuestFileImportCall.requestEvent, userInfoJSON: String(decoding: data, as: UTF8.self), tabId: tabId
            )
            controller.fileImports[tabId] = (call.id, call.reloadKey, reply)
            let requestId = call.id
            DispatchQueue.main.asyncAfter(deadline: .now() + .seconds(GuestFileImportCall.timeoutSeconds)) {
                [weak controller] in
                guard let controller, controller.fileImports[tabId]?.id == requestId else { return }
                controller.cancelFileImport(tabId, message: "The file selection timed out.")
            }
        } catch {
            reply(GuestFileImportReply(id: call.id, error: "Could not contact the file picker."))
        }
    }

    static func saveFile(
        _ call: GuestFileExportCall, tabId: String, reply: @escaping (GuestFileExportReply) -> Void
    ) {
        guard let controller = RendererProcessLifetime.shared.controller,
              controller.surfaces[tabId] != nil else {
            reply(GuestFileExportReply(id: call.id, error: "The export tab is unavailable."))
            return
        }
        controller.fileExport.send(call, tabId: tabId, reply: reply)
    }

    static func callRealmService(
        tabId: String,
        call: RealmServiceCall,
        reply: @escaping (RealmServiceReply) -> Void
    ) {
        guard let controller = RendererProcessLifetime.shared.controller,
              controller.surfaces[tabId] != nil else {
            let failure = RealmServiceFailure(code: "realm_tab_unavailable", message: "The Realm tab is unavailable.")
            reply(RealmServiceReply(id: call.id, failure: failure))
            return
        }
        do {
            let encoder = JSONEncoder()
            encoder.outputFormatting = .withoutEscapingSlashes
            let data = try encoder.encode(call)
            try controller.connection.sendHostEvent(
                name: RealmServiceCall.requestEvent,
                userInfoJSON: String(decoding: data, as: UTF8.self),
                tabId: tabId
            )
            controller.serviceCalls[call.id] = reply
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(call.timeoutMs)) { [weak controller] in
                let failure = RealmServiceFailure(
                    code: "realm_service_timeout", message: "The Service call timed out."
                )
                controller?.serviceCalls.removeValue(forKey: call.id)?(RealmServiceReply(id: call.id, failure: failure))
            }
        } catch {
            reply(RealmServiceReply(id: call.id, failure: RealmServiceFailure(error: error)))
        }
    }
    static func subscribeRealmServiceEvents(
        tabId: String,
        subscription: RealmServiceEventSubscription,
        reply: @escaping (RealmServiceEventReply) -> Void
    ) {
        guard let controller = RendererProcessLifetime.shared.controller,
              controller.surfaces[tabId] != nil else {
            reply(RealmServiceEventReply(id: subscription.id, failure: RealmServiceFailure(
                code: "realm_tab_unavailable", message: "The Realm tab is unavailable."
            )))
            return
        }
        do {
            let data = try JSONEncoder().encode(subscription)
            controller.serviceEventSubscriptions[subscription.id] = reply
            controller.serviceEventTabs[subscription.id] = tabId
            try controller.connection.sendHostEvent(
                name: RealmServiceEventSubscription.requestEvent,
                userInfoJSON: String(decoding: data, as: UTF8.self),
                tabId: tabId
            )
            DispatchQueue.main.asyncAfter(deadline: .now() + .seconds(30)) { [weak controller] in
                guard let pending = controller?.serviceEventSubscriptions.removeValue(forKey: subscription.id) else { return }
                controller?.serviceEventTabs.removeValue(forKey: subscription.id)
                controller?.cancelledServiceEventSubscriptions.remove(subscription.id)
                RendererProcessController.unsubscribeRealmServiceEvents(
                    tabId: tabId, subscriptionId: subscription.id
                )
                pending(RealmServiceEventReply(id: subscription.id, failure: RealmServiceFailure(
                    code: "realm_service_event_timeout", message: "The Service event subscription timed out."
                )))
            }
        } catch {
            reply(RealmServiceEventReply(id: subscription.id, failure: RealmServiceFailure(error: error)))
        }
    }

    static func unsubscribeRealmServiceEvents(tabId: String, subscriptionId: UUID) {
        guard let controller = RendererProcessLifetime.shared.controller else { return }
        // Keep only an in-flight reply for the registration/cancellation race.
        // Established subscriptions can remove their renderer mapping now.
        if controller.serviceEventSubscriptions[subscriptionId] != nil {
            controller.cancelledServiceEventSubscriptions.insert(subscriptionId)
            controller.serviceEventTabs[subscriptionId] = tabId
        } else {
            controller.serviceEventTabs.removeValue(forKey: subscriptionId)
            controller.cancelledServiceEventSubscriptions.remove(subscriptionId)
        }
        let subscription = RealmServiceEventSubscription(id: subscriptionId, events: [])
        guard let data = try? JSONEncoder().encode(subscription) else { return }
        try? controller.connection.sendHostEvent(
            name: RealmServiceEventSubscription.cancelEvent,
            userInfoJSON: String(decoding: data, as: UTF8.self),
            tabId: tabId
        )
    }


    // MARK: - Attachment upload transport

    /// The Host RPC budget is 20 seconds; add IPC slack so a slow-but-honest
    /// Host answer still wins over the local timeout.
    nonisolated static let attachmentPrepareTimeoutMs = 25_000
    nonisolated static let attachmentChunkTimeoutMs = 30_000
    nonisolated static let attachmentUnregisterTimeoutMs = 10_000
    nonisolated static let attachmentAvailabilityTimeoutMs = 5_000

    /// Queries the Host whether the owning tab has a usable Realm Service
    /// upload binding. Fails closed on every transport or timeout error.
    static func attachmentUploadAvailable(
        tabId: String,
        call: RealmServiceAttachmentAvailabilityCall,
        reply: @escaping (RealmServiceAttachmentAvailabilityReply) -> Void
    ) {
        guard let controller = RendererProcessLifetime.shared.controller,
              controller.surfaces[tabId] != nil else {
            reply(RealmServiceAttachmentAvailabilityReply(id: call.id, available: false))
            return
        }
        do {
            let data = try JSONEncoder().encode(call)
            try controller.connection.sendHostEvent(
                name: RealmServiceAttachmentAvailabilityCall.requestEvent,
                userInfoJSON: String(decoding: data, as: UTF8.self),
                tabId: tabId
            )
            controller.attachmentAvailabilityReplies[call.id] = (tabId, reply)
            let requestId = call.id
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(attachmentAvailabilityTimeoutMs)) { [weak controller] in
                controller?.attachmentAvailabilityReplies.removeValue(forKey: requestId)?
                    .reply(RealmServiceAttachmentAvailabilityReply(id: requestId, available: false))
            }
        } catch {
            reply(RealmServiceAttachmentAvailabilityReply(id: call.id, available: false))
        }
    }

    static func prepareAttachmentUpload(
        tabId: String,
        call: RealmServiceAttachmentPrepareCall,
        reply: @escaping (RealmServiceAttachmentPrepareReply) -> Void
    ) {
        guard let controller = RendererProcessLifetime.shared.controller,
              controller.surfaces[tabId] != nil else {
            reply(RealmServiceAttachmentPrepareReply(
                id: call.id,
                failure: RealmServiceFailure(code: "realm_tab_unavailable", message: "The Realm tab is unavailable.")
            ))
            return
        }
        do {
            let data = try JSONEncoder().encode(call)
            try controller.connection.sendHostEvent(
                name: RealmServiceAttachmentPrepareCall.requestEvent,
                userInfoJSON: String(decoding: data, as: UTF8.self),
                tabId: tabId
            )
            controller.attachmentPrepareReplies[call.id] = (tabId, reply)
            let requestId = call.id
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(attachmentPrepareTimeoutMs)) { [weak controller] in
                let failure = RealmServiceFailure(
                    code: "realm_attachment_timeout", message: "The attachment prepare timed out."
                )
                controller?.attachmentPrepareReplies.removeValue(forKey: requestId)?
                    .reply(RealmServiceAttachmentPrepareReply(id: requestId, failure: failure))
            }
        } catch {
            reply(RealmServiceAttachmentPrepareReply(id: call.id, failure: RealmServiceFailure(error: error)))
        }
    }

    static func sendAttachmentChunk(
        tabId: String,
        call: RealmServiceAttachmentChunkCall,
        reply: @escaping (RealmServiceAttachmentChunkReply) -> Void
    ) {
        guard let controller = RendererProcessLifetime.shared.controller,
              controller.surfaces[tabId] != nil else {
            reply(RealmServiceAttachmentChunkReply(
                id: call.id,
                failure: RealmServiceFailure(code: "realm_tab_unavailable", message: "The Realm tab is unavailable.")
            ))
            return
        }
        do {
            let data = try JSONEncoder().encode(call)
            try controller.connection.sendHostEvent(
                name: RealmServiceAttachmentChunkCall.requestEvent,
                userInfoJSON: String(decoding: data, as: UTF8.self),
                tabId: tabId
            )
            controller.attachmentChunkReplies[call.id] = (tabId, reply)
            let requestId = call.id
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(attachmentChunkTimeoutMs)) { [weak controller] in
                let failure = RealmServiceFailure(
                    code: "realm_attachment_timeout", message: "The attachment chunk timed out."
                )
                controller?.attachmentChunkReplies.removeValue(forKey: requestId)?
                    .reply(RealmServiceAttachmentChunkReply(id: requestId, failure: failure))
            }
        } catch {
            reply(RealmServiceAttachmentChunkReply(id: call.id, failure: RealmServiceFailure(error: error)))
        }
    }

    static func unregisterAttachmentStream(
        tabId: String,
        call: RealmServiceAttachmentUnregisterCall,
        reply: @escaping (RealmServiceAttachmentUnregisterReply) -> Void
    ) {
        guard let controller = RendererProcessLifetime.shared.controller,
              controller.surfaces[tabId] != nil else {
            reply(RealmServiceAttachmentUnregisterReply(
                id: call.id,
                failure: RealmServiceFailure(code: "realm_tab_unavailable", message: "The Realm tab is unavailable.")
            ))
            return
        }
        do {
            let data = try JSONEncoder().encode(call)
            try controller.connection.sendHostEvent(
                name: RealmServiceAttachmentUnregisterCall.requestEvent,
                userInfoJSON: String(decoding: data, as: UTF8.self),
                tabId: tabId
            )
            controller.attachmentUnregisterReplies[call.id] = (tabId, reply)
            let requestId = call.id
            DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(attachmentUnregisterTimeoutMs)) { [weak controller] in
                let failure = RealmServiceFailure(
                    code: "realm_attachment_timeout", message: "The attachment unregister timed out."
                )
                controller?.attachmentUnregisterReplies.removeValue(forKey: requestId)?
                    .reply(RealmServiceAttachmentUnregisterReply(id: requestId, failure: failure))
            }
        } catch {
            reply(RealmServiceAttachmentUnregisterReply(id: call.id, failure: RealmServiceFailure(error: error)))
        }
    }
    private let reactNativeHost = ServiceReactNativeHost.shared
    private var surfaces: [String: RendererTabSurface] = [:]
    private var isTerminating = false

    private init(arguments: Arguments) {
        self.arguments = arguments
        attachmentStorage = arguments.attachmentDirectory.flatMap { directory in
            do {
                let lock = try RendererAttachmentDirectory.acquireRendererLock(for: directory)
                return (RendererAttachmentCache(directory: directory), lock)
            } catch {
                NSLog("Could not lock the renderer attachment cache: %@", error.localizedDescription)
                return nil
            }
        }
        installRNMarkedTextGate()
        connection = IPCHelperConnection(
            tabId: arguments.rendererId,
            connectedSocket: STDIN_FILENO
        )
        configureIPC()
    }

    static func runFromCommandLine() -> Never {
        guard let arguments = Arguments.parse(ProcessInfo.processInfo.arguments) else {
            fputs("ActriumRenderer: missing required Broker arguments\n", stderr)
            exit(64)
        }

        let application = NSApplication.shared
        application.setActivationPolicy(.prohibited)
        application.finishLaunching()

        let controller = RendererProcessController(arguments: arguments)
        RendererProcessLifetime.shared.controller = controller

        do {
            try controller.connection.connect()
            try controller.connection.sendReady(
                surfaceId: 0,
                width: 0,
                height: 0,
                rendererId: arguments.rendererId
            )
        } catch {
            fputs("ActriumRenderer: Broker connection failed: \(error)\n", stderr)
            exit(70)
        }

        NSLog(
            "ActriumRenderer ready: renderer=%@ fingerprint=%@ pid=%d host=%d",
            arguments.rendererId,
            arguments.fingerprint,
            ProcessInfo.processInfo.processIdentifier,
            arguments.hostPid
        )
        application.run()
        exit(0)
    }

    private func configureIPC() {
        NSWindow.actrium_installFirstResponderHook()
        NotificationCenter.default.addObserver(
            forName: ActriumFirstResponderDidChangeNotification,
            object: nil,
            queue: .main
        ) { [weak self] note in
            guard let self, let window = note.object as? NSWindow else { return }
            for (tabId, surface) in self.surfaces where surface.window === window {
                // Focus moved: an interrupted composition must not leave its
                // component in the composing gate forever.
                surface.pruneComposingState()
                self.reportTextInputFocus(tabId: tabId, surface: surface)
            }
        }
        connection.onHostEvent = { [weak self] name, json, tabId in
            guard let self else { return }
            if name == GuestLocalStorageCall.responseEvent {
                localStorage.receive(json: json, tabId: tabId)
                return
            }
            if name == GuestFileImportCall.responseEvent {
                receiveFileImport(json, tabId: tabId)
                return
            }
            if name == GuestFileExportCall.responseEvent {
                fileExport.receive(json: json, tabId: tabId)
                return
            }
            if name == GuestMenuCall.responseEvent {
                receiveGuestMenu(json, tabId: tabId)
                return
            }
            if name == "com.actrium.rendererKeyboardFocus" {
                guard let focused = Bool(json) else { return }
                // Apply focus on the same queue as input so the next key
                // event can reach the restored editor.
                DispatchQueue.main.async {
                    self.surfaces[tabId]?.setKeyboardFocus(focused)
                }
                return
            }
            if name == RealmServiceCall.responseEvent {
                guard let response = try? JSONDecoder().decode(RealmServiceReply.self, from: Data(json.utf8)) else {
                    return
                }
                self.serviceCalls.removeValue(forKey: response.id)?(response)
                return
            }
            if name == RealmServiceEventSubscription.responseEvent,
               let response = try? JSONDecoder().decode(RealmServiceEventReply.self, from: Data(json.utf8)) {
                let tabId = self.serviceEventTabs[response.id]
                let cancelled = self.cancelledServiceEventSubscriptions.remove(response.id) != nil
                self.serviceEventSubscriptions.removeValue(forKey: response.id)?(response)
                if cancelled && response.failure == nil, let tabId {
                    RendererProcessController.unsubscribeRealmServiceEvents(
                        tabId: tabId, subscriptionId: response.id
                    )
                }
                if cancelled || response.failure != nil {
                    self.serviceEventTabs.removeValue(forKey: response.id)
                    self.cancelledServiceEventSubscriptions.remove(response.id)
                }
                return
            }
            if name == RealmServiceEventSubscription.eventEvent,
               let event = try? JSONDecoder().decode(RealmServiceEvent.self, from: Data(json.utf8)),
               let tabId = self.serviceEventTabs[event.subscriptionId] {
                RealmService.emit(event, tabId: tabId)
                return
            }
            if name == RealmServiceAttachmentPrepareCall.responseEvent {
                guard let response = try? JSONDecoder().decode(RealmServiceAttachmentPrepareReply.self, from: Data(json.utf8)) else {
                    return
                }
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.attachmentPrepareReplies[response.id]?.tabId == tabId else { return }
                    self.attachmentPrepareReplies.removeValue(forKey: response.id)?.reply(response)
                }
                return
            }
            if name == RealmServiceAttachmentChunkCall.responseEvent {
                guard let response = try? JSONDecoder().decode(RealmServiceAttachmentChunkReply.self, from: Data(json.utf8)) else {
                    return
                }
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.attachmentChunkReplies[response.id]?.tabId == tabId else { return }
                    self.attachmentChunkReplies.removeValue(forKey: response.id)?.reply(response)
                }
                return
            }
            if name == RealmServiceAttachmentUnregisterCall.responseEvent {
                guard let response = try? JSONDecoder().decode(RealmServiceAttachmentUnregisterReply.self, from: Data(json.utf8)) else {
                    return
                }
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.attachmentUnregisterReplies[response.id]?.tabId == tabId else { return }
                    self.attachmentUnregisterReplies.removeValue(forKey: response.id)?.reply(response)
                }
                return
            }
            if name == RealmServiceAttachmentAvailabilityCall.responseEvent {
                guard let response = try? JSONDecoder().decode(RealmServiceAttachmentAvailabilityReply.self, from: Data(json.utf8)) else {
                    return
                }
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.attachmentAvailabilityReplies[response.id]?.tabId == tabId else { return }
                    self.attachmentAvailabilityReplies.removeValue(forKey: response.id)?.reply(response)
                }
                return
            }
            if name == RealmServiceAttachmentResultEvent.event {
                guard let event = try? JSONDecoder().decode(RealmServiceAttachmentResultEvent.self, from: Data(json.utf8)) else {
                    return
                }
                DispatchQueue.main.async {
                    ServiceReactNativeHost.shared.emitAttachmentUploadResult(tabId: tabId, event: event)
                }
                return
            }
            // IME composition runs in the Host process (TSM never engages in
            // this background, activation-prohibited process); the Host
            // mirrors marked/committed text back for the focused text input.
            DispatchQueue.main.async {
                switch name {
                case "com.actrium.imeMarkedText":
                    guard let op = try? JSONDecoder().decode(IMETextPayload.self, from: Data(json.utf8)) else { return }
                    self.surfaces[tabId]?.applyMarkedText(op.text, selectedRange: NSRange(location: op.loc, length: op.len))
                case "com.actrium.imeInsertText":
                    guard let op = try? JSONDecoder().decode(IMETextPayload.self, from: Data(json.utf8)) else { return }
                    self.surfaces[tabId]?.applyInsertText(op.text)
                case "com.actrium.textInputBlurRequest":
                    self.surfaces[tabId]?.resignTextInputFocus()
                case "com.actrium.documentSelectionCommand":
                    guard let payload = try? JSONDecoder().decode(IMECommandPayload.self, from: Data(json.utf8)) else { return }
                    self.surfaces[tabId]?.performDocumentSelectionCommand(payload.command)
                case "com.actrium.textCommand":
                    guard let payload = try? JSONDecoder().decode(IMECommandPayload.self, from: Data(json.utf8)) else { return }
                    self.surfaces[tabId]?.performTextCommand(payload.command)
                case "com.actrium.textInputFocusReset":
                    // The Host dropped our pre-interaction focus report and
                    // its user-interaction gate just opened; force a fresh
                    // report so IME can activate on the already-focused field.
                    if let surface = self.surfaces[tabId] {
                        self.reportTextInputFocus(tabId: tabId, surface: surface, force: true)
                    }
                default:
                    break
                }
            }
        }

        connection.onLoadContent = { [weak self] request, reply in
            DispatchQueue.main.async {
                self?.loadContent(request, reply: reply)
            }
        }

        connection.onTabUpdateViewport = { [weak self] tabId, width, height, requestId, overview in
            DispatchQueue.main.async {
                self?.updateViewport(
                    tabId: tabId,
                    size: CGSize(width: width, height: height),
                    requestId: requestId,
                    isOverviewMode: overview
                )
            }
        }

        connection.onInputEvent = { [weak self] event in
            DispatchQueue.main.async {
                self?.surfaces[event.tabID]?.dispatch(event)
            }
        }

        connection.onTabGetSurfaceID = { [weak self] tabId, reply in
            DispatchQueue.main.async {
                reply(self?.surfaces[tabId]?.contextId ?? 0)
            }
        }

        connection.onDestroyTab = { [weak self] tabId in
            DispatchQueue.main.async {
                self?.destroyTab(tabId)
            }
        }

        connection.onGetAccessibilityTree = { [weak self] tabId, reply in
            DispatchQueue.main.async {
                reply(self?.surfaces[tabId]?.accessibilityTree() ?? Actrium_Ipc_AccessibilityTree())
            }
        }

        connection.onAccessibilityAction = { [weak self] request, reply in
            DispatchQueue.main.async {
                let ok = self?.surfaces[request.tabID]?.performAccessibility(
                    action: request.action,
                    nodeId: request.nodeID
                ) ?? false
                if ok {
                    self?.publishAccessibilityTree(tabId: request.tabID)
                }
                reply(ok, ok ? nil : "Accessibility action failed")
            }
        }

        connection.onGetMetrics = { reply in
            reply(RendererProcessController.metrics())
        }

        connection.onTabExecuteJS = { _, _, reply in
            reply(false, nil, "Renderer evaluateScript is not implemented")
        }

        connection.onTerminate = { [weak self] reason in
            DispatchQueue.main.async {
                self?.terminate(reason: reason)
            }
        }

        connection.onDisconnected = { [weak self] in
            DispatchQueue.main.async {
                self?.terminate(reason: "Broker disconnected")
            }
        }
    }

    private func loadContent(
        _ request: Actrium_Ipc_LoadContentRequest,
        reply: @escaping (Bool, String?) -> Void
    ) {
        let tabId = request.tabID
        guard !tabId.isEmpty else {
            reply(false, "Missing tabId")
            return
        }

        var config: [String: Any] = [:]
        if let data = request.configJson.data(using: .utf8),
           let decoded = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {
            config = decoded
        }

        if let pending = fileImports[tabId], pending.reloadKey != Int(request.reloadKey) {
            cancelFileImport(tabId)
        }
        if let pending = guestMenus[tabId], pending.call.reloadKey != Int(request.reloadKey) {
            cancelGuestMenu(tabId)
        }
        attachmentCache?.load(tabId: tabId, reloadKey: Int(request.reloadKey))
        config["tabId"] = tabId
        if config["title"] == nil {
            config["title"] = request.title
        }
        if config["backgroundColor"] == nil {
            config["backgroundColor"] = request.backgroundColor
        }
        if config["unifiedScript"] == nil, request.contentType == "javascript" {
            config["unifiedScript"] = request.content
        }

        let width = (config["containerWidth"] as? NSNumber)?.doubleValue ?? 1280
        let height = (config["containerHeight"] as? NSNumber)?.doubleValue ?? 720
        let size = CGSize(width: max(width, 1), height: max(height, 1))

        let surface: RendererTabSurface
        if let existing = surfaces[tabId] {
            surface = existing
            surface.resize(to: size)
        } else {
            let entry = reactNativeHost.createRendererEntry(tabId: tabId, size: size)
            guard let created = RendererTabSurface(tabId: tabId, entry: entry, size: size) else {
                reply(false, "Failed to create remote CoreAnimation context")
                return
            }
            surfaces[tabId] = created
            surface = created
        }

        reactNativeHost.applyRendererConfig(config) { [weak self, weak surface] success, error in
            guard let self, let surface else {
                reply(false, "Renderer surface released")
                return
            }

            if success {
                surface.commit()
                self.publishAccessibilityTree(tabId: tabId)
                try? self.connection.sendSurfaceUpdated(
                    surfaceId: surface.contextId,
                    tabId: tabId
                )
                NSLog(
                    "Renderer surface ready: renderer=%@ tab=%@ context=%u",
                    self.arguments.rendererId,
                    tabId,
                    surface.contextId
                )
            }
            reply(success, error)
        }
    }

    private func updateViewport(
        tabId: String,
        size: CGSize,
        requestId: UInt32,
        isOverviewMode: Bool
    ) {
        guard let surface = surfaces[tabId],
              size.width > 0,
              size.height > 0 else {
            return
        }

        surface.resize(to: size)
        reactNativeHost.resizeRendererEntry(
            tabId: tabId,
            size: size,
            requestId: requestId,
            isOverviewMode: isOverviewMode
        )
        surface.commit()
        try? connection.sendViewportApplied(
            tabId: tabId,
            requestId: requestId,
            width: size.width,
            height: size.height
        )
        publishAccessibilityTree(tabId: tabId)
    }

    /// Called from the Fabric surface-presenter observer after mount
    /// instructions are applied. Re-commits the matching tab surface so the
    /// new CAContext frame reaches the Host instead of waiting for a resize.
    func commitSurface(forRootTag rootTag: Int) {
        let surfaceSelector = NSSelectorFromString("surface")
        let rootTagSelector = NSSelectorFromString("rootTag")
        for surface in surfaces.values {
            guard let rootView = surface.entry.rootView else { continue }
            let rootObject = rootView as AnyObject
            guard rootObject.responds(to: surfaceSelector) else { continue }
            guard let surfaceObject = rootObject.value(forKey: "surface") as AnyObject?,
                  surfaceObject.responds(to: rootTagSelector) else { continue }
            let tagValue = surfaceObject.value(forKey: "rootTag")
            let tag = (tagValue as? NSNumber).map { Int(truncating: $0) } ?? (tagValue as? Int)
            if tag == rootTag {
                surface.commit()
            }
        }
    }

    /// Forwards the surface caret rect to the Host; nil hides the candidate
    /// window anchor. Transport reuses the generic HostEvent channel.
    /// Reports the surface's text-input focus state to the Host, unless it is
    /// unchanged since the last report. `force` bypasses the dedupe latch.
    private func reportTextInputFocus(tabId: String, surface: RendererTabSurface, force: Bool = false) {
        if force {
            surface.resetTextInputFocusDedupe()
        }
        surface.reportTextInputFocusIfChanged { focused in
            try self.connection.sendHostEvent(
                name: "com.actrium.textInputFocus",
                userInfoJSON: "{\"focused\":\(focused)}",
                tabId: tabId
            )
        }
    }

    func publishCaret(tabId: String, rect: CGRect?) {
        guard let encoded = try? JSONEncoder().encode(rect) else { return }
        try? connection.sendHostEvent(
            name: "com.actrium.rendererCaret",
            userInfoJSON: String(decoding: encoded, as: UTF8.self),
            tabId: tabId
        )
    }

    func publishAccessibilityTree(tabId: String) {
        guard let surface = surfaces[tabId] else { return }
        try? connection.sendAccessibilityTree(surface.accessibilityTree())
    }

    private func destroyTab(_ tabId: String) {
        cancelFileImport(tabId)
        fileExport.close(tabId: tabId)
        cancelGuestMenu(tabId)
        attachmentCache?.close(tabId: tabId)
        failPendingAttachmentCalls(tabId: tabId)
        if let surface = surfaces.removeValue(forKey: tabId) {
            // Before the hierarchy is detached: afterwards backing.window is
            // nil and no prune can reach the tab's composition entries, while
            // a recycled Fabric input could carry them into a new editor.
            surface.invalidateComposingState()
            surface.close()
        }
        reactNativeHost.destroyRendererEntry(tabId: tabId)
    }

    /// Rejects every attachment Promise still waiting on this tab so JS
    /// never hangs when the surface disappears mid-upload.
    private func failPendingAttachmentCalls(tabId: String) {
        let failure = RealmServiceFailure(code: "realm_tab_closed", message: "The tab was closed.")
        for (id, pending) in attachmentPrepareReplies where pending.tabId == tabId {
            attachmentPrepareReplies.removeValue(forKey: id)?
                .reply(RealmServiceAttachmentPrepareReply(id: id, failure: failure))
        }
        for (id, pending) in attachmentChunkReplies where pending.tabId == tabId {
            attachmentChunkReplies.removeValue(forKey: id)?
                .reply(RealmServiceAttachmentChunkReply(id: id, failure: failure))
        }
        for (id, pending) in attachmentUnregisterReplies where pending.tabId == tabId {
            attachmentUnregisterReplies.removeValue(forKey: id)?
                .reply(RealmServiceAttachmentUnregisterReply(id: id, failure: failure))
        }
        for (id, pending) in attachmentAvailabilityReplies where pending.tabId == tabId {
            attachmentAvailabilityReplies.removeValue(forKey: id)?
                .reply(RealmServiceAttachmentAvailabilityReply(id: id, available: false))
        }
    }

    private func cancelGuestMenu(_ tabId: String) {
        guard let pending = guestMenus.removeValue(forKey: tabId) else { return }
        pending.reply(GuestMenuReply(id: pending.call.id))
    }

    private func receiveGuestMenu(_ json: String, tabId: String) {
        guard let response = try? JSONDecoder().decode(GuestMenuReply.self, from: Data(json.utf8)),
              guestMenus[tabId]?.call.id == response.id else { return }
        guestMenus.removeValue(forKey: tabId)?.reply(response)
    }

    private func cancelFileImport(_ tabId: String, message: String = "The picker document changed or closed.") {
        if let pending = fileImports.removeValue(forKey: tabId) {
            pending.reply(GuestFileImportReply(id: pending.id, error: message))
        }
    }

    private func receiveFileImport(_ json: String, tabId: String) {
        guard let response = try? JSONDecoder().decode(GuestFileImportReply.self, from: Data(json.utf8)),
              fileImports[tabId]?.id == response.id else { return }
        fileImports.removeValue(forKey: tabId)?.reply(response)
    }

    private func terminate(reason: String) {
        guard !isTerminating else { return }
        isTerminating = true
        NSLog("ActriumRenderer terminating: %@", reason)
        let tabIds = Array(surfaces.keys)
        for tabId in tabIds {
            destroyTab(tabId)
        }
        connection.disconnect()
        Task { @MainActor in
            await attachmentCache?.finishClosing()
            NSApplication.shared.terminate(nil)
        }
    }

    private static func metrics() -> Actrium_Ipc_MetricsResponse {
        var response = Actrium_Ipc_MetricsResponse()
        var usage = rusage()
        if getrusage(RUSAGE_SELF, &usage) == 0 {
            #if arch(arm64)
            response.memoryUsage = UInt64(max(usage.ru_maxrss, 0))
            #else
            response.memoryUsage = UInt64(max(usage.ru_maxrss, 0)) * 1024
            #endif
        }
        return response
    }
}

/// Keeps the top-level controller alive for the lifetime of NSApplication.
@MainActor
private final class RendererProcessLifetime {
    static let shared = RendererProcessLifetime()
    var controller: RendererProcessController?
}

/// Fabric mount observer: commit the matching Renderer surface after the
/// native mount instructions finish, so reload content is published without
/// waiting for a window resize.
final class RendererSurfaceMountObserver: NSObject {
    static let shared = RendererSurfaceMountObserver()

    @objc(didMountComponentsWithRootTag:)
    func didMountComponents(rootTag: Int) {
        Task { @MainActor in
            RendererProcessLifetime.shared.controller?.commitSurface(
                forRootTag: rootTag
            )
        }
    }
}

/// Renderer-side state machine for text-selection gestures. AppKit's native
/// mouse tracking expects the drag and mouse-up in the local event queue, but
/// they arrive asynchronously over IPC, so press, drag and release are driven
/// here. One object owns both the active gesture and the anchor a later Shift
/// gesture extends from, which keeps the invalidation rules in one place.
private final class TextSelectionGesture {
    private enum Phase: Int {
        /// No text-input button is down.
        case idle = 0
        /// The press that starts a gesture has been handled.
        case pressing = 1
        /// A move while the button is down has applied a range.
        case dragging = 2
    }

    private var phase: Phase = .idle
    /// The control this record belongs to, weak so an unmounted control takes
    /// its anchor with it and a fresh one cannot inherit it.
    private weak var owner: AnyObject?
    /// Character index the current selection was built from.
    private(set) var anchor: Int?
    /// The selection this anchor created.
    private(set) var trackedRange: NSRange?

    /// Records the selection a mouse-down or Shift+click just applied.
    func beginPress(owner: AnyObject, anchor: Int, range: NSRange) {
        self.owner = owner
        self.anchor = anchor
        trackedRange = range
        phase = .pressing
    }

    /// Anchors the range a drag move applies. Returns `false` once the drag no
    /// longer belongs to the press that started it. For multi-click drags, pass
    /// the extended anchor so Shift+click after mouse-up extends from the
    /// stationary end of the drag.
    func trackDrag(owner: AnyObject, range: NSRange, anchor: Int? = nil) -> Bool {
        guard phase != .idle, self.owner === owner else { return false }
        trackedRange = range
        if let anchor { self.anchor = anchor }
        phase = .dragging
        return true
    }

    /// The button came up; the anchor stays for a later Shift gesture.
    func endPress() {
        phase = .idle
    }

    /// The anchor a Shift+click may extend from. Only the anchor that built
    /// this control's current selection qualifies: an equal range created some
    /// other way (keyboard, a controlled update) does not describe it.
    func shiftAnchor(owner: AnyObject, current: NSRange) -> Int? {
        guard self.owner === owner, trackedRange == current else { return nil }
        return anchor
    }

    /// A selection change the gesture did not make retires the anchor, so a
    /// keyboard-built selection or a controlled update cannot be extended from
    /// a stale base. Changes applied by the gesture itself are ignored through
    /// `phase`.
    func noteSelectionChange(owner: AnyObject, current: NSRange) {
        guard phase == .idle, self.owner === owner else { return }
        guard trackedRange != current else { return }
        invalidate()
    }

    /// Invalidates the gesture only if the given owner matches the current
    /// gesture owner and no drag is in progress. Used by text-change observers
    /// to avoid retiring an anchor that belongs to a different control or
    /// interrupting an active drag.
    func invalidateIfOwned(by owner: AnyObject) {
        guard phase == .idle, self.owner === owner else { return }
        invalidate()
    }

    func invalidate() {
        phase = .idle
        owner = nil
        anchor = nil
        trackedRange = nil
    }
}

/// One Tab root and its remotely hosted CoreAnimation context.
@MainActor
private final class RendererTabSurface {

    let tabId: String
    let entry: TabViewControllerEntry
    let container: ResizableContainerView
    let window: RendererWindow
    let context: RendererCAContext
    private var generation: UInt64 = 0
    private var hasCommitted = false
    private var accessibilityObservers: [NSObjectProtocol] = []
    private var accessibilityPublishWork: DispatchWorkItem?
    private let mouseTracking = RendererMouseTracking()
    private let hoverTracking = RendererHoverTracking(tracksHover: ActriumRendererTracksHover)
    private var mousePressEventNumbers: [UInt32: Int] = [:]
    private var textInputPresses: Set<UInt32> = []
    /// The active drag and the anchor a later Shift gesture extends from;
    /// every invalidation path funnels through this one object.
    private let selectionGesture = TextSelectionGesture()
    private var selectionObservers: [NSObjectProtocol] = []
    /// Multi-click selection state: the initial range and granularity for
    /// double-click (word) or triple-click (paragraph) drag extension.
    private var multiClickStart: (range: NSRange, granularity: NSSelectionGranularity)?
    /// Last text-input focus state reported to the Host. The Host mirrors
    /// IME composition for whichever surface owns the focused text input.
    private var lastReportedTextFocus = false
    private var lastCursor: String?
    private weak var focusedEditor: NSTextView?
    private var lastCaretRect: CGRect?
    /// Snapshot-based undo/redo for Host-mirrored text commits. AppKit's
    /// undoManager registrations go stale as soon as RN rewrites the text
    /// storage, so undo is implemented by restoring previous snapshots
    /// through the normal input-client path (which RN observes).
    private struct EditSnapshot {
        let string: String
        let caret: Int
    }
    /// Per-control undo history. The window field editor is shared, so a
    /// control's identity comes from the field editor's delegate. History
    /// persists while the surface lives: switching away and back keeps a
    /// control's undo stack, matching AppKit's own behavior.
    private final class UndoHistoryBox {
        var undo: [EditSnapshot] = []
        var redo: [EditSnapshot] = []
    }
    /// Weak-keyed so an unmounted guest control's history disappears with
    /// the control; a fresh control allocated at the same address must not
    /// inherit a stale stack.
    private let undoHistories = NSMapTable<AnyObject, UndoHistoryBox>.weakToStrongObjects()

    /// The identity a control's selection and undo state is keyed on. The
    /// window field editor is shared between fields, so a field is identified
    /// by its delegate rather than the shared editor. Returns nil when the
    /// delegate is unknown, so callers can treat it as "no owner".
    private func control(for editor: NSTextView) -> AnyObject? {
        if editor.isFieldEditor {
            return editor.delegate.map { $0 as AnyObject }
        }
        return editor
    }

    private func currentControl() -> AnyObject? {
        guard let editor = resolvedTextView(window.firstResponder) else { return nil }
        return control(for: editor)
    }

    /// Unwraps RN macOS's `RCTWrappedTextView` container: when JS issues a
    /// `focus()` command AppKit makes the wrapper (a plain NSView) first
    /// responder and forwards responder calls to the inner `RCTUITextView`.
    /// All text handling (edits, undo, key forwarding, focus reporting) must
    /// target that inner text view, so resolve it through the view hierarchy.
    /// Only the wrapper is unwrapped; the surface container must keep its
    /// plain-responder status so an unfocused surface reads as unfocused.
    private func resolvedTextView(_ responder: NSResponder?) -> NSTextView? {
        if let textView = responder as? NSTextView { return textView }
        guard let wrapperClass = NSClassFromString("RCTWrappedTextView"),
              let view = responder as? NSView,
              view.isKind(of: wrapperClass)
        else { return nil }
        var queue = [view]
        while !queue.isEmpty {
            let next = queue.removeFirst()
            if let textView = next as? NSTextView { return textView }
            queue.append(contentsOf: next.subviews)
        }
        return nil
    }

    private func withHistory(_ body: (UndoHistoryBox) -> Void) {
        guard let control = currentControl() else { return }
        let box = undoHistories.object(forKey: control) ?? {
            let created = UndoHistoryBox()
            undoHistories.setObject(created, forKey: control)
            return created
        }()
        body(box)
        if box.undo.count > 100 { box.undo.removeFirst(box.undo.count - 100) }
    }


    var contextId: UInt32 {
        context.contextId
    }

    init?(tabId: String, entry: TabViewControllerEntry, size: CGSize) {
        guard let container = entry.containerView,
              let context = RendererCAContext() else {
            return nil
        }

        self.tabId = tabId
        self.entry = entry
        self.container = container
        self.context = context

        window = RendererWindow(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.borderless],
            backing: .buffered,
            defer: false
        )
        window.isReleasedWhenClosed = false
        window.backgroundColor = .clear
        window.isOpaque = false
        window.acceptsMouseMovedEvents = true
        window.contentView = container
        window.setFrameOrigin(NSPoint(x: -100_000, y: -100_000))
        window.orderOut(nil)

        container.frame = NSRect(origin: .zero, size: size)
        container.wantsLayer = true
        commit()
        observeAccessibility()
        observeTextSelection()
    }

    func accessibilityTree() -> Actrium_Ipc_AccessibilityTree {
        generation += 1
        return RendererAccessibility.snapshot(
            tabId: tabId,
            root: container,
            generation: generation
        )
    }

    func performAccessibility(action: String, nodeId: UInt64) -> Bool {
        RendererAccessibility.perform(action: action, nodeId: nodeId, in: container)
    }

    func resize(to size: CGSize) {
        guard size.width > 0, size.height > 0 else { return }
        window.setContentSize(size)
        container.applyHostSize(size)
        container.layer?.frame = CGRect(origin: .zero, size: size)
    }

    func commit() {
        // The first commit happens on an empty view tree, so lay it out once.
        // Later commits rely on AppKit/RN marking the container dirty
        // (addSubview, surface stage change, resize); Fabric sets frames
        // directly, so a subtree-only dirty state is intentionally not
        // remeasured here.
        if !hasCommitted || container.needsLayout {
            container.needsLayout = true
            container.layoutSubtreeIfNeeded()
            hasCommitted = true
        }
        context.attachWindowLayerTree(for: container)
        CATransaction.flush()
    }

    func setKeyboardFocus(_ focused: Bool) {
        guard window.hasHostKeyboardFocus != focused else { return }
        if !focused {
            ServiceReactNativeHost.shared.sendDocumentSelectionCommand("clear", tabId: tabId)
            mouseTracking.cancel()
            mousePressEventNumbers.removeAll()
            textInputPresses.removeAll()
            // Reset the gesture so a stale phase cannot block anchor retirement.
            selectionGesture.invalidate()
            multiClickStart = nil
        }
        window.setHostKeyboardFocus(focused)
        commit()
    }

    func close() {
        mouseTracking.cancel()
        mousePressEventNumbers.removeAll()
        undoHistories.removeAllObjects()
        accessibilityPublishWork?.cancel()
        accessibilityPublishWork = nil
        for observer in accessibilityObservers {
            NotificationCenter.default.removeObserver(observer)
        }
        accessibilityObservers.removeAll()
        for observer in selectionObservers {
            NotificationCenter.default.removeObserver(observer)
        }
        selectionObservers.removeAll()
        context.layer = nil
        window.contentView = nil
        window.close()
    }

    private func observeAccessibility() {
        let names: [Notification.Name] = [
            Notification.Name(NSAccessibility.Notification.focusedUIElementChanged.rawValue),
            Notification.Name(NSAccessibility.Notification.layoutChanged.rawValue),
            Notification.Name(NSAccessibility.Notification.valueChanged.rawValue),
            Notification.Name(NSAccessibility.Notification.titleChanged.rawValue),
            Notification.Name(NSAccessibility.Notification.selectedChildrenChanged.rawValue)
        ]
        for name in names {
            let observer = NotificationCenter.default.addObserver(
                forName: name,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                // NotificationCenter delivers this observer on OperationQueue.main.
                MainActor.assumeIsolated {
                    guard let self else { return }
                    if let view = notification.object as? NSView, view.window !== self.window {
                        return
                    }
                    self.scheduleAccessibilityPublish()
                }
            }
            accessibilityObservers.append(observer)
        }
    }

    /// Retires a saved selection anchor when the selection or the text changes
    /// behind the gesture's back: a keyboard selection, an independent edit,
    /// or a controlled React Native update replacing the value. Changes the
    /// gesture itself applies are filtered by `TextSelectionGesture`'s phase.
    private func observeTextSelection() {
        let center = NotificationCenter.default
        selectionObservers.append(center.addObserver(
            forName: NSTextView.didChangeSelectionNotification,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            MainActor.assumeIsolated {
                guard let self,
                      let view = notification.object as? NSTextView,
                      view.window === self.window,
                      let owner = self.control(for: view) else { return }
                self.selectionGesture.noteSelectionChange(
                    owner: owner,
                    current: view.selectedRange()
                )
            }
        })
        selectionObservers.append(center.addObserver(
            forName: NSText.didChangeNotification,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            MainActor.assumeIsolated {
                guard let self,
                      let view = notification.object as? NSTextView,
                      view.window === self.window,
                      let owner = self.control(for: view) else { return }
                // Marked text churns during composition; the anchor retires
                // when committed text changes instead.
                guard view.markedRange().length == 0 else { return }
                // Only invalidate if the edit belongs to the gesture's owner,
                // so typing in field B does not retire field A's anchor.
                self.selectionGesture.invalidateIfOwned(by: owner)
            }
        })
    }

    private func scheduleAccessibilityPublish() {
        accessibilityPublishWork?.cancel()
        let work = DispatchWorkItem { [weak self] in
            self?.pushAccessibilityTree()
        }
        accessibilityPublishWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.08, execute: work)
    }

    private func pushAccessibilityTree() {
        guard let controller = RendererProcessLifetime.shared.controller else { return }
        controller.publishAccessibilityTree(tabId: tabId)
    }

    func dispatch(_ input: Actrium_Ipc_InputEvent) {
        let modifiers = modifierFlags(from: input.modifiers)
        // IPC coordinates are top-left based, while AppKit window events use
        // bottom-left based window coordinates.
        let location = container.convert(NSPoint(x: input.x, y: input.y), to: nil)
        let timestamp = ProcessInfo.processInfo.systemUptime

        switch input.type {
        case .mouseDown, .mouseUp, .mouseMove, .mouseDragged:
            guard let type = RendererMouseTracking.eventType(for: input) else { return }
            let originalEventNumber = input.hasEventNumber ? Int(input.eventNumber) : Int(input.button) + 1
            let eventNumber: Int
            if input.type == .mouseDown {
                mousePressEventNumbers[input.button] = originalEventNumber
                eventNumber = originalEventNumber
            } else if input.type == .mouseUp {
                eventNumber = mousePressEventNumbers.removeValue(forKey: input.button) ?? originalEventNumber
            } else if input.type == .mouseDragged {
                // React Native identifies a press by its mouse-down event number.
                eventNumber = mousePressEventNumbers[input.button] ?? originalEventNumber
            } else {
                eventNumber = originalEventNumber
            }
            guard let event = NSEvent.mouseEvent(
                with: type,
                location: location,
                modifierFlags: modifiers,
                timestamp: timestamp,
                windowNumber: window.windowNumber,
                context: nil,
                eventNumber: eventNumber,
                clickCount: input.hasClickCount ? Int(input.clickCount) : (input.type == .mouseMove ? 0 : 1),
                pressure: input.type == .mouseDown || input.type == .mouseDragged ? 1 : 0
            ) else {
                return
            }

            let hitView = hitTest(at: location)
            window.hostMouseLocation = location
            hoverTracking.update(hitView: hitView, windowNumber: window.windowNumber, location: location)
            if input.type == .mouseMove {
                updateCursor(for: hitView)
            }
            // Editable RN controls keep AppKit's native focus path; routing
            // their clicks through the JS responder revokes first responder.
            if input.type == .mouseDown {
                textInputPresses.remove(input.button)
                let ancestors = sequence(first: hitView, next: { $0?.superview })
                    .prefix(while: { $0 != nil }).compactMap { $0 }
                let textInput = ancestors.first { $0 is NSTextView || $0 is NSTextField }
                if input.button == 0, let textInput {
                    mouseTracking.cancel(button: input.button)
                    ServiceReactNativeHost.shared.sendDocumentSelectionCommand("clear", tabId: tabId)
                    textInputPresses.insert(input.button)
                    focusTextInput(textInput, at: location, modifiers: modifiers, clickCount: event.clickCount)
                    // AppKit adds the field editor to the offscreen view
                    // hierarchy on focus. Republish after that update.
                    DispatchQueue.main.async { [weak self] in
                        guard let self, self.window.contentView === self.container else { return }
                        self.commit()
                    }
                    return
                }
                window.makeFirstResponder(hitView ?? container)
                // The click leaves the text input; no anchor survives it.
                selectionGesture.invalidate()
                let recognizers = ancestors.flatMap(\.gestureRecognizers)
                mouseTracking.begin(event, button: input.button, recognizers: recognizers)
            } else if input.type == .mouseDragged, input.button == 0,
                      textInputPresses.contains(0),
                      let anchor = selectionGesture.anchor,
                      let editor = resolvedTextView(window.firstResponder) {
                // Drive drag selection manually (see TextSelectionGesture).
                // The drag belongs to the control it started in; a focus
                // change mid-drag ends it instead of selecting in the new
                // control with a foreign anchor.
                guard let owner = currentControl() else {
                    textInputPresses.remove(input.button)
                    selectionGesture.invalidate()
                    return
                }
                // A controlled input may replace its value while the mouse
                // is down (e.g. RN re-renders with a shorter string); an
                // anchor past the new end must end the drag instead of
                // feeding an out-of-bounds range to AppKit.
                let textLength = (editor.string as NSString).length
                guard anchor <= textLength else {
                    textInputPresses.remove(input.button)
                    selectionGesture.invalidate()
                    return
                }
                let point = editor.convert(location, from: nil)
                let index = editor.characterIndexForInsertion(at: point)
                if index != NSNotFound, index <= textLength {
                    var range = NSRange(
                        location: min(anchor, index),
                        length: abs(index - anchor)
                    )
                    // Extend multi-click selection (double-click word, triple-click paragraph).
                    var extendedAnchor: Int?
                    if let start = multiClickStart, start.range.upperBound <= textLength {
                        let caret = NSRange(location: index, length: 0)
                        let target = editor.selectionRange(forProposedRange: caret, granularity: start.granularity)
                        let extended = extendMultiClickSelection(from: start.range, to: target, at: index)
                        range = extended.range
                        extendedAnchor = extended.anchor
                    }
                    // Keep the stored selection in sync with what the drag
                    // applies, so a later Shift+click still recognises the
                    // anchor as the creator of the current selection.
                    guard selectionGesture.trackDrag(owner: owner, range: range, anchor: extendedAnchor) else {
                        textInputPresses.remove(input.button)
                        selectionGesture.invalidate()
                        return
                    }
                    if editor.selectedRange() != range {
                        editor.setSelectedRange(range)
                        editor.scrollRangeToVisible(range)
                        publishCaret()
                        commit()
                    }
                }
                return
            } else if input.type == .mouseDragged {
                // Without the original press, the drag has no valid gesture target.
                guard mouseTracking.drag(event, button: input.button) else { return }
            } else if input.type == .mouseUp {
                if textInputPresses.remove(input.button) != nil {
                    selectionGesture.endPress()
                    multiClickStart = nil
                    return
                }
                mouseTracking.end(event, button: input.button)
            }
            window.sendEvent(event)

        case .scroll:
            guard let hitView = hitTest(at: location) else { return }
            window.hostMouseLocation = location
            let delta = NSPoint(x: input.scrollDeltaX, y: input.scrollDeltaY)
            if ActriumScrollRendererContent(hitView, delta) {
                commit()
                let target = hitTest(at: location)
                hoverTracking.update(hitView: target, windowNumber: window.windowNumber, location: location)
                updateCursor(for: target)
            }

        case .keyDown, .keyUp:
            guard window.hasHostKeyboardFocus,
                  let event = NSEvent.keyEvent(
                with: input.type == .keyDown ? .keyDown : .keyUp,
                location: location,
                modifierFlags: modifiers,
                timestamp: timestamp,
                windowNumber: window.windowNumber,
                context: nil,
                characters: input.characters,
                charactersIgnoringModifiers: input.characters,
                isARepeat: false,
                keyCode: UInt16(input.keyCode)
            ) else {
                return
            }
            // Text composition happens in the Host process (TSM never engages
            // in this background, activation-prohibited process); dispatched
            // keys here are only non-text commands (arrows, enter, ...).
            // Deleting, Return and Tab can mutate the text storage, so
            // capture undo snapshots around them the same way mirrored
            // inserts do.
            let isDelete = input.type == .keyDown
                && (input.keyCode == 51 || input.keyCode == 117
                    || input.keyCode == 36 || input.keyCode == 48)
            if isDelete { beginUndoableEdit() }
            if let textView = resolvedTextView(window.firstResponder) {
                if input.type == .keyDown { textView.keyDown(with: event) }
                else { textView.keyUp(with: event) }
            } else {
                window.sendEvent(event)
            }
            if isDelete { endUndoableEdit() }
            if input.type == .keyDown {
                // Fabric publishes content changes after mounting. A caret
                // movement alone does not need to flush the remote surface.
                publishCaret()
            }

        case .mouseEntered:
            let target = hitTest(at: location)
            window.hostMouseLocation = location
            hoverTracking.update(hitView: target, windowNumber: window.windowNumber, location: location)
            updateCursor(for: target, force: true)
        case .mouseExited:
            window.hostMouseLocation = nil
            hoverTracking.update(hitView: nil, windowNumber: window.windowNumber, location: location)
            lastCursor = nil
        case .UNRECOGNIZED(_):
            break
        }
    }

    private func hitTest(at location: NSPoint) -> NSView? {
        let point = container.superview?.convert(location, from: nil) ?? location
        return container.hitTest(point)
    }

    private func updateCursor(for hitView: NSView?, force: Bool = false) {
        let cursor = ActriumRendererCursorForHitView(hitView)
        guard force || cursor != lastCursor else { return }
        lastCursor = cursor
        RendererProcessController.sendHostEvent(name: "com.actrium.rendererCursor",
                                               userInfo: ["cursor": cursor], tabId: tabId)
    }



    /// Compares the current first responder against the last reported
    /// text-input focus state and reports changes through `report`.
    func reportTextInputFocusIfChanged(_ report: (Bool) throws -> Void) {
        let responder = window.firstResponder
        let textView = resolvedTextView(responder)
        let focused = textView?.isEditable == true
            || (responder as? NSTextField)?.isEditable == true
        guard focused != lastReportedTextFocus else { return }
        lastReportedTextFocus = focused
        if focused { performDocumentSelectionCommand("clear") }
        try? report(focused)
        if focused { publishCaret() }
    }

    /// Flips the dedupe latch so the next `reportTextInputFocusIfChanged`
    /// reports even when the focus state itself did not change.
    func resetTextInputFocusDedupe() {
        let responder = window.firstResponder
        let focused = resolvedTextView(responder)?.isEditable == true
            || (responder as? NSTextField)?.isEditable == true
        lastReportedTextFocus = !focused
    }

    /// Applies Host-mirrored IME marked text to the focused text input.
    func applyMarkedText(_ text: String, selectedRange: NSRange) {
        guard let client = resolvedTextView(window.firstResponder) as? NSTextInputClient else { return }
        // Join the composing table BEFORE `setMarkedText:`: RN fires
        // `textInputDidChange` synchronously inside it, and the gate must
        // already recognize this component as composing to keep that event
        // away from JS. An empty payload means composition ended (cancel).
        if let component = rnComponentView(for: resolvedTextView(window.firstResponder)) {
            if text.isEmpty {
                rnComposingComponents.removeObject(forKey: component)
            } else if let editor = resolvedTextView(window.firstResponder) {
                rnComposingComponents.setObject(editor, forKey: component)
            }
        }
        client.setMarkedText(
            text,
            selectedRange: selectedRange,
            replacementRange: NSRange(location: NSNotFound, length: 0)
        )
        publishCaret()
        commit()
    }

    /// Applies Host-mirrored committed IME text to the focused text input.
    func applyInsertText(_ text: String) {
        let resolved = resolvedTextView(window.firstResponder)
        guard let client = resolved as? NSTextInputClient else { return }
        // Composition commit is detected by marked text present on the
        // editor BEFORE the insert: `insertText` replaces the marked text,
        // so reading the marked range after it would always see a cleared
        // range. (Zero-length ranges like {2, 0} linger after a commit —
        // only a NON-ZERO length means an active composition, which also
        // keeps plain-text inserts from entering the recovery path.)
        let preMarkedLength = resolved?.markedRange().length ?? 0
        beginUndoableEdit()
        client.insertText(text, replacementRange: NSRange(location: NSNotFound, length: 0))
        endUndoableEdit()
        // The insert fires NSTextView delegate callbacks while RN's
        // `_ignoreNextTextInputCall` latch is set (composition selection
        // updates set it even though our gate kept the composition change
        // events away from JS), so RN's own textInputDidChange swallows the
        // commit's onChange and JS never receives the final text. Clear the
        // latch and drive the event explicitly — identical bookkeeping to a
        // real keystroke. Plain-text inserts see no pre-insert marked text,
        // so ordinary characters keep exactly one onChange from RN's own
        // callback.
        let component = rnComponentView(for: resolved)
        if preMarkedLength > 0, let component {
            rnComposingComponents.removeObject(forKey: component)
            component.setValue(false, forKey: "ignoreNextTextInputCall")
            _ = component.perform(NSSelectorFromString("textInputDidChange"))
        }
        publishCaret()
        commit()
    }

    /// The editor's string without any marked (in-progress composition)
    /// text plus the caret, both in unmarked-string coordinates. Marked text
    /// lives in the text storage but must not leak into undo snapshots.
    private func currentEditSnapshot() -> EditSnapshot? {
        guard let editor = resolvedTextView(window.firstResponder) else { return nil }
        let marked = editor.markedRange()
        let string = (marked.location != NSNotFound && marked.length > 0)
            ? (editor.string as NSString).replacingCharacters(in: marked, with: "")
            : editor.string
        let caret = min(editor.selectedRange().location, (string as NSString).length)
        return EditSnapshot(string: string, caret: caret)
    }

    private func beginUndoableEdit() {
        guard let snapshot = currentEditSnapshot() else { return }
        withHistory { box in
            if box.undo.last?.string != snapshot.string { box.undo.append(snapshot) }
            box.redo.removeAll()
        }
    }

    private func endUndoableEdit() {
        guard let snapshot = currentEditSnapshot() else { return }
        withHistory { box in
            if box.undo.last?.string != snapshot.string { box.undo.append(snapshot) }
        }
    }

    private func restoreSnapshot(_ snapshot: String, caret: Int) {
        guard let editor = resolvedTextView(window.firstResponder), editor.isEditable else { return }
        let current = editor.string
        guard current != snapshot else {
            editor.setSelectedRange(NSRange(location: caret, length: 0))
            publishCaret()
            return
        }
        let full = NSRange(location: 0, length: (current as NSString).length)
        editor.setSelectedRange(full)
        editor.insertText(snapshot, replacementRange: full)
        editor.setSelectedRange(NSRange(location: caret, length: 0))
        // Undo/redo bypasses applyInsertText, so a change made while an IME
        // composition is active would have its event discarded by the gate.
        rnPublishEditingMutation(on: editor)
        publishCaret()
        commit()
    }

    /// Mutating editing commands (undo/redo/cut/paste) apply changes directly
    /// to the editor instead of going through `applyInsertText`, so the
    /// composing gate never sees the edit resolved. Drop the composition
    /// entry and re-drive RN's change event explicitly — identical
    /// bookkeeping to the commit recovery — otherwise JS keeps its pre-edit
    /// value while the native field shows the restored text.
    private func rnPublishEditingMutation(on editor: NSTextView?) {
        guard let editor, let component = rnComponentView(for: editor),
              rnComposingComponents.object(forKey: component) != nil
        else { return }
        rnComposingComponents.removeObject(forKey: component)
        component.setValue(false, forKey: "ignoreNextTextInputCall")
        _ = component.perform(NSSelectorFromString("textInputDidChange"))
    }

    private func undoTextEdit() {
        guard resolvedTextView(window.firstResponder) != nil else { return }
        withHistory { box in
            guard box.undo.count > 1 else { return }
            let current = box.undo.removeLast()
            box.redo.append(current)
            let target = box.undo[box.undo.count - 1]
            // Undoing an insertion puts the caret back at the insertion
            // point, not at the end of the restored text. All offsets are
            // UTF-16 units to match NSTextView selection semantics.
            let delta = (current.string as NSString).length - (target.string as NSString).length
            let caret = max(0, min(current.caret - delta, (target.string as NSString).length))
            restoreSnapshot(target.string, caret: caret)
        }
    }

    private func redoTextEdit() {
        guard resolvedTextView(window.firstResponder) != nil else { return }
        withHistory { box in
            guard let snapshot = box.redo.popLast() else { return }
            if let current = currentEditSnapshot() {
                box.undo.append(current)
            }
            restoreSnapshot(snapshot.string, caret: snapshot.caret)
        }
    }

    func performDocumentSelectionCommand(_ command: String) {
        switch command {
        case "clear": break
        case "copy":
            guard window.hasHostKeyboardFocus, !(window.firstResponder is NSTextView),
                  !(window.firstResponder is NSTextField) else { return }
        default: return
        }
        ServiceReactNativeHost.shared.sendDocumentSelectionCommand(command, tabId: tabId)
    }

    /// Applies Host-forwarded editing actions (menu key equivalents) to the
    /// focused text input. copy/cut/paste/selectAll run through AppKit's own
    /// implementations on the real text storage; undo/redo restore snapshots
    /// captured at each Host-mirrored text commit (AppKit's undoManager
    /// registrations go stale once RN rewrites the text storage).
    func performTextCommand(_ command: String) {
        if command == "undo" { undoTextEdit(); return }
        if command == "redo" { redoTextEdit(); return }
        let selectors: [String: Selector] = [
            "copy": #selector(NSText.copy(_:)),
            "cut": #selector(NSText.cut(_:)),
            "paste": #selector(NSText.paste(_:)),
            "selectAll": #selector(NSText.selectAll(_:)),
        ]
        guard let selector = selectors[command] else { return }
        let target: NSResponder? = (window.firstResponder as? NSResponder) ?? window
        let mutates = command == "cut" || command == "paste"
        if mutates { beginUndoableEdit() }
        ActriumSendEditingAction(selector, target ?? NSApp)
        if mutates {
            endUndoableEdit()
            // Same gate gap as undo/redo: the mutation bypasses
            // applyInsertText, so publish explicitly after it.
            rnPublishEditingMutation(on: resolvedTextView(window.firstResponder))
        }
        publishCaret()
        commit()
    }


    /// Drop ALL composition state owned by this surface, regardless of the
    /// current first responder. Called before the view hierarchy is torn
    /// down: once detached, `backing.window` is nil and window-scoped
    /// pruning can no longer reach the entries, while a recycled Fabric
    /// input could otherwise carry the stale state into a new editor.
    func invalidateComposingState() {
        rnPruneComposingComponents(activeEditor: nil, inWindow: window)
    }

    /// Drop composition state for components whose editor is no longer the
    /// focused text view of THIS surface. Called when the first responder
    /// changes; an interrupted composition must not keep its component in
    /// the gate's composing set (which would suppress its ordinary change
    /// events). Entries owned by other windows (tabs sharing this renderer)
    /// are left untouched.
    func pruneComposingState() {
        rnPruneComposingComponents(
            activeEditor: resolvedTextView(window.firstResponder),
            inWindow: window
        )
    }

    /// The Host asked this text input to give up focus (its own IME client
    /// resigned first responder, e.g. the user focused the address bar).
    func resignTextInputFocus() {
        if window.firstResponder is NSTextView || resolvedTextView(window.firstResponder) != nil
            || window.firstResponder is NSTextField {
            window.makeFirstResponder(container)
        }
        selectionGesture.invalidate()
        // Editing ended: drop any composition state still attached to inputs
        // owned by this surface.
        rnPruneComposingComponents(activeEditor: nil, inWindow: window)
        focusedEditor = nil
        lastCaretRect = nil
        RendererProcessLifetime.shared.controller?.publishCaret(tabId: tabId, rect: nil)
    }

    private func focusTextInput(
        _ view: NSView, at location: NSPoint, modifiers: NSEvent.ModifierFlags, clickCount: Int
    ) {
        // AppKit's native mouse tracking waits synchronously for mouse-up.
        // Renderer mouse-up arrives on the main queue over IPC.
        let focused = window.makeFirstResponder(view)
        let editor = (view as? NSTextField)?.currentEditor() as? NSTextView ?? view as? NSTextView
        focusedEditor = focused ? editor : nil
        multiClickStart = nil
        guard focused, let editor, let owner = control(for: editor) else { return }
        let point = editor.convert(location, from: nil)
        let index = editor.characterIndexForInsertion(at: point)
        guard index != NSNotFound, index <= (editor.string as NSString).length else {
            // No gesture starts at an invalid position; a leftover anchor
            // from an earlier press must not drive the coming moves.
            selectionGesture.invalidate()
            return
        }
        if modifiers.contains(.shift) {
            // Shift+click/drag extends the selection; the mouse-down and
            // mouse-up paths consume the event, so AppKit's own extension
            // never runs. The base is the existing selection's anchor when a
            // selection is live, and the opposite edge otherwise, so the
            // whole range stays selected. The resolved base is stored back so
            // the rest of the gesture uses one anchor.
            let current = editor.selectedRange()
            let base: Int
            if current.length > 0,
               let anchor = selectionGesture.shiftAnchor(owner: owner, current: current) {
                // The stored anchor still describes the current selection
                // in this control.
                base = anchor
            } else if index >= current.upperBound {
                // Untracked selection, click on the right: extend from the
                // opposite (left) edge so the existing range stays selected.
                base = current.location
            } else if index <= current.location {
                // Click on the left: extend from the right edge.
                base = current.upperBound
            } else {
                // Click inside: keep the farther edge so neither side is
                // dropped arbitrarily.
                base = (index - current.location) > (current.upperBound - index)
                    ? current.location
                    : current.upperBound
            }
            let range = NSRange(location: min(base, index), length: abs(index - base))
            selectionGesture.beginPress(owner: owner, anchor: base, range: range)
            editor.setSelectedRange(range)
            publishCaret()
            commit()
        } else {
            // Determine selection granularity from click count.
            let granularity: NSSelectionGranularity = switch clickCount {
            case 2: .selectByWord
            case 3...: .selectByParagraph
            default: .selectByCharacter
            }
            let caret = NSRange(location: index, length: 0)
            let range = editor.selectionRange(forProposedRange: caret, granularity: granularity)
            if granularity != .selectByCharacter {
                multiClickStart = (range, granularity)
            }
            selectionGesture.beginPress(
                owner: owner,
                anchor: range.location,
                range: range
            )
            editor.setSelectedRange(range)
            publishCaret()
        }
    }

    /// Extends a multi-click selection (word or paragraph) during drag.
    /// Returns the new range and anchor to use for the selection.
    private func extendMultiClickSelection(
        from start: NSRange,
        to target: NSRange,
        at index: Int
    ) -> (range: NSRange, anchor: Int) {
        // Dragging right: extend from start's left edge to target's right edge.
        if index >= start.location {
            let range = NSRange(
                location: start.location,
                length: target.upperBound - start.location
            )
            return (range, start.location)
        }
        // Dragging left: extend from target's left edge to start's right edge.
        let range = NSRange(
            location: target.location,
            length: start.upperBound - target.location
        )
        return (range, start.upperBound)
    }

    /// Publishes the caret rect to the Host so native IME UI (candidate
    /// window) can anchor at the remotely rendered insertion point.
    private func publishCaret() {
        let rect = caretRect()
        guard rect != lastCaretRect else { return }
        lastCaretRect = rect
        RendererProcessLifetime.shared.controller?.publishCaret(tabId: tabId, rect: rect)
    }

    private func caretRect() -> CGRect? {
        // Prefer the stored editor but fall back to the current first
        // responder: autoFocus / Tab / JS .focus() never pass through
        // `focusTextInput`, leaving `focusedEditor` nil even though a text
        // view owns first responder.
        let editor = focusedEditor ?? resolvedTextView(window.firstResponder)
        guard let editor, resolvedTextView(window.firstResponder) === editor, editor.isEditable,
              editor.selectedRange().length == 0 else {
            return nil
        }
        let screenRect = editor.firstRect(forCharacterRange: editor.selectedRange(), actualRange: nil)
        let localRect: NSRect
        if editor.string.isEmpty {
            // AppKit can return an empty character rectangle for an empty field.
            let origin = editor.textContainerOrigin
            let inset = editor.textContainer?.lineFragmentPadding ?? 0
            let lineHeight = editor.font.map {
                editor.layoutManager?.defaultLineHeight(for: $0) ?? $0.pointSize
            } ?? NSFont.systemFontSize
            let height = min(editor.bounds.height, lineHeight)
            let rect = NSRect(x: origin.x + inset, y: editor.bounds.midY - height / 2,
                              width: 2, height: height)
            localRect = container.convert(rect, from: editor)
        } else {
            guard screenRect.height > 0 else {
                return nil
            }
            localRect = container.convert(window.convertFromScreen(screenRect), from: nil)
        }
        return CGRect(x: localRect.minX, y: localRect.minY, width: 2, height: localRect.height)
    }

    private func modifierFlags(from encoded: UInt32) -> NSEvent.ModifierFlags {
        var result: NSEvent.ModifierFlags = []
        if encoded & 1 != 0 { result.insert(.shift) }
        if encoded & 2 != 0 { result.insert(.control) }
        if encoded & 4 != 0 { result.insert(.option) }
        if encoded & 8 != 0 { result.insert(.command) }
        if encoded & 16 != 0 { result.insert(.capsLock) }
        return result
    }
}

private extension RendererMouseTracking {
    static func eventType(for input: Actrium_Ipc_InputEvent) -> NSEvent.EventType? {
        switch (input.type, input.button) {
        case (.mouseMove, _): return .mouseMoved
        case (.mouseDown, 0): return .leftMouseDown
        case (.mouseDown, 1): return .rightMouseDown
        case (.mouseDown, 2): return .otherMouseDown
        case (.mouseDragged, 0): return .leftMouseDragged
        case (.mouseDragged, 1): return .rightMouseDragged
        case (.mouseDragged, 2): return .otherMouseDragged
        case (.mouseUp, 0): return .leftMouseUp
        case (.mouseUp, 1): return .rightMouseUp
        case (.mouseUp, 2): return .otherMouseUp
        default: return nil
        }
    }
}
