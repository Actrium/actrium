import SwiftUI
import AppKit
import IOSurface
import QuartzCore
import Combine
import CoreVideo

// MARK: - IOSurface Host View (SwiftUI)

/// SwiftUI wrapper for displaying IOSurface content from a remote process
struct IOSurfaceHostView: NSViewRepresentable {
    let surfaceID: UInt32
    let width: CGFloat
    let height: CGFloat
    let tabId: UUID

    func makeNSView(context: Context) -> IOSurfaceDisplayView {
        let view = IOSurfaceDisplayView()
        view.tabId = tabId
        view.updateSurface(id: surfaceID, width: width, height: height)
        return view
    }

    func updateNSView(_ nsView: IOSurfaceDisplayView, context: Context) {
        nsView.tabId = tabId
        nsView.updateSurface(id: surfaceID, width: width, height: height)
    }
}

// MARK: - IOSurface Display View (AppKit)

/// AppKit view that displays IOSurface content using CALayer
class IOSurfaceDisplayView: NSView {
    private var surfaceLayer: CALayer?
    private var currentSurfaceID: UInt32 = 0

    /// Tab ID for event forwarding
    var tabId: UUID?

    /// Tracking area for mouse movement events
    private var trackingArea: NSTrackingArea?

    /// Display link for refreshing IOSurface content
    private var displayLink: CVDisplayLink?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setupLayer()
        setupTracking()
        setupDisplayLink()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setupLayer()
        setupTracking()
        setupDisplayLink()
    }

    deinit {
        stopDisplayLink()
    }

    private func setupLayer() {
        wantsLayer = true
        layer?.backgroundColor = NSColor.black.cgColor
    }

    private func setupDisplayLink() {
        var link: CVDisplayLink?
        CVDisplayLinkCreateWithActiveCGDisplays(&link)
        guard let displayLink = link else { return }

        let callback: CVDisplayLinkOutputCallback = { _, _, _, _, _, userInfo -> CVReturn in
            guard let userInfo = userInfo else { return kCVReturnSuccess }
            let view = Unmanaged<IOSurfaceDisplayView>.fromOpaque(userInfo).takeUnretainedValue()
            DispatchQueue.main.async {
                view.refreshSurfaceContent()
            }
            return kCVReturnSuccess
        }

        CVDisplayLinkSetOutputCallback(displayLink, callback, Unmanaged.passUnretained(self).toOpaque())
        CVDisplayLinkStart(displayLink)
        self.displayLink = displayLink
    }

    private func stopDisplayLink() {
        guard let link = displayLink else { return }
        CVDisplayLinkStop(link)
        displayLink = nil
    }

    private func refreshSurfaceContent() {
        guard currentSurfaceID != 0, let surface = IOSurfaceLookup(currentSurfaceID) else { return }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        surfaceLayer?.contents = surface
        CATransaction.commit()
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
        TabProcessManager.shared.sendEvent(xpcEvent, to: tabId)
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
        TabProcessManager.shared.sendEvent(xpcEvent, to: tabId)
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
        TabProcessManager.shared.sendEvent(xpcEvent, to: tabId)
    }

    private func forwardKeyEvent(_ event: NSEvent, type: TabXPCEvent.EventType) {
        guard let tabId = tabId else { return }
        let xpcEvent = TabXPCEvent(
            eventType: type,
            modifierFlags: event.modifierFlags.rawValue,
            keyCode: event.keyCode,
            characters: event.characters
        )
        TabProcessManager.shared.sendEvent(xpcEvent, to: tabId)
    }

    func updateSurface(id: UInt32, width: CGFloat, height: CGFloat) {
        guard id != 0 else {
            // No surface available - show placeholder
            showPlaceholder()
            return
        }

        guard id != currentSurfaceID else {
            // Same surface, just refresh
            refreshSurface()
            return
        }

        currentSurfaceID = id

        // Look up the IOSurface by ID
        guard let surface = IOSurfaceLookup(id) else {
            NSLog("IOSurfaceHostView: Failed to lookup surface with ID \(id)")
            showPlaceholder()
            return
        }

        // Create or update the surface layer
        if surfaceLayer == nil {
            let newLayer = CALayer()
            newLayer.contentsGravity = .resize  // Use resize to fill the view
            newLayer.magnificationFilter = .nearest
            newLayer.isOpaque = true
            layer?.addSublayer(newLayer)
            surfaceLayer = newLayer
        }

        // Set the IOSurface as the layer's contents
        // Force update by clearing first
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        surfaceLayer?.contents = nil
        surfaceLayer?.contents = surface
        surfaceLayer?.frame = bounds
        CATransaction.commit()

        NSLog("IOSurfaceHostView: Displaying surface ID \(id), size \(IOSurfaceGetWidth(surface))x\(IOSurfaceGetHeight(surface)), layer frame: \(bounds)")
    }

    func refreshSurface() {
        // For IOSurface, we need to re-set the contents to force a redraw
        guard currentSurfaceID != 0, let surface = IOSurfaceLookup(currentSurfaceID) else {
            return
        }

        CATransaction.begin()
        CATransaction.setDisableActions(true)
        surfaceLayer?.contents = surface
        CATransaction.commit()
    }

    private func showPlaceholder() {
        currentSurfaceID = 0
        surfaceLayer?.contents = nil

        // Show a placeholder color
        layer?.backgroundColor = NSColor(red: 0.15, green: 0.15, blue: 0.2, alpha: 1.0).cgColor
    }

    override func layout() {
        super.layout()
        surfaceLayer?.frame = bounds
    }

    override var isFlipped: Bool {
        return true
    }

    // MARK: - Hit Testing (Required for mouse events in SwiftUI wrapper)

    override func hitTest(_ point: NSPoint) -> NSView? {
        // Ensure this view receives all mouse events within its bounds
        if bounds.contains(point) {
            return self
        }
        return nil
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        // Accept first mouse to receive mouseDown even when window is inactive
        return true
    }
}

