import SwiftUI
import Kingfisher

private let logger = AppLogger.ui
#if os(macOS)
import AppKit
typealias PlatformColor = NSColor
typealias PlatformFont = NSFont
#elseif os(iOS)
import UIKit
typealias PlatformColor = UIColor
typealias PlatformFont = UIFont
#endif
import UniformTypeIdentifiers

// MARK: - Main View

/// Large canvas structure - Unified Actrium View
///
/// Core concepts:
/// - All Tab The cards are all on a large canvas, fixed.
/// - Overview Mode: narrow canvas to show all card grids
/// - Focus Mode: Magnify the canvas to fill the screen with the target card
/// - Only pass scale + offset Convert Implementation Mode Switch
struct UnifiedActriumView: View {
    @ObservedObject var viewModel: BrowserViewModel
    @ObservedObject var historyVM: HistoryViewModel

    // Mode is stored in viewModel.actriumMode to survive view recreation
    // (e.g., when DevTools layout changes from docked to detached)

    @State private var isACTRDropTarget: Bool = false
    @State private var showingBookmarkSheet: Bool = false

    // Toolbar Status
    @State private var addressInputText: String = ""
    @State private var isAddressBarEditing: Bool = false
    @State private var isAddressBarFirstResponder: Bool = false
    @State private var eventMonitor: Any?

    // Search Tabs Status
    @State private var searchTabsText: String = ""
    @FocusState private var isSearchTabsFocused: Bool

    // Filter by Searching Text tabs
    private var filteredTabs: [Tab] {
        guard !searchTabsText.isEmpty else {
            return viewModel.tabs
        }
        return viewModel.tabs.filter { tab in
            tab.title.localizedCaseInsensitiveContains(searchTabsText)
        }
    }

    // For Filtered tabs Create a new position map (rearranged from 0 Start continuous)
    private var filteredTabPositions: [UUID: Int] {
        guard !searchTabsText.isEmpty else {
            return viewModel.tabSlotPositions
        }
        var newPositions: [UUID: Int] = [:]
        for (index, tab) in filteredTabs.enumerated() {
            newPositions[tab.id] = index
        }
        return newPositions
    }

