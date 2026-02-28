import SwiftUI

// MARK: - Viewport Size Environment

private struct ViewportSizeKey: EnvironmentKey {
    static let defaultValue: CGSize = .zero
}

extension EnvironmentValues {
    var viewportSize: CGSize {
        get { self[ViewportSizeKey.self] }
        set { self[ViewportSizeKey.self] = newValue }
    }
}

// MARK: - Workspace Main Viewport

/// The main viewport container - measures actual size and provides it via Environment
struct WorkspaceMainViewport<Content: View>: View {
    @ViewBuilder let content: () -> Content

    var body: some View {
        GeometryReader { geometry in
            // 🔑 在源头取整，避免下游不同地方取整方式不一致导致 1px 抖动
            // 使用 floor() 向下取整，确保视口不超过实际可用空间
            let roundedSize = CGSize(
                width: floor(geometry.size.width),
                height: floor(geometry.size.height)
            )

            // 🔍 详细日志：每次 GeometryReader 重新评估都记录
            let _ = {
                // 写入共享日志文件
                let timestamp = String(format: "%02d:%02d:%02d.%03d",
                    Calendar.current.component(.hour, from: Date()),
                    Calendar.current.component(.minute, from: Date()),
                    Calendar.current.component(.second, from: Date()),
                    Int(Date().timeIntervalSince1970.truncatingRemainder(dividingBy: 1) * 1000))
                let line = "[\(timestamp)] [HOST] 📐 [GeometryReader] raw=\(Int(geometry.size.width))x\(Int(geometry.size.height)) → rounded=\(Int(roundedSize.width))x\(Int(roundedSize.height))\n"
                let logPath = "/tmp/canvas_ipc_debug.log"
                if let handle = FileHandle(forWritingAtPath: logPath) {
                    handle.seekToEndOfFile()
                    if let data = line.data(using: .utf8) {
                        handle.write(data)
                    }
                    handle.closeFile()
                }
            }()

            content()
                .environment(\.viewportSize, roundedSize)
        }
    }
}

// MARK: - DevTools Divider

/// Draggable divider for resizing the DevTools panel
struct DevToolsDivider: View {
    @Binding var size: CGFloat
    let minSize: CGFloat
    let maxSize: CGFloat
    let axis: Axis

    @State private var isDragging = false

    var body: some View {
        ZStack {
            // Visual divider line
            Rectangle()
                .fill(Color(nsColor: .separatorColor))
                .frame(
                    width: axis == .horizontal ? 1 : nil,
                    height: axis == .vertical ? 1 : nil
                )

            // Invisible drag handle (larger hit area)
            Rectangle()
                .fill(Color.clear)
                .frame(
                    width: axis == .horizontal ? 8 : nil,
                    height: axis == .vertical ? 8 : nil
                )
                .contentShape(Rectangle())
                .gesture(
                    DragGesture(minimumDistance: 1)
                        .onChanged { value in
                            isDragging = true
                            let delta = axis == .vertical ? -value.translation.height : -value.translation.width
                            // 🔑 使用 round() 取整，避免 dockedWidth/dockedHeight 产生小数
                            // 小数的 docked 尺寸会导致 contentWidth/contentHeight 计算出现精度问题
                            let newSize = round(size + delta)
                            size = max(minSize, min(maxSize, newSize))
                        }
                        .onEnded { _ in
                            isDragging = false
                            // Save the new size
                            DevToolsManager.shared.saveSettings()
                        }
                )
                .onHover { hovering in
                    if hovering {
                        // 根据分隔条方向选择正确的光标
                        let cursor = axis == .vertical ? NSCursor.resizeUpDown : NSCursor.resizeLeftRight
                        cursor.push()
                    } else {
                        NSCursor.pop()
                    }
                }
        }
        .frame(
            width: axis == .horizontal ? 8 : nil,
            height: axis == .vertical ? 8 : nil
        )
        .background(isDragging ? Color.accentColor.opacity(0.2) : Color.clear)
    }
}

/// Workspace layout: main content area + optional docked DevTools panel
/// Uses ZStack-based layout to prevent content recreation when toggling DevTools
struct WorkspaceView<Content: View>: View {
    @ObservedObject var devTools: DevToolsManager
    @ObservedObject var viewModel: BrowserViewModel
    let content: Content

    init(
        devTools: DevToolsManager,
        viewModel: BrowserViewModel,
        @ViewBuilder content: () -> Content
    ) {
        self.devTools = devTools
        self.viewModel = viewModel
        self.content = content()
    }

