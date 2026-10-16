//
//  TabSurfaceDisplayView.swift
//  Actrium
//
//  Pure display component for tab surfaces.
//  Only references contextId and displays - no rendering lifecycle management.
//
//  Usage:
//  - TabSurfaceManager manages the rendering
//  - This view just displays the result via CALayerHost
//  - Apply scaleEffect for Overview mode thumbnails
//

import SwiftUI
import AppKit
import QuartzCore

// MARK: - Tab Surface Display View (SwiftUI)

/// Pure display view for tab content.
/// Shows the CALayerHost content for a given contextId.
/// Does NOT manage renderer lifecycle - that's TabSurfaceManager's job.
struct TabSurfaceDisplayView: NSViewRepresentable {
    let tabId: UUID
    let contextId: UInt32
    let width: CGFloat
    let height: CGFloat

    func makeNSView(context: Context) -> TabSurfaceDisplayNSView {
        let view = TabSurfaceDisplayNSView(frame: NSRect(x: 0, y: 0, width: width, height: height))
        view.tabId = tabId
        view.updateContext(id: contextId)
        return view
    }

    func updateNSView(_ nsView: TabSurfaceDisplayNSView, context: Context) {
        nsView.tabId = tabId
        nsView.updateContext(id: contextId)

        // Update size if changed
        if abs(nsView.bounds.width - width) > 1 || abs(nsView.bounds.height - height) > 1 {
            nsView.setFrameSize(NSSize(width: width, height: height))
        }
    }
}

// MARK: - Tab Surface Display NSView (AppKit)

/// AppKit view that displays remote CALayer content using CALayerHost.
/// Simplified version focused only on display, not lifecycle management.
class TabSurfaceDisplayNSView: NSView {
    private var hostLayer: CALayer?
    private var currentContextId: UInt32 = 0

    /// Tab ID for event forwarding
    var tabId: UUID?

    /// Tracking area for mouse events
    private var trackingArea: NSTrackingArea?