    var body: some View {
        GeometryReader { geometry in
            let viewportSize = geometry.size
            // DEBUG: Tracking GeometryReader Size provided (in US$) print Make sure you're visible.
            // Extremely narrow/Avoiding Very Short Window 0 The size makes the matrix strange.
            let safeViewportSize = CGSize(
                width: max(viewportSize.width, 1),
                height: max(viewportSize.height, 1)
            )

            // Calculate real through Platform Window toolbar Height
            let measuredToolbarHeight: CGFloat = {
                #if os(macOS)
                guard let window = NSApp.keyWindow ?? NSApp.windows.first,
                      let contentView = window.contentView else {
                    return 52 // Default value
                }
                let windowContentHeight = contentView.frame.height
                let calculated = windowContentHeight - viewportSize.height
                // Reasonable scope checks
                return (calculated > 30 && calculated < 100) ? calculated : 52
                #elseif os(iOS)
                // iOS Use fixed navigation bar height
                return 44
                #endif
            }()

            // Using measured toolbar Height
            let toolbarHeight = measuredToolbarHeight
            let contentSize = safeViewportSize
            // Titlebar removed, card ratio = Portrait ratio
            let titleBarRatio: CGFloat = 0
            let cardAspectRatio = safeViewportSize.width / safeViewportSize.height
            let gridResult = gridLayout(
                for: contentSize,
                cardAspectRatio: cardAspectRatio,
                cardCount: filteredTabs.count
            )
            let surfaceSize = gridResult.surfaceSize

            // Calculate Changes
            let transform: (scale: CGFloat, offset: CGPoint) = {
                switch viewModel.actriumMode {
                case .overview:
                    return calculateOverviewTransform(
                        surfaceSize: surfaceSize,
                        viewportSize: viewportSize
                    )
                case .focus(let tabId):
                    return calculateFocusTransform(
                        for: tabId,
                        gridResult: gridResult,
                        viewportSize: viewportSize,
                        titleBarRatio: titleBarRatio
                    )
                }
            }()

            ZStack(alignment: .topTrailing) {
                // Background
                #if os(macOS)
                Color(nsColor: NSColor(white: 0.1, alpha: 0.95))
                #elseif os(iOS)
                Color.black
                #endif

                // Unique Rendering Actrium(Overview and Focus Share the same view, pass transform Animated Switch)
                // Every one. Tab The card contains a separate card. RCTRootView♪ Get real scaling ♪
                ActriumView(
                    tabs: filteredTabs,
                    tabSlotPositions: filteredTabPositions,
                    positions: gridResult.positions,
                    cardWidth: gridResult.cardWidth,
                    cardHeight: gridResult.cardHeight,
                    titleBarRatio: titleBarRatio,
                    toolbarHeight: toolbarHeight,
                    currentScale: transform.scale,
                    mode: viewModel.actriumMode,
                    showNewTabButton: searchTabsText.isEmpty,
                    viewportSize: safeViewportSize,
                    viewModel: viewModel,
                    historyVM: historyVM,
                    onSelectTab: { tabId in
                        switchToFocus(tabId: tabId)
                    },
                    onSwapPositions: { tabId1, tabId2 in
                        viewModel.swapSlotPositions(tabId1: tabId1, tabId2: tabId2)
                    },
                    onNewTab: {
                        // New button in Overview Create only in mode tab,do not switch to Focus
                        viewModel.addNewTab(focus: viewModel.actriumMode.isInFocusMode)
                    },
                    onCloseTab: { tabId in
                        withAnimation {
                            viewModel.closeTab(id: tabId)
                        }
                    }
                )
                .frame(width: surfaceSize.width, height: surfaceSize.height)
                // Prevention scale Yes 0 It leads to a strange matrix.
                .scaleEffect(max(transform.scale, 0.001), anchor: .topLeading)
                .offset(x: transform.offset.x, y: transform.offset.y)
                // simultaneousGesture I won't. UIKit(RNThe #Geoscope competition,
                // Avoid blocking. UITextField Wait. UIResponder Touch events of chains
                .simultaneousGesture(
                    TapGesture().onEnded {
                        if case .focus = viewModel.actriumMode, isAddressBarEditing {
                            isAddressBarEditing = false
                            isAddressBarFirstResponder = false
                        }
                    }
                )

                // Glass View Cover processing drag (transparent layer,hitTest Back nil ♪ Let the click through ♪
                GlassViewOnlyRepresentable(
                    onDropACTR: { url in
                        Task { @MainActor in
                            if case .overview = viewModel.actriumMode {
                                searchTabsText = ""
                            }

                            // Check if it's a directory or .actr file
                            var isDirectory: ObjCBool = false
                            if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) {
                                if isDirectory.boolValue {
                                    viewModel.loadFromDevDirectory(url)
                                } else {
                                    viewModel.importACTRPackage(from: url)
                                }
                            }

                            // Autotoggle to New after Import Tab It's... Focus Mode
                            if let newTabId = viewModel.currentTabId {
                                switchToFocus(tabId: newTabId)
                            }
                        }
                    }
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)

                // Note:RN Content Layer Moved to Every TabCardView Internal
                // Every one. Tab There's an independent one. RCTRootView♪ Get real scaling ♪
            }
            .ignoresSafeArea() // Core change: Allow content stacks to ignore the security zone
            .toolbar {
                ToolbarItemGroup(placement: .principal) {
                    let totalWidth = geometry.size.width
                    let addressBarContainerWidth = totalWidth * 0.5

                    HStack {
                        Spacer(minLength: 0)

                        // Overview Mode: Show empty status;Focus Mode: Show Address Bar
                        if case .overview = viewModel.actriumMode {
                            // Overview Empty place in mode
                            Color.clear
                                .frame(width: addressBarContainerWidth, height: 28)
                        } else {
                            AddressBarContainer(
                                titleText: .constant(viewModel.currentTab?.title ?? TM.active.blackTab()),
                                inputText: $addressInputText,
                                isEditing: $isAddressBarEditing,
                                isFirstResponder: $isAddressBarFirstResponder,
                                toggleLeftPanel: {
                                    withAnimation(.easeInOut(duration: 0.2)) {
                                        viewModel.toggleLeftPanelForCurrentTab()
                                    }
                                },
                                refreshAction: {
                                    viewModel.reloadCurrentTab()
                                }
                            )
                            .frame(width: addressBarContainerWidth)
                        }

                        Spacer(minLength: 0)
                    }
                    .frame(maxWidth: .infinity)
                }

                ToolbarItemGroup(placement: .primaryAction) {
                    Button(action: {}) {
                        Image(systemName: "square.and.arrow.up")
                    }
                    .help("Share")

                    Button(action: {
                        withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
                            // Toolbar New: In Overview Down does not automatically enter Focus
                            _ = viewModel.addNewTab(focus: viewModel.actriumMode.isInFocusMode)
                        }
                    }) {
                        Image(systemName: "plus")
                    }
                    .help("New Tab")

                    Button(action: toggleMode) {
                        Image(systemName: "square.grid.2x2")
                    }
                    .help("Tab Overview")
                }
            }
        }
        .ignoresSafeArea() // Make sure you get the full screen physical dimensions.
        .onChange(of: viewModel.actriumMode) { oldValue, newValue in
            // Enter Overview Auto Focus Search Box on Mode
            if case .overview = newValue {
                // Delay one frame to make sure the search box is rendered
                DispatchQueue.main.async {
                    isSearchTabsFocused = true
                }
            } else {
                // Leave Overview Empty search text while mode
                searchTabsText = ""
                isSearchTabsFocused = false
            }
        }
        .onAppear {
            // Default entry on startup Focus Mode (if any) tab)
            if let tabId = viewModel.currentTabId {
                logger.notice("🚀 [UnifiedActriumView] Direct access on startup Focus Mode")
                DispatchQueue.main.async {
                    withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
                        viewModel.actriumMode = .focus(tabId: tabId)
                    }
                }
            }

            #if os(macOS)
            // Listening ESC Keys (only) macOS)
            eventMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
                if event.keyCode == 53 { // ESC key code
                    if case .overview = viewModel.actriumMode {
                        switchToFocus(tabId: viewModel.currentTabId ?? viewModel.tabs.first!.id)
                        return nil
                    }

                    if isAddressBarEditing {
                        isAddressBarEditing = false
                        isAddressBarFirstResponder = false
                        return nil
                    }
                }
                return event
            }
            #endif
        }
        .onDisappear {
            #if os(macOS)
            // Clear event listening (only) macOS)
            if let monitor = eventMonitor {
                NSEvent.removeMonitor(monitor)
            }
            #endif
        }
    }

    // MARK: - Mode Switching

    private func toggleMode() {
        if case .overview = viewModel.actriumMode {
            // From Overview Switch to First Tab
            if let firstTab = viewModel.tabs.first {
                switchToFocus(tabId: firstTab.id)
            }
        } else {
            // From Focus Switch to Overview
            switchToOverview()
        }
    }

    private func switchToOverview() {
        guard case .focus = viewModel.actriumMode else { return }

        withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
            viewModel.actriumMode = .overview
        }
    }

    private func switchToFocus(tabId: UUID) {
        guard viewModel.actriumMode != .focus(tabId: tabId) else { return }

        // Make sure you switch to Focus , and update the currently selected tab
        viewModel.currentTabId = tabId
        viewModel.actriumMode = .focus(tabId: tabId)
    }

    // MARK: - Transform Calculations

    /// Overview Mode change: Scale the canvas to fit the view, horizontally centered, top alignment
    private func calculateOverviewTransform(
        surfaceSize: CGSize,
        viewportSize: CGSize
    ) -> (scale: CGFloat, offset: CGPoint) {
        guard surfaceSize.width > 0 && surfaceSize.height > 0 else {
            return (1.0, .zero)
        }

        // Scale: Jean Actrium Adaptive vision (no zoom in, only shrink in)
        let scaleX = viewportSize.width / surfaceSize.width
        let scaleY = viewportSize.height / surfaceSize.height
        let scale = max(min(scaleX, scaleY, 1.0), 0.001)

        // Centre Horizontally, Top Alignment
        let scaledWidth = surfaceSize.width * scale
        let offsetX = (viewportSize.width - scaledWidth) / 2
        let offsetY: CGFloat = 0  // Top Alignment without Adding Vertical Offset

        guard offsetX.isFinite && offsetY.isFinite && scale.isFinite else {
            return (1.0, .zero)
        }

        return (scale, CGPoint(x: offsetX, y: offsetY))
    }

    /// Focus Mode change: Magnify the canvas to fill the target card with a visual window
    private func calculateFocusTransform(
        for tabId: UUID,
        gridResult: (positions: [CGPoint], cardWidth: CGFloat, cardHeight: CGFloat, surfaceSize: CGSize),
        viewportSize: CGSize,
        titleBarRatio: CGFloat
    ) -> (scale: CGFloat, offset: CGPoint) {
        guard let position = viewModel.tabSlotPositions[tabId],
              position < gridResult.positions.count else {
            return (1.0, .zero)
        }

        let cardPosition = gridResult.positions[position]
        let cardFullRect = CGRect(
            x: cardPosition.x,
            y: cardPosition.y,
            width: gridResult.cardWidth,
            height: gridResult.cardHeight
        )

        // Card content area (leaf title bar)
        // Use imported titleBarRatio, the force multiplier is already included.
        let titleBarHeight = cardFullRect.height * titleBarRatio
        let cardContentRect = CGRect(
            x: cardFullRect.origin.x,
            y: cardFullRect.origin.y + titleBarHeight,
            width: cardFullRect.width,
            height: cardFullRect.height - titleBarHeight
        )

        guard cardContentRect.width > 0 else {
            return (1.0, .zero)
        }

        // Scale: keep the card content area width = The width of the view
        let scale = max(viewportSize.width / cardContentRect.width, 0.001)

        // Offset: Align Top Left
        let offsetX = -cardContentRect.origin.x * scale
        let offsetY = -cardContentRect.origin.y * scale

        guard offsetX.isFinite && offsetY.isFinite && scale.isFinite else {
            return (1.0, .zero)
        }

        return (scale, CGPoint(x: offsetX, y: offsetY))
    }

    // MARK: - Grid Layout Calculations

    /// Bottom service line height (%2)Overview Shows in the bottom of the card in mode)
    private let floatingTitleBarHeight: CGFloat = 56

    /// Calculate spacing (between cards) and vertical margins
    /// - Spacing = windowWidth / (Number of cards × 7)Lower limit 16px
    /// - Vertical Space = Horizontal spacing (lower service bars inside cards, no additional space required)
    /// - Vertical Margins = Spacing × 1.25
    private func calculateSpacing(windowWidth: CGFloat, totalCards: Int) -> (hSpacing: CGFloat, vSpacing: CGFloat, verticalMargin: CGFloat) {
        let effectiveCards = max(totalCards, 4)
        let baseSpacing = max(16, windowWidth / (CGFloat(effectiveCards) * 7))
        let vSpacing = baseSpacing  // Vertical spacing does not require additional height
        let verticalMargin = baseSpacing * 1.25  // Top Margin
        return (baseSpacing, vSpacing, verticalMargin)
    }

    /// Grid layout calculation
    /// - Initial capacity 4(2×2) Number of cards ≤ 4 Force 2×2 Layout
    /// - Horizontal Margin = spacing × 1.25(Ensures by card size)
    private func gridLayout(
        for size: CGSize,
        cardAspectRatio: CGFloat,
        cardCount: Int
    ) -> (positions: [CGPoint], cardWidth: CGFloat, cardHeight: CGFloat, surfaceSize: CGSize) {
        let totalCards = cardCount + 1  // Organisation [+] button
        let minCardWidth: CGFloat = 120  // Prevents the card width of windows to be as low as 0 It causes the transformation matrix to deteriorate.

        guard cardAspectRatio.isFinite && cardAspectRatio > 0 else {
            return gridLayout(for: size, cardAspectRatio: 16.0 / 9.0, cardCount: cardCount)
        }
        guard totalCards > 0 else {
            return ([], 0, 0, .zero)
        }

        let initialCapacity = 4
        let isForced2x2 = totalCards <= initialCapacity
        let (hSpacing, vSpacing, verticalMargin) = calculateSpacing(windowWidth: size.width, totalCards: max(totalCards, initialCapacity))

        // Compute Best Layout
        func calculateOptimalLayout() -> (columns: Int, rows: Int, cardWidth: CGFloat, cardHeight: CGFloat) {
            var bestColumns = 1
            var bestCardWidth: CGFloat = 0
            var bestCardHeight: CGFloat = 0

            let minColumns = isForced2x2 ? 2 : 1
            let maxColumns = isForced2x2 ? 2 : totalCards

            for columns in minColumns...maxColumns {
                let rows = isForced2x2 ? 2 : Int(ceil(Double(totalCards) / Double(columns)))

                // Horizontal bound: Margins = 1.25 × hSpacing
                // cardWidth = (width - (columns + 1.5) × hSpacing) / columns
                let cardWidthFromH = (size.width - CGFloat(columns + 1) * hSpacing - 0.5 * hSpacing) / CGFloat(columns)

                // Vertical bound (use) vSpacing, with suspended title bar space)
                let availableHeight = size.height - verticalMargin * 2
                let cardHeightFromV = (availableHeight - CGFloat(rows - 1) * vSpacing) / CGFloat(rows)
                let cardWidthFromV = cardHeightFromV * cardAspectRatio

                let cardWidth = min(cardWidthFromH, cardWidthFromV)
                let cardHeight = cardWidth / cardAspectRatio

                if cardWidth > bestCardWidth {
                    bestColumns = columns
                    bestCardWidth = cardWidth
                    bestCardHeight = cardHeight
                }
            }

            let rows = isForced2x2 ? 2 : Int(ceil(Double(totalCards) / Double(bestColumns)))
            return (bestColumns, rows, bestCardWidth, bestCardHeight)
        }

        let layout = calculateOptimalLayout()
        let columns = layout.columns
        let rows = layout.rows
        // When Window is too narrow cardWidth Could be closer to 0, resulting in scale Matrix Odd;Land Bottom
        let cardWidth = max(layout.cardWidth, minCardWidth)
        let cardHeight = cardWidth / cardAspectRatio

        // 2×2 Keep while forced layout 4 Position
        let positionCount = isForced2x2 ? initialCapacity : totalCards

        // Horizontal margin: Fill the width of the view mouth and eliminate middle deviation
        let horizontalMargin = max((size.width - CGFloat(columns) * cardWidth - CGFloat(columns - 1) * hSpacing) / 2, 0)

        // Calculating card position (use vSpacing As line spacing)
        var positions: [CGPoint] = []
        for index in 0..<positionCount {
            let col = index % columns
            let row = index / columns
            let x = horizontalMargin + CGFloat(col) * (cardWidth + hSpacing)
            let y = verticalMargin + CGFloat(row) * (cardHeight + vSpacing)
            positions.append(CGPoint(x: x, y: y))
        }

        // Actrium Width = viewport Width
        let surfaceSize = CGSize(
            width: size.width,
            height: verticalMargin * 2 + CGFloat(rows) * cardHeight + CGFloat(rows - 1) * vSpacing
        )

        return (positions, cardWidth, cardHeight, surfaceSize)
    }

}

