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
    var tabId: UUID? {
        didSet {
            // When tabId changes, update notification observer
            if oldValue != tabId {
                setupNotificationObserver()
            }
        }
    }

    /// Expected surface dimensions for intrinsicContentSize
    var surfaceWidth: CGFloat = 0
    var surfaceHeight: CGFloat = 0

    /// Tracking area for mouse movement events
    private var trackingArea: NSTrackingArea?

    /// Display link for refreshing IOSurface content
    private var displayLink: CVDisplayLink?

    /// Notification observer for direct surface updates (bypasses SwiftUI)
    private var surfaceObserver: NSObjectProtocol?

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
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setupLayer()
        setupTracking()
    }

    deinit {
        NSLog("IOSurfaceDisplayView: deinit called for tabId \(tabId?.uuidString ?? "nil")")
        stopDisplayLink()
        if let observer = surfaceObserver {
            NotificationCenter.default.removeObserver(observer)
        }
    }

    /// Set up direct notification observer for surface updates (bypasses SwiftUI state)
    private func setupNotificationObserver() {
        // Remove old observer
        if let observer = surfaceObserver {
            NotificationCenter.default.removeObserver(observer)
        }

        guard let tabId = tabId else { return }

        // Listen directly to surface update notifications
        surfaceObserver = NotificationCenter.default.addObserver(
            forName: .tabSurfaceUpdated,
            object: nil,
            queue: .main
        ) { [weak self] notification in
            guard let self = self else { return }
            guard let notifTabId = notification.userInfo?["tabId"] as? UUID,
                  notifTabId == tabId,
                  let newSurfaceID = notification.userInfo?["surfaceID"] as? UInt32,
                  newSurfaceID != 0 else { return }

            // Directly update the layer without going through SwiftUI
            self.directUpdateSurface(id: newSurfaceID)
        }
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
    private var cachedSurface: IOSurface?  // Cache the IOSurface reference
    private var needsRefresh: Bool = false  // Flag for event-driven refresh

    private func refreshSurfaceContent() {
        refreshCount += 1

        // Use cached surface, only lookup if not cached or ID changed
        guard let surface = cachedSurface, currentSurfaceID != 0 else {
            if refreshCount % 60 == 1 {
                NSLog("🖥️ [DIAG] refreshSurfaceContent: no cached surface, currentSurfaceID=\(currentSurfaceID)")
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

        // Only mark layer for redisplay - CALayer will read IOSurface when compositing
        // This is less aggressive and reduces flicker
        surfaceLayer?.setNeedsDisplay()
    }

    /// Called when Helper notifies surface update
    func markNeedsRefresh() {
        needsRefresh = true
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

    private var surfaceUpdateCount: UInt64 = 0

    /// Direct update from notification - fast path that bypasses SwiftUI
    private func directUpdateSurface(id: UInt32) {
        surfaceUpdateCount += 1

        guard id != 0 else { return }

        // Skip if same surface (shouldn't happen with double buffering, but just in case)
        // Actually, with double buffering we DO want to update even if same ID comes back
        currentSurfaceID = id

        // Look up the IOSurface by ID
        guard let surface = IOSurfaceLookup(id) else {
            if surfaceUpdateCount % 120 == 1 {
                NSLog("🖥️ [DIAG] directUpdateSurface: IOSurfaceLookup FAILED for id=\(id)")
            }
            return
        }

        cachedSurface = surface

        // Ensure layer exists
        ensureSurfaceLayerExists()

        // Update layer contents directly - no animations
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        surfaceLayer?.contents = surface
        // Use current bounds, fallback to expected size
        let currentBounds = bounds.width > 0 ? bounds : NSRect(x: 0, y: 0, width: surfaceWidth, height: surfaceHeight)
        surfaceLayer?.frame = currentBounds
        CATransaction.commit()

        // Log periodically
        if surfaceUpdateCount == 1 || surfaceUpdateCount % 120 == 0 {
            let surfacePixelWidth = IOSurfaceGetWidth(surface)
            let surfacePixelHeight = IOSurfaceGetHeight(surface)
            NSLog("🖥️ [DIAG] directUpdateSurface #\(surfaceUpdateCount): id=\(id), pixels=\(surfacePixelWidth)x\(surfacePixelHeight), bounds=\(currentBounds)")
        }
    }

    /// Ensure the surface layer exists
    private func ensureSurfaceLayerExists() {
        guard surfaceLayer == nil else { return }

        let newLayer = CALayer()
        newLayer.contentsGravity = .resize
        newLayer.magnificationFilter = .linear
        newLayer.minificationFilter = .linear
        newLayer.isOpaque = true
        layer?.addSublayer(newLayer)
        surfaceLayer = newLayer

        let screenScale = window?.backingScaleFactor ?? NSScreen.main?.backingScaleFactor ?? 2.0
        newLayer.contentsScale = screenScale
        NSLog("🖥️ [DIAG] Created surfaceLayer with contentsScale=\(screenScale)")
    }

    /// Called from SwiftUI - just sets up initial state, actual updates come via notification
    func updateSurface(id: UInt32, width: CGFloat, height: CGFloat) {
        guard id != 0 else {
            // No surface available - show placeholder
            cachedSurface = nil
            showPlaceholder()
            return
        }

        // Just update the initial surface if we don't have one yet
        // Subsequent updates come directly via notification observer
        if currentSurfaceID == 0 {
            directUpdateSurface(id: id)
        }
    }

    func refreshSurface() {
        // Just mark layer for redisplay - IOSurface content is automatically visible
        surfaceLayer?.setNeedsDisplay()
    }

    private func showPlaceholder() {
        currentSurfaceID = 0
        cachedSurface = nil
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
        .onChange(of: containerSize) { oldSize, newSize in
            NSLog("📐 [SIZE-CHANGE] containerSize changed: \(Int(oldSize.width))x\(Int(oldSize.height)) -> \(Int(newSize.width))x\(Int(newSize.height)) for tab \(tabId)")
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
               let newSurfaceID = notification.userInfo?["surfaceID"] as? UInt32,
               newSurfaceID != 0 {
                // Only update SwiftUI state once to show the view
                // Actual frame updates go directly to IOSurfaceDisplayView via its own observer
                if surfaceID == 0 {
                    NSLog("📐 [DIAG] First valid surfaceID received: \(newSurfaceID)")
                    activeTabId = tabId
                    surfaceID = newSurfaceID
                    isLoading = false
                }
                // Don't update surfaceID for subsequent frames - IOSurfaceDisplayView handles those directly
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
