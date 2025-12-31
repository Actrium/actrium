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
        let view = IOSurfaceDisplayView(frame: NSRect(x: 0, y: 0, width: width, height: height))
        view.tabId = tabId
        view.surfaceWidth = width
        view.surfaceHeight = height
        view.updateSurface(id: surfaceID, width: width, height: height)
        return view
    }

    func updateNSView(_ nsView: IOSurfaceDisplayView, context: Context) {
        nsView.tabId = tabId
        nsView.surfaceWidth = width
        nsView.surfaceHeight = height
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

    /// Expected surface dimensions for intrinsicContentSize
    var surfaceWidth: CGFloat = 0
    var surfaceHeight: CGFloat = 0

    /// Tracking area for mouse movement events
    private var trackingArea: NSTrackingArea?

    /// Display link for refreshing IOSurface content
    private var displayLink: CVDisplayLink?

    /// Return the expected size so SwiftUI can properly size this view
    override var intrinsicContentSize: NSSize {
        return NSSize(width: surfaceWidth, height: surfaceHeight)
    }

    private static var instanceCount: Int = 0

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        IOSurfaceDisplayView.instanceCount += 1
        NSLog("IOSurfaceDisplayView: init, total instances: \(IOSurfaceDisplayView.instanceCount)")
        setupLayer()
        setupTracking()
        // Don't start displayLink here - wait until we have a valid surface ID
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setupLayer()
        setupTracking()
        // Don't start displayLink here - wait until we have a valid surface ID
    }

    deinit {
        NSLog("IOSurfaceDisplayView: deinit called for tabId \(tabId?.uuidString ?? "nil")")
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

    private var refreshCount: UInt64 = 0
    private var lastSetSurfaceID: UInt32 = 0

    private func refreshSurfaceContent() {
        refreshCount += 1
        guard currentSurfaceID != 0, let surface = IOSurfaceLookup(currentSurfaceID) else {
            if refreshCount % 60 == 1 {
                NSLog("🖥️ [DIAG] refreshSurfaceContent: currentSurfaceID=\(currentSurfaceID), IOSurfaceLookup failed")
            }
            return
        }

        // Log surface info on first frame and every ~2 seconds
        if refreshCount == 1 || refreshCount % 120 == 0 {
            let width = IOSurfaceGetWidth(surface)
            let height = IOSurfaceGetHeight(surface)
            let viewScale = window?.backingScaleFactor ?? 1.0
            NSLog("🖥️ [DIAG] refreshSurfaceContent #\(refreshCount): surfaceID=\(currentSurfaceID), surfacePixels=\(width)x\(height), viewBounds=\(bounds), screenScale=\(viewScale)")
        }

        // DON'T re-set contents every frame - this causes flicker!
        // IOSurface is shared memory, CALayer will see updates automatically.
        // Just mark the layer as needing display to trigger a redraw.
        surfaceLayer?.setNeedsDisplay()
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
        TabProcessManager.shared.forwardEvent(xpcEvent, to: tabId)
    }

    private func forwardMouseEvent(_ event: NSEvent, type: TabXPCEvent.EventType) {
        guard let tabId = tabId else {
            NSLog("🖱️ [DIAG] forwardMouseEvent: tabId is nil!")
            return
        }
        let location = convert(event.locationInWindow, from: nil)
        NSLog("🖱️ [DIAG] forwardMouseEvent: type=\(type), location=\(location), tabId=\(tabId)")
        let xpcEvent = TabXPCEvent(
            eventType: type,
            locationX: Double(location.x),
            locationY: Double(location.y),
            modifierFlags: event.modifierFlags.rawValue,
            viewWidth: Double(bounds.width),
            viewHeight: Double(bounds.height)
        )
        TabProcessManager.shared.forwardEvent(xpcEvent, to: tabId)
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
        TabProcessManager.shared.forwardEvent(xpcEvent, to: tabId)
    }

    private func forwardKeyEvent(_ event: NSEvent, type: TabXPCEvent.EventType) {
        guard let tabId = tabId else { return }
        let xpcEvent = TabXPCEvent(
            eventType: type,
            modifierFlags: event.modifierFlags.rawValue,
            keyCode: event.keyCode,
            characters: event.characters
        )
        TabProcessManager.shared.forwardEvent(xpcEvent, to: tabId)
    }

    func updateSurface(id: UInt32, width: CGFloat, height: CGFloat) {
        NSLog("📐 [DIAG] IOSurfaceDisplayView.updateSurface: id=\(id), requestedSize=\(Int(width))x\(Int(height)), currentID=\(currentSurfaceID), bounds=\(bounds)")

        guard id != 0 else {
            // No surface available - show placeholder and stop display link
            stopDisplayLink()
            showPlaceholder()
            return
        }

        guard id != currentSurfaceID else {
            // Same surface, just refresh
            NSLog("📐 [DIAG]   Same surface ID, calling refreshSurface()")
            refreshSurface()
            return
        }

        NSLog("📐 [DIAG]   New surface ID! Updating from \(currentSurfaceID) to \(id)")
        currentSurfaceID = id

        // Look up the IOSurface by ID
        guard let surface = IOSurfaceLookup(id) else {
            NSLog("IOSurfaceHostView: Failed to lookup surface with ID \(id)")
            showPlaceholder()
            return
        }

        let surfacePixelWidth = IOSurfaceGetWidth(surface)
        let surfacePixelHeight = IOSurfaceGetHeight(surface)
        NSLog("📐 [DIAG]   IOSurface pixel size: \(surfacePixelWidth)x\(surfacePixelHeight)")

        // Create or update the surface layer
        if surfaceLayer == nil {
            let newLayer = CALayer()
            newLayer.contentsGravity = .resize  // Use resize to fill the view
            newLayer.magnificationFilter = .linear  // Smoother scaling
            newLayer.minificationFilter = .linear
            newLayer.isOpaque = true
            // Set contentsScale for Retina
            newLayer.contentsScale = window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 2.0
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

        // NOTE: We no longer use displayLink on the host side.
        // IOSurface is shared memory - once set as contents, updates from
        // the renderer process are automatically visible without needing
        // to re-set contents every frame (which causes flicker).
        // The renderer's displayLink handles the 60fps rendering.

        NSLog("🖥️ [DIAG] IOSurfaceHostView.updateSurface: surfaceID=\(id), surfaceSize=\(IOSurfaceGetWidth(surface))x\(IOSurfaceGetHeight(surface))")
        NSLog("🖥️ [DIAG]   view.bounds=\(bounds), view.frame=\(frame)")
        NSLog("🖥️ [DIAG]   surfaceLayer.frame=\(surfaceLayer?.frame ?? .zero), surfaceLayer.hidden=\(surfaceLayer?.isHidden ?? true)")
        NSLog("🖥️ [DIAG]   layer.sublayers=\(layer?.sublayers?.count ?? 0), view.superview=\(superview != nil)")
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
    let tab: Tab
    let containerSize: CGSize

    // Don't use @ObservedObject here - it causes re-renders when processManager updates
    // which disrupts the view lifecycle. We only need to call methods, not observe state.
    private let processManager = TabProcessManager.shared

    @State private var surfaceID: UInt32 = 0
    @State private var isLoading: Bool = true
    @State private var errorMessage: String?
    @State private var activeTabId: UUID?  // The actual tabId used for the process
    @State private var crashReason: String?  // Crash reason if process crashed

    private var tabId: UUID { tab.id }

    var body: some View {
        ZStack {
            if let reason = crashReason {
                crashedView(reason: reason)
            } else if surfaceID != 0, let activeId = activeTabId {
                IOSurfaceHostView(
                    surfaceID: surfaceID,
                    width: containerSize.width,
                    height: containerSize.height,
                    tabId: activeId  // Use the actual process tabId
                )
                .frame(width: containerSize.width, height: containerSize.height)
            } else if isLoading {
                loadingView
            } else if let error = errorMessage {
                errorView(error)
            } else {
                placeholderView
            }
        }
        .onChange(of: containerSize) { _, newSize in
            updateViewportSize(newSize)
        }
        .onAppear {
            NSLog("🔵 [DIAG] MultiProcessTabView.onAppear: tabId=\(tabId), surfaceID=\(surfaceID), isLoading=\(isLoading)")
            startRenderer()
        }
        .onDisappear {
            NSLog("🔴 [DIAG] MultiProcessTabView.onDisappear: tabId=\(tabId), surfaceID=\(surfaceID)")
            // 不在消失时强行终止进程，避免 SwiftUI 视图重建导致渲染进程被提前杀掉
        }
        .onReceive(NotificationCenter.default.publisher(for: .tabSurfaceUpdated)) { notification in
            if let notifTabId = notification.userInfo?["tabId"] as? UUID, notifTabId == tabId,
               let newSurfaceID = notification.userInfo?["surfaceID"] as? UInt32 {
                NSLog("📐 [DIAG] tabSurfaceUpdated notification: tabId=\(tabId), surfaceID=\(newSurfaceID)")
                // Set activeTabId if not yet set (handles race condition during startup)
                if activeTabId == nil {
                    activeTabId = tabId
                }
                if newSurfaceID != surfaceID {
                    surfaceID = newSurfaceID
                    isLoading = false
                }
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: .tabRendererCrashed)) { notification in
            if let notifTabId = notification.userInfo?["tabId"] as? UUID, notifTabId == activeTabId {
                let reason = notification.userInfo?["reason"] as? String ?? "Unknown error"
                handleCrash(reason: reason)
            }
        }
        // Use stable identity to prevent SwiftUI from recreating the view
        .id(tab.id)
    }

    private var loadingView: some View {
        VStack(spacing: 12) {
            // 纯文本占位，避免 AppKitProgressView 的 min/max 约束日志
            Text("Starting renderer process...")
                .font(.system(size: 12))
                .foregroundColor(.secondary)
        }
        .frame(minWidth: 180)
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
        // Clear any previous crash state
        crashReason = nil

        isLoading = true
        errorMessage = nil

        // Use the passed tabId for this session
        let processTabId = tabId

        Task {
            do {
                // 如果已有运行中的进程，直接复用，避免重复启动
                if let existing = processManager.processes[processTabId], existing.isRunning {
                    if let id = await processManager.getIOSurfaceID(for: processTabId), id != 0 {
                        await MainActor.run {
                            activeTabId = processTabId
                            surfaceID = id
                            isLoading = false
                        }
                        return
                    }
                }

                // Build config from actual tab data
                let config = TabXPCConfig(
                    tabId: processTabId.uuidString,
                    title: tab.title.isEmpty ? "New Tab" : tab.title,
                    url: tab.url?.absoluteString,
                    htmlContent: tab.htmlContent,
                    unifiedScript: tab.unifiedScript,
                    leftPanelScript: tab.leftPanelScript,
                    rightPanelScript: tab.rightPanelScript,
                    backgroundColor: tab.backgroundColor,
                    leftPanelVisible: tab.leftPanelDefaultVisible,
                    rightPanelVisible: tab.rightPanelDefaultVisible,
                    reloadKey: tab.reloadNonce,
                    contractName: tab.contractName,
                    contractVersion: tab.contractVersion,
                    permissions: tab.permissions
                )

                _ = try await processManager.getRenderer(for: processTabId, config: config, viewportSize: containerSize)

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
        guard let activeId = activeTabId else {
            NSLog("📐 [DIAG] refreshSurface: activeTabId is nil, skipping")
            return
        }
        Task {
            if let id = await processManager.getIOSurfaceID(for: activeId) {
                if id != surfaceID {
                    NSLog("📐 [DIAG] refreshSurface: surfaceID changed \(surfaceID) -> \(id), updating state")
                    await MainActor.run {
                        surfaceID = id
                    }
                } else {
                    NSLog("📐 [DIAG] refreshSurface: surfaceID unchanged (\(id))")
                }
            } else {
                NSLog("📐 [DIAG] refreshSurface: getIOSurfaceID returned nil")
            }
        }
    }

    private func updateViewportSize(_ size: CGSize) {
        guard let activeId = activeTabId else {
            NSLog("📐 [DIAG] updateViewportSize: activeTabId is nil, skipping")
            return
        }
        NSLog("📐 [DIAG] updateViewportSize: \(Int(size.width))x\(Int(size.height)) for tab \(activeId)")
        processManager.updateViewport(width: size.width, height: size.height, for: activeId)
    }
}

// MARK: - Preview

#Preview {
    MultiProcessTabView(
        tab: Tab(title: "Preview Tab"),
        containerSize: CGSize(width: 800, height: 600)
    )
    .frame(width: 800, height: 600)
}