// MARK: - Actrium View

private struct ActriumView: View {
    let tabs: [Tab]
    let tabSlotPositions: [UUID: Int]
    let positions: [CGPoint]
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    let titleBarRatio: CGFloat  // Headbar height as a percentage of card height
    let toolbarHeight: CGFloat  // toolbar Actual height
    let currentScale: CGFloat
    let mode: ActriumMode
    let showNewTabButton: Bool  // Whether to show [+] button (hidden while searching for filters)
    let viewportSize: CGSize    // Visual dimensions for RN Render
    // Add: for creation SingleTabRootView
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel?
    let onSelectTab: (UUID) -> Void
    let onSwapPositions: (UUID?, UUID?) -> Void
    let onNewTab: () -> Void
    let onCloseTab: (UUID) -> Void

    @State private var hoveringOverTabId: UUID?
    @State private var swappingTabId: UUID?  // The card in the exchange (for faded animation)
    @State private var dropTargetSlotIndexFixed: Int?  // Target slot b

    // Build Show Item List (Sorted by Location)
    // [+] Buttons to the last. tabLocation = tabs.count)
    // 2×2 Example layout (in1 tab): Tab - In position. 0, [+] - In position. 1Location 2-3 empty
    private var displayItems: [(position: Int, item: DisplayItem)] {
        var items: [(Int, DisplayItem)] = []

        // Add All tabs
        for tab in tabs {
            if let position = tabSlotPositions[tab.id] {
                items.append((position, .tab(tab)))
            }
        }

        // Sort by Location
        let sorted = items.sorted { $0.0 < $1.0 }

        // [+] Buttons to the last. tab(only shown if not searched)
        var result = sorted
        if showNewTabButton {
            result.append((tabs.count, .newTabButton))
        }

        return result
    }


