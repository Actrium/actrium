import SwiftUI

/// Draggable divider for resizing the sidebar panel
struct SidebarDivider: View {
    @Binding var width: CGFloat
    let minWidth: CGFloat
    let maxWidth: CGFloat

    @State private var isDragging = false

    var body: some View {
        ZStack {
            // Visual divider line (1px)
            Rectangle()
                .fill(Color(nsColor: .separatorColor))
                .frame(width: 1)

            // Invisible drag handle (8px hit area)
            Rectangle()
                .fill(Color.clear)
                .frame(width: 8)
                .contentShape(Rectangle())
                .gesture(
                    DragGesture(minimumDistance: 1)
                        .onChanged { value in
                            isDragging = true
                            // Positive translation = drag right = increase width
                            let delta = value.translation.width
                            let newWidth = round(width + delta)
                            let clampedWidth = max(minWidth, min(maxWidth, newWidth))
                            width = clampedWidth
                            // Sync with TitlebarManager
                            TitlebarManager.shared.updateSidebarWidth(clampedWidth)
                        }
                        .onEnded { _ in
                            isDragging = false
                        }
                )
                .onHover { hovering in
                    if hovering {
                        NSCursor.resizeLeftRight.push()
                    } else {
                        NSCursor.pop()
                    }
                }
        }
        .frame(width: 8)
        .background(isDragging ? Color.accentColor.opacity(0.2) : Color.clear)
    }
}

#Preview {
    struct PreviewWrapper: View {
        @State var sidebarWidth: CGFloat = 250

        var body: some View {
            HStack(spacing: 0) {
                Color.blue.opacity(0.3)
                    .frame(width: sidebarWidth)

                SidebarDivider(
                    width: $sidebarWidth,
                    minWidth: 180,
                    maxWidth: 400
                )

                Color.green.opacity(0.3)
            }
            .frame(width: 800, height: 500)
        }
    }

    return PreviewWrapper()
}