// MARK: - Multi-Process Tab View

/// A view that displays tab content from a renderer process
struct MultiProcessTabView: View {
    let tabId: UUID
    @ObservedObject var processManager = TabProcessManager.shared

    @State private var surfaceID: UInt32 = 0
    @State private var isLoading: Bool = true
    @State private var errorMessage: String?
    @State private var activeTabId: UUID?  // The actual tabId used for the process
    @State private var crashReason: String?  // Crash reason if process crashed

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                if let reason = crashReason {
                    crashedView(reason: reason)
                } else if surfaceID != 0, let activeId = activeTabId {
                    IOSurfaceHostView(
                        surfaceID: surfaceID,
                        width: geometry.size.width,
                        height: geometry.size.height,
                        tabId: activeId  // Use the actual process tabId
                    )
                } else if isLoading {
                    loadingView
                } else if let error = errorMessage {
                    errorView(error)
                } else {
                    placeholderView
                }
            }
            .onChange(of: geometry.size) { _, newSize in
                updateViewportSize(newSize)
            }
        }
        .onAppear {
            startRenderer()
        }
        .onDisappear {
            stopRenderer()
        }
        .onReceive(NotificationCenter.default.publisher(for: .tabSurfaceUpdated)) { notification in
            if let notifTabId = notification.userInfo?["tabId"] as? UUID, notifTabId == tabId {
                refreshSurface()
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .tabRendererCrashed)) { notification in
            if let notifTabId = notification.userInfo?["tabId"] as? UUID, notifTabId == activeTabId {
                let reason = notification.userInfo?["reason"] as? String ?? "Unknown error"
                handleCrash(reason: reason)
            }
        }
    }

    private var loadingView: some View {
        VStack(spacing: 12) {
            ProgressView()
                .scaleEffect(1.5)
            Text("Starting renderer process...")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
        }
    }

    private func errorView(_ message: String) -> some View {
        VStack(spacing: 12) {
            Image(systemName: "exclamationmark.triangle")
                .font(.system(size: 32))
                .foregroundColor(.orange)
            Text("Renderer Error")
                .font(.headline)
            Text(message)
                .font(.system(size: 12))
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal)

            Button("Retry") {
                startRenderer()
            }
            .buttonStyle(.bordered)
        }
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
    }

    private func crashedView(reason: String) -> some View {
        VStack(spacing: 16) {
            // Sad tab icon
            ZStack {
                Circle()
                    .fill(Color.gray.opacity(0.1))
                    .frame(width: 80, height: 80)
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.system(size: 36))
                    .foregroundColor(.orange)
            }

            Text("This tab has crashed")
                .font(.system(size: 16, weight: .semibold))

            Text(reason)
                .font(.system(size: 12, design: .monospaced))
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 32)
                .lineLimit(3)

            Button(action: {
                reloadAfterCrash()
            }) {
                HStack(spacing: 6) {
                    Image(systemName: "arrow.clockwise")
                    Text("Reload Tab")
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 8)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.regular)

            Text("The renderer process terminated unexpectedly.\nReloading will restart it in a new process.")
                .font(.system(size: 10))
                .foregroundColor(Color(nsColor: .tertiaryLabelColor))
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private func handleCrash(reason: String) {
        crashReason = reason
        surfaceID = 0
        isLoading = false
    }

    private func reloadAfterCrash() {
        crashReason = nil
        surfaceID = 0
        activeTabId = nil
        startRenderer()
    }

    private func startRenderer() {
        guard processManager.isMultiProcessEnabled else {
            errorMessage = "Multi-process mode not available"
            isLoading = false
            return
        }

        // Clear any previous crash state
        crashReason = nil

        isLoading = true
        errorMessage = nil

        // Use the passed tabId for this session
        let processTabId = tabId

        Task {
            do {
                let config = TabXPCConfig(
                    tabId: processTabId.uuidString,
                    title: "Test Tab",
                    backgroundColor: "#1a1a2e"
                )

                let processInfo = try await processManager.spawnProcess(for: processTabId, config: config)

                // Get the IOSurface ID
                if let id = await processManager.getIOSurfaceID(for: processTabId) {
                    await MainActor.run {
                        // Store the actual tabId used for the process
                        activeTabId = processTabId
                        surfaceID = id
                        isLoading = false
                    }
                }
            } catch {
                await MainActor.run {
                    errorMessage = error.localizedDescription
                    isLoading = false
                }
            }
        }
    }

    private func stopRenderer() {
        if let activeId = activeTabId {
            processManager.terminateProcess(for: activeId)
        }
        surfaceID = 0
        activeTabId = nil
    }

    private func refreshSurface() {
        guard let activeId = activeTabId else { return }
        Task {
            if let id = await processManager.getIOSurfaceID(for: activeId), id != surfaceID {
                await MainActor.run {
                    surfaceID = id
                }
            }
        }
    }

    private func updateViewportSize(_ size: CGSize) {
        guard let activeId = activeTabId else { return }
        processManager.updateViewport(width: size.width, height: size.height, for: activeId)
    }
}

// MARK: - Preview

#Preview {
    MultiProcessTabView(tabId: UUID())
        .frame(width: 800, height: 600)
}