    var body: some View {
        ZStack(alignment: .topLeading) {
            // First floor: Card layer (mobility, power)
            ForEach(displayItems, id: \.item.id) { displayItem in
                let itemPosition = displayItem.position
                let position = positions[itemPosition]

                switch displayItem.item {
                case .tab(let tab):
                    let isHovering = hoveringOverTabId == tab.id
                    let isSwapping = swappingTabId == tab.id
                    let isFocusedCard: Bool = {
                        if case .focus(let id) = mode { return id == tab.id }
                        return false
                    }()
                    let isOverviewMode: Bool = {
                        if case .overview = mode { return true }
                        return false
                    }()
                    let baseCard = TabCardView(
                        tab: tab,
                        cardWidth: cardWidth,
                        cardHeight: cardHeight,
                        titleBarRatio: titleBarRatio,
                        toolbarHeight: toolbarHeight,
                        currentScale: currentScale,
                        isFocused: isFocusedCard,
                        isHovering: isHovering,
                        isSwapping: isSwapping,
                        viewModel: viewModel,
                        historyVM: historyVM,
                        viewportSize: viewportSize,
                        onSelect: { onSelectTab(tab.id) },
                        onClose: { onCloseTab(tab.id) }
                    )
                    Group {
                        // Drag Sort Only Overview Enabled under Mode
                        // Focus All cards in mode are free of drag and drag gestures, avoiding the signal recognition device. RN Touch events of content
                        if isOverviewMode {
                            baseCard
                                .draggable(tab.id.uuidString) {
                                    TabDragPreviewView(tab: tab, cardWidth: cardWidth, cardHeight: cardHeight)
                                }
                                .dropDestination(for: String.self) { items, _ in
                                    guard let sourceIdStr = items.first,
                                          let sourceId = UUID(uuidString: sourceIdStr),
                                          sourceId != tab.id else { return false }
                                    swappingTabId = tab.id
                                    withAnimation(.spring(response: 0.35, dampingFraction: 1.0, blendDuration: 0)) {
                                        onSwapPositions(sourceId, tab.id)
                                        hoveringOverTabId = nil
                                    }
                                    DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) {
                                        swappingTabId = nil
                                        dropTargetSlotIndexFixed = nil
                                    }
                                    return true
                                } isTargeted: { targeted in
                                    if targeted {
                                        hoveringOverTabId = tab.id
                                        dropTargetSlotIndexFixed = tabSlotPositions[tab.id]
                                    } else if hoveringOverTabId == tab.id {
                                        hoveringOverTabId = nil
                                        dropTargetSlotIndexFixed = nil
                                    }
                                }
                        } else {
                            baseCard
                        }
                    }
                    .zIndex(isFocusedCard ? 1000 : Double(itemPosition))
                    .position(
                        x: position.x + cardWidth / 2,
                        y: position.y + cardHeight / 2
                    )

                case .newTabButton:
                    NewTabCardView(
                        cardWidth: cardWidth,
                        cardHeight: cardHeight,
                        onTap: onNewTab
                    )
                    .position(
                        x: position.x + cardWidth / 2,
                        y: position.y + cardHeight / 2
                    )
                }
            }

            // Second level: slot border layer (top level, not covered by cards)
            // Disable all animations: slots and borders should not have any animation effects
            ForEach(0..<positions.count, id: \.self) { slotIndex in
                SlotView(
                    cardWidth: cardWidth,
                    cardHeight: cardHeight,
                    isDropTarget: dropTargetSlotIndexFixed == slotIndex
                )
                .position(
                    x: positions[slotIndex].x + cardWidth / 2,
                    y: positions[slotIndex].y + cardHeight / 2
                )
                .zIndex(10000)  // Super tall. zIndexMake sure the border remains at the top of the top floor, not covered by any card
                .animation(nil, value: dropTargetSlotIndexFixed)
                .allowsHitTesting(false)  // Border does not respond to the click, lets the click go through the lower card
            }
        }
    }
}