    var body: some View {
        GeometryReader { geometry in
            // Determine if DevTools panel should occupy space (regardless of mode)
            let shouldShowDevTools: Bool = {
                if devTools.displayMode == .hidden || devTools.displayMode == .detached {
                    return false
                }
                // Always show docked panel to keep viewport stable
                return true
            }()

            // Determine if DevTools content is active (only in Focus mode)
            let isDevToolsActive = viewModel.canvasMode.isInFocusMode

            let isDockedBottom = devTools.dockPosition == .bottom

            // Calculate content size (shrink to make room for DevTools)
            // 🔑 使用 floor() 取整，与 WorkspaceMainViewport 保持一致，避免小数导致布局抖动
            let contentWidth: CGFloat = {
                if shouldShowDevTools && !isDockedBottom {
                    return floor(geometry.size.width - devTools.dockedWidth - 8)
                }
                return floor(geometry.size.width)
            }()

            let contentHeight: CGFloat = {
                if shouldShowDevTools && isDockedBottom {
                    return floor(geometry.size.height - devTools.dockedHeight - 8)
                }
                return floor(geometry.size.height)
            }()

            // 🔍 写入共享日志文件
            let _ = {
                let timestamp = String(format: "%02d:%02d:%02d.%03d",
                    Calendar.current.component(.hour, from: Date()),
                    Calendar.current.component(.minute, from: Date()),
                    Calendar.current.component(.second, from: Date()),
                    Int(Date().timeIntervalSince1970.truncatingRemainder(dividingBy: 1) * 1000))
                let line = "[\(timestamp)] [HOST] 📐 [WorkspaceView] geometry=\(Int(geometry.size.width))x\(Int(geometry.size.height)) devTools=\(shouldShowDevTools ? 1 : 0) contentSize=\(Int(contentWidth))x\(Int(contentHeight))\n"
                let logPath = "/tmp/canvas_ipc_debug.log"
                if let handle = FileHandle(forWritingAtPath: logPath) {
                    handle.seekToEndOfFile()
                    if let data = line.data(using: .utf8) {
                        handle.write(data)
                    }
                    handle.closeFile()
                }
            }()

            // Use ZStack with explicit positioning to keep view hierarchy stable
            ZStack(alignment: .topLeading) {
                // Main content wrapped in viewport - provides viewportSize via Environment
                WorkspaceMainViewport {
                    content
                }
                .frame(width: contentWidth, height: contentHeight)
                .id("workspace-main-viewport")

                // DevTools panel - positioned based on dock position
                if shouldShowDevTools {
                    if isDockedBottom {
                        VStack(spacing: 0) {
                            Spacer()
                                .frame(height: contentHeight)

                            DevToolsDivider(
                                size: $devTools.dockedHeight,
                                minSize: DevToolsManager.minDockedHeight,
                                maxSize: min(DevToolsManager.maxDockedHeight, geometry.size.height - 100),
                                axis: .vertical
                            )

                            DevToolsPanel(devTools: devTools, viewModel: viewModel, isActive: isDevToolsActive)
                                .frame(width: geometry.size.width, height: devTools.dockedHeight)
                        }
                    } else {
                        HStack(spacing: 0) {
                            Spacer()
                                .frame(width: contentWidth)

                            DevToolsDivider(
                                size: $devTools.dockedWidth,
                                minSize: DevToolsManager.minDockedWidth,
                                maxSize: min(DevToolsManager.maxDockedWidth, geometry.size.width - 100),
                                axis: .horizontal
                            )

                            DevToolsPanel(devTools: devTools, viewModel: viewModel, isActive: isDevToolsActive)
                                .frame(width: devTools.dockedWidth, height: geometry.size.height)
                        }
                    }
                }
            }
            .frame(width: geometry.size.width, height: geometry.size.height)
        }
        // 🔑 忽略 safe area，让 GeometryReader 获取完整的可用空间
        // 这样 ViewBridge 内容可以正确填充到 titlebar 下方
        // 只忽略顶部 safe area (titlebar)，避免覆盖 sidebar
        // .ignoresSafeArea(.container, edges: .top) // 暂时完全移除，确保完全尊重 sidebar 和 titlebar 区域
    }
}

// MARK: - Preview

#Preview("DevTools Divider - Bottom") {
    struct PreviewWrapper: View {
        @State var height: CGFloat = 200

        var body: some View {
            VStack(spacing: 0) {
                Color.blue.opacity(0.3)

                DevToolsDivider(
                    size: $height,
                    minSize: 100,
                    maxSize: 400,
                    axis: .vertical
                )

                Color.green.opacity(0.3)
                    .frame(height: height)
            }
            .frame(width: 600, height: 500)
        }
    }

    return PreviewWrapper()
}
