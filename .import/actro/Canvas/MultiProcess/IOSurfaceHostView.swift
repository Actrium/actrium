import SwiftUI
import AppKit
import IOSurface
import QuartzCore

// MARK: - IOSurface Host View (SwiftUI)

/// SwiftUI wrapper for displaying IOSurface content from a remote process
struct IOSurfaceHostView: NSViewRepresentable {
    let surfaceID: UInt32
    let width: CGFloat
    let height: CGFloat

    func makeNSView(context: Context) -> IOSurfaceDisplayView {
        let view = IOSurfaceDisplayView()
        view.updateSurface(id: surfaceID, width: width, height: height)
        return view
    }

    func updateNSView(_ nsView: IOSurfaceDisplayView, context: Context) {
        nsView.updateSurface(id: surfaceID, width: width, height: height)
    }
}

// MARK: - IOSurface Display View (AppKit)

/// AppKit view that displays IOSurface content using CALayer
class IOSurfaceDisplayView: NSView {
    private var surfaceLayer: CALayer?
    private var currentSurfaceID: UInt32 = 0

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setupLayer()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setupLayer()
    }

    private func setupLayer() {
        wantsLayer = true
        layer?.backgroundColor = NSColor.black.cgColor
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
            newLayer.contentsGravity = .resizeAspect
            newLayer.magnificationFilter = .nearest
            layer?.addSublayer(newLayer)
            surfaceLayer = newLayer
        }

        // Set the IOSurface as the layer's contents
        surfaceLayer?.contents = surface
        surfaceLayer?.frame = bounds

        NSLog("IOSurfaceHostView: Displaying surface ID \(id), size \(IOSurfaceGetWidth(surface))x\(IOSurfaceGetHeight(surface))")
    }

    func refreshSurface() {
        // Force the layer to redraw
        surfaceLayer?.setNeedsDisplay()
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
}

// MARK: - Multi-Process Tab View

/// A view that displays tab content from a renderer process
struct MultiProcessTabView: View {
    let tabId: UUID
    @ObservedObject var processManager = TabProcessManager.shared

    @State private var surfaceID: UInt32 = 0
    @State private var isLoading: Bool = true
    @State private var errorMessage: String?

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                if surfaceID != 0 {
                    IOSurfaceHostView(
                        surfaceID: surfaceID,
                        width: geometry.size.width,
                        height: geometry.size.height
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

    private func startRenderer() {
        guard processManager.isMultiProcessEnabled else {
            errorMessage = "Multi-process mode not available"
            isLoading = false
            return
        }

        isLoading = true
        errorMessage = nil

        Task {
            do {
                let config = TabXPCConfig(
                    tabId: tabId.uuidString,
                    title: "Test Tab",
                    backgroundColor: "#1a1a2e"
                )

                let processInfo = try await processManager.spawnProcess(for: tabId, config: config)

                // Get the IOSurface ID
                if let id = await processManager.getIOSurfaceID(for: tabId) {
                    await MainActor.run {
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
        processManager.terminateProcess(for: tabId)
        surfaceID = 0
    }

    private func refreshSurface() {
        Task {
            if let id = await processManager.getIOSurfaceID(for: tabId), id != surfaceID {
                await MainActor.run {
                    surfaceID = id
                }
            }
        }
    }

    private func updateViewportSize(_ size: CGSize) {
        processManager.updateViewport(width: size.width, height: size.height, for: tabId)
    }
}

// MARK: - Preview

#Preview {
    MultiProcessTabView(tabId: UUID())
        .frame(width: 800, height: 600)
}