// MARK: - Slot View (Slot View, show the frame when drag)

private struct SlotView: View {
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    let isDropTarget: Bool  // Whether or not to target slots (show solid line borders)

    var body: some View {
        Rectangle()
            .fill(Color.clear)
            .frame(width: cardWidth, height: cardHeight)
            .overlay(
                Group {
                    if isDropTarget {
                        // Target: Green line
                        RoundedRectangle(cornerRadius: 10)
                            .strokeBorder(
                                Color.green.opacity(0.7),
                                style: StrokeStyle(lineWidth: 2)
                            )
                    }
                }
            )
            .transaction { tx in
                // Disable all hidden animations (replaces abandoned ones) .animation(nil))
                tx.animation = nil
            }
    }
}

// Show item type
private enum DisplayItem {
    case tab(Tab)
    case newTabButton

    // Provide stability IDJean. SwiftUI Can track view moves instead of content replacement
    var id: String {
        switch self {
        case .tab(let tab):
            return tab.id.uuidString
        case .newTabButton:
            return "newTabButton"
        }
    }
}

// MARK: - Address Bar Container

private struct AddressBarContainer: View {
    @Binding var titleText: String
    @Binding var inputText: String
    @Binding var isEditing: Bool
    @Binding var isFirstResponder: Bool
    var toggleLeftPanel: () -> Void
    var refreshAction: () -> Void

