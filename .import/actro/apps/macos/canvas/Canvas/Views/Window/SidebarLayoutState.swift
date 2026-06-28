import CoreGraphics
import Combine

@MainActor
final class SidebarLayoutState: ObservableObject {
    @Published private(set) var sidebarWidth: CGFloat = 250
    @Published private(set) var splitBoundaryX: CGFloat = 250
    @Published private(set) var isSidebarCollapsed: Bool = false
    @Published private(set) var isWindowFullScreen: Bool = false
    @Published private(set) var windowWidth: CGFloat = 1200

    let collapsedToolbarWidth: CGFloat = 78

    private let changeEpsilon: CGFloat = 0.1

    func update(sidebarWidth: CGFloat, splitBoundaryX: CGFloat, isSidebarCollapsed: Bool, windowWidth: CGFloat) {
        let normalizedSidebarWidth = max(0, sidebarWidth)
        let normalizedSplitBoundaryX = max(0, splitBoundaryX)
        let normalizedWindowWidth = max(0, windowWidth)

        var changed = false
        if abs(self.sidebarWidth - normalizedSidebarWidth) > changeEpsilon {
            self.sidebarWidth = normalizedSidebarWidth
            changed = true
        }
        if abs(self.splitBoundaryX - normalizedSplitBoundaryX) > changeEpsilon {
            self.splitBoundaryX = normalizedSplitBoundaryX
            changed = true
        }
        if self.isSidebarCollapsed != isSidebarCollapsed {
            self.isSidebarCollapsed = isSidebarCollapsed
            changed = true
        }
        if abs(self.windowWidth - normalizedWindowWidth) > changeEpsilon {
            self.windowWidth = normalizedWindowWidth
            changed = true
        }

        _ = changed
    }

    func updateWindowFullScreen(_ isFullScreen: Bool) {
        guard isWindowFullScreen != isFullScreen else { return }
        isWindowFullScreen = isFullScreen
    }
}