    private static let layerHostClass: AnyClass? = NSClassFromString("CALayerHost")

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setupLayer()
        setupTracking()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setupLayer()
        setupTracking()
    }

    private func setupLayer() {
        wantsLayer = true
        layer?.backgroundColor = NSColor(white: 0.12, alpha: 1.0).cgColor
    }

    private func setupTracking() {
        trackingArea = NSTrackingArea(
            rect: bounds,
            options: [.mouseEnteredAndExited, .mouseMoved, .activeInKeyWindow, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(trackingArea!)
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let existing = trackingArea {
            removeTrackingArea(existing)
        }
        trackingArea = NSTrackingArea(
            rect: bounds,
            options: [.mouseEnteredAndExited, .mouseMoved, .activeInKeyWindow, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(trackingArea!)
    }

    // MARK: - Context Update

    /// Update the CALayerHost with a new context ID
    func updateContext(id: UInt32) {
        guard id != 0 else {
            showPlaceholder()
            return
        }

        guard id != currentContextId else { return }

        currentContextId = id
        setupHostLayer(contextId: id)
    }

    private func setupHostLayer(contextId: UInt32) {
        // Remove existing host layer
        hostLayer?.removeFromSuperlayer()
        hostLayer = nil

        guard let layerHostClass = Self.layerHostClass else {
            NSLog("🔴 [TabSurfaceDisplay] CALayerHost class not available")
            return
        }

        guard let newHostLayer = layerHostClass.alloc() as? CALayer else {
            NSLog("🔴 [TabSurfaceDisplay] Failed to allocate CALayerHost")
            return
        }

        // Initialize
        _ = newHostLayer.perform(NSSelectorFromString("init"))

        // Set the contextId
        newHostLayer.setValue(NSNumber(value: contextId), forKey: "contextId")

        let scale = window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 2.0

        // Configure
        newHostLayer.anchorPoint = .zero
        newHostLayer.frame = bounds
        newHostLayer.contentsScale = scale
        newHostLayer.isOpaque = true
        newHostLayer.masksToBounds = false

        // Disable implicit animations
        newHostLayer.actions = [
            "bounds": NSNull(),
            "position": NSNull(),
            "contents": NSNull(),
            "sublayers": NSNull(),
            "transform": NSNull()
        ]

        layer?.addSublayer(newHostLayer)
        hostLayer = newHostLayer
    }

    private func showPlaceholder() {
        currentContextId = 0
        hostLayer?.removeFromSuperlayer()
        hostLayer = nil
        layer?.backgroundColor = NSColor(white: 0.12, alpha: 1.0).cgColor
    }

    override func layout() {
        super.layout()
        hostLayer?.frame = bounds
    }

    override var isFlipped: Bool { true }

    // MARK: - Mouse Events

    override func mouseDown(with event: NSEvent) {
        forwardMouseEvent(event, type: .mouseDown)
    }

    override func mouseUp(with event: NSEvent) {
        forwardMouseEvent(event, type: .mouseUp)
    }

    override func mouseMoved(with event: NSEvent) {
        forwardMouseEvent(event, type: .mouseMoved)
    }

    override func mouseDragged(with event: NSEvent) {
        forwardMouseEvent(event, type: .mouseDragged)
    }

    override func scrollWheel(with event: NSEvent) {
        guard let tabId = tabId else { return }
        let location = convert(event.locationInWindow, from: nil)
        let xpcEvent = TabXPCEvent(
            eventType: .scrollWheel,
            locationX: Double(location.x),
            locationY: Double(location.y),
            modifierFlags: event.modifierFlags.rawValue,
            scrollDeltaX: event.scrollingDeltaX,
            scrollDeltaY: event.scrollingDeltaY,
            viewWidth: Double(bounds.width),
            viewHeight: Double(bounds.height)
        )
        LegacyRendererProcessManager.shared.forwardEvent(xpcEvent, to: tabId)
    }

    private func forwardMouseEvent(_ event: NSEvent, type: TabXPCEvent.EventType) {
        guard let tabId = tabId else { return }
        let location = convert(event.locationInWindow, from: nil)
        let xpcEvent = TabXPCEvent(
            eventType: type,
            locationX: Double(location.x),
            locationY: Double(location.y),
            modifierFlags: event.modifierFlags.rawValue,
            viewWidth: Double(bounds.width),
            viewHeight: Double(bounds.height)
        )
        LegacyRendererProcessManager.shared.forwardEvent(xpcEvent, to: tabId)
    }

    // MARK: - Keyboard Events

    override var acceptsFirstResponder: Bool { true }

    override func keyDown(with event: NSEvent) {
        forwardKeyEvent(event, type: .keyDown)
    }

    override func keyUp(with event: NSEvent) {
        forwardKeyEvent(event, type: .keyUp)
    }

    override func flagsChanged(with event: NSEvent) {
        guard let tabId = tabId else { return }
        let xpcEvent = TabXPCEvent(
            eventType: .flagsChanged,
            modifierFlags: event.modifierFlags.rawValue,
            keyCode: event.keyCode
        )
        LegacyRendererProcessManager.shared.forwardEvent(xpcEvent, to: tabId)
    }

    private func forwardKeyEvent(_ event: NSEvent, type: TabXPCEvent.EventType) {
        guard let tabId = tabId else { return }
        let xpcEvent = TabXPCEvent(
            eventType: type,
            modifierFlags: event.modifierFlags.rawValue,
            keyCode: event.keyCode,
            characters: event.characters
        )
        LegacyRendererProcessManager.shared.forwardEvent(xpcEvent, to: tabId)
    }

    // MARK: - Hit Testing

    override func hitTest(_ point: NSPoint) -> NSView? {
        if bounds.contains(point) {
            return self
        }
        return nil
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}

// MARK: - Tab Surface Content View

/// High-level SwiftUI view for displaying tab content via surface manager.
/// Combines TabSurfaceDisplayView with loading/error states.
struct TabSurfaceContentView: View {
    let tab: Tab
    let displaySize: CGSize  // Size to display at (may be scaled)
    @ObservedObject var surfaceManager: TabSurfaceManager

    var body: some View {
        let contextId = surfaceManager.contextId(for: tab.id)
        let isLoading = surfaceManager.isLoading(for: tab.id)
        let crashReason = surfaceManager.crashReason(for: tab.id)

        ZStack {
            if let reason = crashReason {
                crashedView(reason: reason)
            } else if contextId != 0 {
                // Display the surface at viewport size, then scale to display size
                TabSurfaceDisplayView(
                    tabId: tab.id,
                    contextId: contextId,
                    width: surfaceManager.viewportSize.width,
                    height: surfaceManager.viewportSize.height
                )
                .frame(
                    width: surfaceManager.viewportSize.width,
                    height: surfaceManager.viewportSize.height
                )
                .scaleEffect(
                    displaySize.width / max(surfaceManager.viewportSize.width, 1),
                    anchor: .topLeading
                )
                .frame(width: displaySize.width, height: displaySize.height)
                .clipped()
            } else if isLoading {
                loadingView
            } else {
                placeholderView
            }
        }
        .onAppear {
            surfaceManager.ensureRendering(tab: tab)
        }
    }

    private var loadingView: some View {
        VStack(spacing: 12) {
            ProgressView()
                .scaleEffect(0.8)
            Text("Loading...")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .windowBackgroundColor).opacity(0.9))
    }

    private var placeholderView: some View {
        VStack(spacing: 12) {
            Image(systemName: "rectangle.dashed")
                .font(.system(size: 32))
                .foregroundColor(.secondary)
            Text("No content")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func crashedView(reason: String) -> some View {
        VStack(spacing: 16) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 36))
                .foregroundColor(.orange)

            Text("This tab has crashed")
                .font(.system(size: 16, weight: .semibold))

            Text(reason)
                .font(.system(size: 12, design: .monospaced))
                .foregroundColor(.secondary)
                .lineLimit(2)

            Button("Reload") {
                surfaceManager.reloadCrashedTab(tab)
            }
            .buttonStyle(.borderedProminent)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}