    private let placeholder = "Search from bookmark or enter app id"
    @State private var focusRingScale: CGFloat = 1.0
    @State private var focusRingOpacity: Double = 0.0

    var body: some View {
        Group {
            if isEditing {
                // Edit Status: Magnifying Mirror + Input Box
                HStack(spacing: 10) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(Color.white.opacity(0.85))
                        .frame(width: 16, height: 16)

                    FocusableTextField(
                        text: $inputText,
                        placeholder: placeholder,
                        isFirstResponder: $isFirstResponder
                    )
                }
            } else {
                // Non-edited status: left and right icons + Intermediate title
                HStack(spacing: 10) {
                    Button(action: toggleLeftPanel) {
                        Image(systemName: "list.bullet")
                            .frame(width: 20, height: 20)
                    }
                    .buttonStyle(.borderless)
                    .help("Toggle Left Panel")

                    Text(titleText)
                        .foregroundColor(.white)
                        .multilineTextAlignment(.center)
                        .frame(maxWidth: .infinity)
                        .contentShape(Rectangle())
                        .onTapGesture {
                            if !isEditing {
                                isEditing = true
                                inputText = ""
                                DispatchQueue.main.async {
                                    isFirstResponder = true
                                }
                            }
                        }

                    Button(action: refreshAction) {
                        Image(systemName: "arrow.clockwise")
                    }
                    .buttonStyle(.borderless)
                    .help("Refresh")
                }
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 4)
        .frame(maxWidth: .infinity, minHeight: 28)
        .overlay(
            Capsule()
                .stroke(
                    Color(red: 0.18, green: 0.56, blue: 0.96)
                        .opacity(focusRingOpacity),
                    lineWidth: 2.8
                )
                .scaleEffect(focusRingScale)
                .padding(.vertical, -4)
        )
        .onChange(of: isFirstResponder) {
            if !isFirstResponder && isEditing {
                isEditing = false
            }
        }
        .onChange(of: isEditing) {
            if isEditing {
                // Shrink from a slightly larger, transparent blue circle to normal size and growing.
                focusRingScale = 1.08
                focusRingOpacity = 0.0
                withAnimation(.spring(response: 0.25, dampingFraction: 0.7, blendDuration: 0.1)) {
                    focusRingScale = 1.0
                    focusRingOpacity = 1.0
                }
            } else {
                withAnimation(.easeOut(duration: 0.15)) {
                    focusRingOpacity = 0.0
                }
            }
        }
    }

    private func endEditing() {
        isEditing = false
        isFirstResponder = false
    }
}

// MARK: - Focusable TextField

#if os(macOS)
private struct FocusableTextField: NSViewRepresentable {
    @Binding var text: String
    var placeholder: String
    @Binding var isFirstResponder: Bool

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField()
        field.isBordered = false
        field.isBezeled = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = NSFont.systemFont(ofSize: NSFont.systemFontSize)
        field.placeholderString = placeholder
        field.delegate = context.coordinator
        field.usesSingleLineMode = true
        field.maximumNumberOfLines = 1
        field.cell?.wraps = false
        field.cell?.isScrollable = true
        return field
    }

    func updateNSView(_ nsView: NSTextField, context: Context) {
        if nsView.stringValue != text {
            nsView.stringValue = text
        }
        nsView.placeholderString = placeholder

        if isFirstResponder,
           let window = nsView.window,
           window.firstResponder != nsView.currentEditor() {
            window.makeFirstResponder(nsView)
        }
    }

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: FocusableTextField

        init(_ parent: FocusableTextField) {
            self.parent = parent
        }

        func controlTextDidChange(_ obj: Notification) {
            guard let field = obj.object as? NSTextField else { return }
            // Filter out line break and keep as single line content
            let raw = field.stringValue
            let cleaned = raw
                .components(separatedBy: .newlines)
                .joined(separator: " ")
            if cleaned != raw {
                field.stringValue = cleaned
            }
            parent.text = cleaned
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            parent.isFirstResponder = false
        }
    }
}
#elseif os(iOS)
private struct FocusableTextField: UIViewRepresentable {
    @Binding var text: String
    var placeholder: String
    @Binding var isFirstResponder: Bool

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeUIView(context: Context) -> UITextField {
        let field = UITextField()
        field.borderStyle = .none
        field.backgroundColor = .clear
        field.font = UIFont.systemFont(ofSize: UIFont.systemFontSize)
        field.placeholder = placeholder
        field.delegate = context.coordinator
        field.returnKeyType = .search
        field.autocorrectionType = .no
        field.autocapitalizationType = .none
        return field
    }

    func updateUIView(_ uiView: UITextField, context: Context) {
        if uiView.text != text {
            uiView.text = text
        }
        uiView.placeholder = placeholder

        if isFirstResponder && !uiView.isFirstResponder {
            uiView.becomeFirstResponder()
        } else if !isFirstResponder && uiView.isFirstResponder {
            uiView.resignFirstResponder()
        }
    }

    final class Coordinator: NSObject, UITextFieldDelegate {
        var parent: FocusableTextField

        init(_ parent: FocusableTextField) {
            self.parent = parent
        }

        func textField(_ textField: UITextField, shouldChangeCharactersIn range: NSRange, replacementString string: String) -> Bool {
            // Filter Break
            if string.contains("\n") {
                textField.resignFirstResponder()
                return false
            }
            return true
        }

