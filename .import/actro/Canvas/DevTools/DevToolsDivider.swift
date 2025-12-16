import SwiftUI

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
                            let newSize = size + delta
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
                        NSCursor.resizeUpDown.push()
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

/// Container view that wraps content with a DevTools panel
/// Uses ZStack-based layout to prevent content recreation when toggling DevTools
struct DevToolsContainer<Content: View>: View {
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
            // Determine if DevTools should be visible
            let shouldShowDevTools: Bool = {
                if devTools.displayMode == .hidden || devTools.displayMode == .detached {
                    return false
                }
                // Hide docked DevTools when not in Focus mode
                if !viewModel.canvasMode.isInFocusMode {
                    return false
                }
                return true
            }()

            let isDockedBottom = devTools.dockPosition == .bottom

            // Calculate content size (shrink to make room for DevTools)
            let contentWidth: CGFloat = {
                if shouldShowDevTools && !isDockedBottom {
                    return geometry.size.width - devTools.dockedWidth - 8
                }
                return geometry.size.width
            }()

            let contentHeight: CGFloat = {
                if shouldShowDevTools && isDockedBottom {
                    return geometry.size.height - devTools.dockedHeight - 8
                }
                return geometry.size.height
            }()

            // Use ZStack with explicit positioning to keep view hierarchy stable
            ZStack(alignment: .topLeading) {
                // Main content - always at top-left, size varies
                // Use stable id to prevent SwiftUI from recreating the view
                content
                    .frame(width: contentWidth, height: contentHeight)
                    .id("devtools-container-content")

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

                            DevToolsPanel(devTools: devTools, viewModel: viewModel)
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

                            DevToolsPanel(devTools: devTools, viewModel: viewModel)
                                .frame(width: devTools.dockedWidth, height: geometry.size.height)
                        }
                    }
                }
            }
            .frame(width: geometry.size.width, height: geometry.size.height)
        }
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