        func textFieldDidChangeSelection(_ textField: UITextField) {
            parent.text = textField.text ?? ""
        }

        func textFieldDidEndEditing(_ textField: UITextField) {
            parent.isFirstResponder = false
        }
    }
}
#endif

// MARK: - Tab Card View

private struct TabCardView: View {
    let tab: Tab
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    let titleBarRatio: CGFloat  // Accurate proportion of the height of the title bar
    let toolbarHeight: CGFloat  // toolbar Actual height, used as a scaling baseline
    let currentScale: CGFloat
    let isFocused: Bool
    let isHovering: Bool
    let isSwapping: Bool  // Exchangeing (for faded animation)
    // Add: for creation SingleTabRootView
    @ObservedObject var viewModel: BrowserViewModel
    var historyVM: HistoryViewModel?
    let viewportSize: CGSize  // Visual dimensions for RN Render
    let onSelect: () -> Void
    let onClose: () -> Void

    // Bottom service height (fixed value,Overview Shows in the bottom of the card in mode)
    private let floatingTitleBarHeight: CGFloat = 56
    private let iconWidth: CGFloat = 32

    @ViewBuilder
    var body: some View {
        // Use ZStack Allow service lines at the bottom of the card
        let base = ZStack(alignment: .bottom) {
            // Card Subject (Purple Contents) + (Bed line)
            // Round corner only Overview The pattern shows thatFocus Mode does not round corner
            VStack(spacing: 0) {
                // Content Preview Area
                contentView
                    .frame(width: cardWidth, height: cardHeight - floatingTitleBarHeight)

                // Bottom service line - Only Overview Mode DisplayFocus Mode Hidden)
                if !isFocused {
                    floatingTitleBarView
                        .frame(width: cardWidth, height: floatingTitleBarHeight)
                }
            }
            .frame(width: cardWidth, height: cardHeight)
            .cornerRadius(isFocused ? 0 : 22)
            .shadow(color: isFocused ? .clear : Color.black.opacity(0.1), radius: 8, x: 0, y: 2)
        }
        // hover Time and transparency, and the exchange is completely non-transparent.
        .opacity({
            if isSwapping {
                return 1.0
            } else if isHovering && !isFocused {
                return 0.5
            } else {
                return 1.0
            }
        }())

        base
    }

    // Bottom service line view (reference) PhoneTabListView (in layout)
    private var floatingTitleBarView: some View {
        HStack(alignment: .center, spacing: 12) {
            // Apply Icon
            ZStack(alignment: .topTrailing) {
                Circle()
                    .fill(Color(hslString: tab.backgroundColor))
                    .frame(width: iconWidth, height: iconWidth)
                    .overlay {
                        if let icon = tab.icon, let url = URL(string: icon) {
                            KFImage(url)
                                .resizable()
                                .scaledToFit()
                                .frame(width: iconWidth, height: iconWidth)
                                .cornerRadius(iconWidth/2)
                        } else {
                            Image(systemName: "plus.viewfinder")
                                .font(.system(size: 14))
                                .foregroundColor(.white.opacity(0.9))
                        }
                    }

                // Unread message small red dot
                if tab.hasUnreadMessage {
                    Circle()
                        .fill(Color.red)
                        .frame(width: 10, height: 10)
                        .overlay(
                            Circle()
                                .stroke(Color(.systemBackground), lineWidth: 2)
                        )
                        .offset(x: 2, y: -2)
                }
            }
            .frame(width: iconWidth, height: iconWidth)

            // Title
            Text(tab.title)
                .font(.system(size: 16, weight: .semibold))
                .foregroundColor(Color(hex: "#404040"))
                .lineLimit(1)

            Spacer()

            // Collection Star (titled "Step-In-Sight")
            if tab.title != TM.active.blackTab() {
                Button(action: {
                    // Switch collection status and send notification to RN
                    let newBookmarkStatus = !tab.isBookmarked

                    // Update local status immediately (UI Response
                    if let index = viewModel.tabs.firstIndex(where: { $0.id == tab.id }) {
                        viewModel.tabs[index].isBookmarked = newBookmarkStatus
                        viewModel.scheduleSaveTabs() // Enduring change
                    }

                    // Send Notification to RN Side-processing actual collection logic
                    NotificationCenter.default.post(
                        name: .tabBookmarkChangedFromNative,
                        object: nil,
                        userInfo: tab.runtimeBookmarkChangedUserInfo(isBookmarked: newBookmarkStatus)
                    )
                }) {
                    Image(systemName: "star.circle.fill")
                        .font(.system(size: 20))
                        .foregroundColor(tab.isBookmarked ? Color(hex: "#EAC15B") : Color(hex: "#9E9EA7"))
                        .frame(width: 36, height: 36)
                        .background(Circle().fill(Color.clear))
                }
            }

            // Close button
            Button(action: onClose) {
                Image(systemName: "xmark.circle.fill")
                    .font(.system(size: 20))
                    .foregroundColor(Color(hex: "#9E9EA7"))
                    .frame(width: 36, height: 36)
            }
        }
        .frame(height: floatingTitleBarHeight)
        .padding(.horizontal, 14)
        .background(Color(hex: "#E5E5EA"))
    }

    // Size of card content area (now equal to full card size because the title bar is hanging outside)
    private var cardContentSize: CGSize {
        CGSize(width: cardWidth, height: cardHeight)
    }

    // RN Render size (using visual size, width ratio and cardContentSize Match)
    private var rnRenderSize: CGSize {
        CGSize(width: viewportSize.width, height: viewportSize.height)
    }

    // RN View needs scaling
    // RN Always press viewportSize Render (still high) and zoom in cardContentSize
    // Yes. Focus Mode, outer layer surface scale It'll zoom back in. viewportSize
    // Final effect:RN Content 1:1 Scale
    private var rnScaleFactor: CGFloat {
        guard currentScale > 0 else { return 1 }
        let baseScale = cardContentSize.width / max(viewportSize.width, 1)
        return baseScale
    }

    // Content Area View
    @ViewBuilder
    private var contentView: some View {
        let content = ZStack {
            // Bottom Background - Use Tab Random background colour
            Color(hslString: tab.backgroundColor)
            
            // Every one. Tab There's an independent one. RCTRootView - Realize the scaling
            // Critical repairs: use overlay Not frame+scaleEffect Group
            // The old way. scaleEffect Later. frame It's gonna be a problem with alignment, because... SwiftUI Calculate location using original size
            // New: with a fixed-sized container,overlay Place after scaling content
            SingleTabRootView(
                tab: tab,
                viewModel: viewModel,
                historyVM: historyVM,
                containerSize: CGSize(
                    width: max(rnRenderSize.width, 1),
                    height: max(rnRenderSize.height, 1)
                )
            )
            .frame(
                width: max(rnRenderSize.width, 1),
                height: max(rnRenderSize.height, 1)
            )
            .scaleEffect(rnScaleFactor, anchor: .center)
        }
            .frame(maxWidth: cardContentSize.width, maxHeight: cardContentSize.height)
            .contentShape(Rectangle())
            .clipped()

        if isFocused {
            content
        } else {
            content.onTapGesture {
                onSelect()
            }
        }
    }
}

// MARK: - Tab Drag Preview View

private struct TabDragPreviewView: View {
    let tab: Tab
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    private let floatingTitleBarHeight: CGFloat = 56
    private let iconWidth: CGFloat = 32

    var body: some View {
        VStack(spacing: 0) {
            ZStack {
                Color(hslString: tab.backgroundColor)
                Image(systemName: "globe")
                    .font(.system(size: 32))
                    .foregroundColor(.white.opacity(0.3))
            }
            .frame(width: cardWidth, height: cardHeight - floatingTitleBarHeight)

            HStack(alignment: .center, spacing: 12) {
                // Apply Icon
                ZStack(alignment: .topTrailing) {
                    Circle()
                        .fill(Color(hslString: tab.backgroundColor))
                        .frame(width: iconWidth, height: iconWidth)
                        .overlay {
                            Image(systemName: "plus.viewfinder")
                                .font(.system(size: 14))
                                .foregroundColor(.white.opacity(0.9))
                        }
                }
                .frame(width: iconWidth, height: iconWidth)

                // Title
                Text(tab.title)
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundColor(Color(hex: "#404040"))
                    .lineLimit(1)

                Spacer()
            }
            .frame(height: floatingTitleBarHeight)
            .padding(.horizontal, 14)
            .background(Color(hex: "#E5E5EA"))
        }
        .frame(width: cardWidth, height: cardHeight)
        .cornerRadius(22)
        .shadow(color: Color.black.opacity(0.5), radius: 10, x: 0, y: 5)
    }
}

// MARK: - Close Button View

private struct CloseButtonView: View {
    let iconSize: CGFloat
    let containerWidth: CGFloat
    let onClose: () -> Void

    @State private var isHovering = false

    var body: some View {
        Button(action: {
            onClose()
        }) {
            Image(systemName: "xmark")
                .font(.system(size: iconSize, weight: .semibold))
                .foregroundColor(isHovering ? .white : .white.opacity(0.6))
                .frame(width: iconSize * 1.6, height: iconSize * 1.6)
                .background(
                    Circle()
                        .fill(isHovering ? Color.white.opacity(0.15) : Color.clear)
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .frame(width: containerWidth)
        .onHover { hovering in
            isHovering = hovering
        }
    }
}

// MARK: - New Tab Card View

private struct NewTabCardView: View {
    let cardWidth: CGFloat
    let cardHeight: CGFloat
    var onTap: (() -> Void)? = nil

    var body: some View {
        ZStack {
            #if os(macOS)
            Rectangle()
                .fill(Color(nsColor: NSColor(white: 0.15, alpha: 1.0)))
            #elseif os(iOS)
            Rectangle()
                .fill(Color(uiColor: UIColor(white: 0.15, alpha: 1.0)))
            #endif

            Image(systemName: "plus")
                .font(.system(size: 48))
                .foregroundColor(.white.opacity(0.5))
        }
        .frame(width: cardWidth, height: cardHeight)
        .cornerRadius(10)
        .overlay(
            RoundedRectangle(cornerRadius: 10)
                .stroke(Color.white.opacity(0.15), lineWidth: 1)
        )
        .shadow(color: Color.black.opacity(0.3), radius: 5, x: 0, y: 2)
        .contentShape(Rectangle())
        .onTapGesture {
            onTap?()
        }
    }
}

#Preview {
    UnifiedActriumView(viewModel: BrowserViewModel(), historyVM: HistoryViewModel())
}
