import Foundation
import UIKit
import Combine
import ZIPFoundation

private let logger = AppLogger.viewModel

private enum BrowserTabNotificationContract {
    static let tabId = RuntimeBridgeContract.tabId
    static let title = RuntimeBridgeContract.title
    static let icon = RuntimeBridgeContract.icon
    static let appUrl = RuntimeBridgeContract.appUrl
    static let systemTabType = "systemTabType"
    static let previousTabId = "previousTabId"
}

/// Actro display mode - stored in ViewModel to survive view recreation
enum ActroMode: Equatable {
    case overview
    case focus(tabId: UUID)

    var isInFocusMode: Bool {
        if case .focus = self { return true }
        return false
    }
}

@MainActor
class BrowserViewModel: ObservableObject {
    static weak var active: BrowserViewModel?

    @Published var tabs: [Tab] = [] {
        didSet {
            scheduleSaveTabs()
        }
    }
    @Published var currentTabId: UUID?
    // Default that all tabs are left-side panel visible; overrides are recorded only when user switching
    @Published private var leftPanelVisibility: [UUID: Bool] = [:]

    // MARK: - Persistence
    private let tabsKey = "persistedTabs"
    private let browserDataRepository = BrowserDataRepository.shared
    private var cancellables = Set<AnyCancellable>()

    /// Persistence of shaking: when a high frequency of a change in attributes (title, red dot, collection, etc.) is triggered, combined with delayed saving
    private var saveTabsWorkItem: DispatchWorkItem?

    /// Current fullscreen tab ID (for iPhone only, excluding system tabs)
    @Published var fullscreenTabId: UUID?

    /// Mark next time. onDisappear Whether to skip the screenshot (if any)RN Pre-screened when exiting)
    var skipNextDisappearSnapshot = false

    // MARK: - System Tabs (It's a separate management, not a blend. tabs Array)
    @Published var systemTabs: [SystemTabType: Tab] = [:]

    // MARK: - System Tab Management
    @Published var currentSystemTabType: SystemTabType? = nil
    /// Record system label type before opening global search, resume when closing search
    var previousSystemTabType: SystemTabType? = nil

    // MARK: - Tab Groups
    @Published var tabGroups: [TabGroup] = []

    /// Actro display mode (Overview vs Focus)
    /// Stored here to survive view recreation when DevTools layout changes
    @Published var actroMode: ActroMode = .overview

    // MARK: - Grid Slot Position Management

    /// Tab ID → grid slot index mapping
    @Published var tabSlotPositions: [UUID: Int] = [:]

    /// Tab snapshots for overview mode
    @Published var tabSnapshots: [UUID: UIImage] = [:]

    /// Toolbar Actual height (from view measurements)
    @Published var toolbarHeight: CGFloat = 52 // Default value, updated in view

    // MARK: - Multiple Window mode (iPadOS)
    @Published var multipleWindowMode: Bool = false

    var currentTab: Tab? {
        guard let currentTabId = currentTabId else { return nil }
        return tabs.first(where: { $0.id == currentTabId })
    }

    init() {
        BrowserViewModel.active = self

        // First, try to load persisted tabs
        if loadPersistedTabs() {
            initializeSlotPositions()
            logger.notice("📦 [BrowserViewModel] Loaded \(self.tabs.count) A Protracted Tab")
            persistBrowserPageData()
            return
        }

        if restoreSharedBrowserSessionIfNeeded() {
            initializeSlotPositions()
            return
        }

        let defaultTab = Tab(
            title: TM.active.blackTab(),
            htmlContent: """
                <!DOCTYPE html>
                <html>
                ...
                </html>
                """,
            // 🟢 Trick: Give an empty script, force SwiftUI Think it's a... RN Tab
            // Even if it's like this. content Yes. HTMLIt'll be wrapped outside. RuntimeApp It's... RN Shells (Address Bar, etc.)
            unifiedScript: "/* welcome page shell */"
        )
        tabs.append(defaultTab)
        currentTabId = defaultTab.id

        // Initialize slot positions
        initializeSlotPositions()
    }

    private func persistBrowserPageData() {
        browserDataRepository.saveSession(tabs: tabs, currentTabId: currentTabId)
        browserDataRepository.saveBookmarkedTabs(tabs)
    }

    // MARK: - Tab Persistence

    /// Get the URL for the images directory
    private var imagesDirectoryURL: URL {
        let urls = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)
        let documentsURL = urls[0]
        let imagesURL = documentsURL.appendingPathComponent("images", isDirectory: true)

        // Create directory if it doesn't exist
        if !FileManager.default.fileExists(atPath: imagesURL.path) {
            do {
                try FileManager.default.createDirectory(at: imagesURL, withIntermediateDirectories: true)
            } catch {
                logger.error("❌ [BrowserViewModel] Could not create images Contents: \(error)")
            }
        }
        return imagesURL
    }

    /// Get the file URL for a tab snapshot
    private func snapshotURL(for tabId: UUID) -> URL {
        return imagesDirectoryURL.appendingPathComponent("\(tabId.uuidString).jpg")
    }

    /// Save a tab snapshot to disk — Thumbnails use lower compression quality to reduce disk occupancy
    private func saveSnapshot(_ image: UIImage, for tabId: UUID) {
        let fileURL = snapshotURL(for: tabId)
        // Thumbnail size is small,0.5 The compression quality is not significantly different in visual terms, but smaller in size.
        if let jpegData = image.jpegData(compressionQuality: 0.5) {
            do {
                try jpegData.write(to: fileURL)
                logger.debug("💾 [BrowserViewModel] Saved amplitude(Samples down): \(tabId.uuidString.prefix(8)), Size: \(jpegData.count) bytes")
            } catch {
                logger.error("❌ [BrowserViewModel] Failed to save the screenshot: \(error)")
            }
        }
    }

    /// Load a tab snapshot from disk — Drop samples after loading to ensure that memory copies remain thumbnails
    private func loadSnapshot(for tabId: UUID) -> UIImage? {
        let fileURL = snapshotURL(for: tabId)
        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            return nil
        }
        do {
            let data = try Data(contentsOf: fileURL)
            if let image = UIImage(data: data) {
                // The disk may save an old full-resolution intercept and then return after the sample is down
                // Keep the width-to-square samples and avoid stretching
                return image.downsampled(toWidth: Self.snapshotTargetWidth)
            }
            return nil
        } catch {
            logger.error("❌ [BrowserViewModel] Loading the screenshot failed: \(error)")
            return nil
        }
    }

    /// Loading snapshots on demand: Read from disk when no snapshots are in memory.
    /// Memory Warning Recovery tabSnapshots , Card View onAppear Call this method to restore the thumbnail.
    func loadSnapshotIfNeeded(for tabId: UUID) {
        guard tabSnapshots[tabId] == nil else { return }
        if let snapshot = loadSnapshot(for: tabId) {
            tabSnapshots[tabId] = snapshot
        }
    }

    /// Delete a tab snapshot from disk
    private func deleteSnapshot(for tabId: UUID) {
        let fileURL = snapshotURL(for: tabId)
        if FileManager.default.fileExists(atPath: fileURL.path) {
            do {
                try FileManager.default.removeItem(at: fileURL)
                logger.debug("🗑️ [BrowserViewModel] Deleted: \(tabId.uuidString.prefix(8))")
            } catch {
                logger.error("❌ [BrowserViewModel] Failed to delete screenshot: \(error)")
            }
        }
    }

    /// Load persisted tabs from UserDefaults and create full Tab objects
    /// - Returns: true if tabs were loaded, false if no persisted data or error
    private func loadPersistedTabs() -> Bool {
        guard let data = UserDefaults.standard.data(forKey: tabsKey) else {
            logger.debug("📭 [BrowserViewModel] No persistent tab data found")
            return false
        }

        do {
            let persistedTabs = try JSONDecoder().decode([PersistedTab].self, from: data)
            guard !persistedTabs.isEmpty else {
                logger.debug("📭 [BrowserViewModel] Enduring Tab Data is empty")
                return false
            }

            // Convert PersistedTab to full Tab objects
            self.tabs = persistedTabs.map { persistedTab in
                Tab(
                    id: persistedTab.id,
                    title: persistedTab.title,
                    icon: persistedTab.icon,
                    url: persistedTab.appUrl,
                    appId: persistedTab.appId,
                    hasUnreadMessage: persistedTab.hasUnreadMessage,
                    isBookmarked: persistedTab.isBookmarked,
                    status: persistedTab.status
                )
            }

            // Load snapshots for all tabs
            for tab in tabs {
                if let snapshot = loadSnapshot(for: tab.id) {
                    tabSnapshots[tab.id] = snapshot
                }
            }

            // Set current tab to first tab if available
            if let firstTab = self.tabs.first {
                self.currentTabId = firstTab.id
            }

            logger.notice("✅ [BrowserViewModel] Load successfully \(self.tabs.count) A Protracted Tab")
            return true
        } catch {
            logger.error("❌ [BrowserViewModel] Failed to load the Protracted Tab: \(error)")
            return false
        }
    }

    private func restoreSharedBrowserSessionIfNeeded() -> Bool {
        guard let restoredSession = browserDataRepository.loadSessionTabs() else {
            logger.debug("📭 [BrowserViewModel] There's nothing to recover. shared browser session")
            return false
        }

        tabs = restoredSession.tabs
        currentTabId = restoredSession.currentTabId ?? restoredSession.tabs.first?.id
        saveTabsWorkItem?.cancel()
        saveTabsWorkItem = nil
        savePersistedTabsOnly()

        logger.notice("✅ [BrowserViewModel] From shared browser session Restore \(self.tabs.count) Tab")
        return true
    }

    /// New Tab to Add List (for"Pop up first sheet, and then show in the list"(interactive)
    var pendingNewTab: Tab?

    @discardableResult
    func addNewTab(focus: Bool = true, deferAppend: Bool = false) -> Tab {
        logger.debug("🆕 Start New Tab...")

        // Create Default RN Tab (same as when application started)
        let newTab = Tab(
            title: TM.active.blackTab(),
            htmlContent: """
                <!DOCTYPE html>
                <html>
                ...
                </html>
                """,
            // 🟢 Trick: Give an empty script, force SwiftUI Think it's a... RN Tab
            // Even if it's like this. content Yes. HTMLIt'll be wrapped outside. RuntimeApp It's... RN Shells (Address Bar, etc.)
            unifiedScript: "/* welcome page shell */"
        )

        if deferAppend {
            // Delayed addition list: only record first pendingWait. sheet After popup append
            pendingNewTab = newTab
            currentTabId = newTab.id
        } else {
            tabs.append(newTab)
            tabSlotPositions[newTab.id] = tabs.count - 1
            currentTabId = newTab.id
        }

        // It's a caller's intention to decide whether to enter. Focus Mode (some) UI The scene is still in the air. Overview)
        if focus {
            actroMode = .focus(tabId: newTab.id)
        }

        logger.notice(" New Tab Created: \(newTab.title), Location: \(deferAppend ? "pending" : "\(self.tabs.count - 1)")")

        NotificationCenter.default.post(
            name: .tabOpenedFromNative,
            object: nil,
            userInfo: tabOpenedUserInfo(for: newTab)
        )

        if !deferAppend {
            saveTabs()
        }
        return newTab
    }

    /// Will pending Add a new tab to the list
    func commitPendingNewTab() {
        guard let pending = pendingNewTab else { return }
        tabs.append(pending)
        tabSlotPositions[pending.id] = tabs.count - 1
        pendingNewTab = nil
        saveTabs()
    }

    /// Save tab snapshot when set — Deposition of samples stored and significantly reduced memory occupancy
    func setTabSnapshot(_ image: UIImage, for tabId: UUID) {
        // The card preview requires only small thumbnails, sampled by width, and maintains the original width ratio
        let thumbnail = image.downsampled(toWidth: Self.snapshotTargetWidth)
        tabSnapshots[tabId] = thumbnail
        // Copy of disk also saves the down-sampling version, and reduces I/O Volume
        saveSnapshot(thumbnail, for: tabId)
    }

    /// Scattered-down target width (@1x pt)
    /// iPhone Cards wide 344pt, iPad The card is wider.
    /// 400pt @1x Clear enough on two platforms, single memory protocol 50KB.
    static let snapshotTargetWidth: CGFloat = 400

    /// Open or get a system tab (History, Bookmarks, Settings)
    /// System tabs are managed separately from regular tabs
    /// Returns the system tab (creates if doesn't exist)
    @discardableResult
    func openSystemTab(
        _ type: SystemTabType,
        additionalProps: [String: Any]? = nil,
        replaceAdditionalProps: Bool = false
    ) -> Tab {
        // Comment in Chinese:login Yes. iPad End needs to be included in normal tabs The system,
        // Existing TabBar / currentTab / Focus Modes are all around tabs Work.
        // If you keep it on systemTabs, the login signer is created but the host is unable to focus and present correctly.
        if type == .login {
            if let existingTabIndex = tabs.firstIndex(where: { $0.systemTabType == type }) {
                var existingTab = tabs[existingTabIndex]
                if replaceAdditionalProps {
                    existingTab.additionalProps = additionalProps
                    existingTab.reloadNonce &+= 1
                    tabs[existingTabIndex] = existingTab
                } else if let additionalProps = additionalProps {
                    existingTab.additionalProps = additionalProps
                    existingTab.reloadNonce &+= 1
                    tabs[existingTabIndex] = existingTab
                }

                // Comment in Chinese: Unified switchToTab, replace the host's existing page sign to switch the notification and status sync link.
                switchToTab(id: existingTab.id)
                logger.debug("🔄 Return existing login tags: \(type.title)")
                return existingTab
            }

            let newTab = Tab(
                id: UUID(),
                title: type.title,
                systemTabType: type,
                backgroundColor: "#1e1e1e",
                additionalProps: additionalProps
            )
            tabs.append(newTab)
            tabSlotPositions[newTab.id] = tabs.count - 1
            logger.notice(" Created Login Tab: \(type.title)")

            NotificationCenter.default.post(
                name: .tabOpenedFromNative,
                object: nil,
                userInfo: tabOpenedUserInfo(for: newTab, systemTabType: type)
            )
            // Comment: Switch as soon as the new one is finished, guaranteed iPad End Selected,Focus Modes and RN The side events are consistent.
            switchToTab(id: newTab.id)
            return newTab
        }

        // Check if system tab already exists
        if var existingTab = systemTabs[type] {
            if replaceAdditionalProps {
                existingTab.additionalProps = additionalProps
                existingTab.reloadNonce &+= 1
                systemTabs[type] = existingTab
            } else if let additionalProps = additionalProps {
                existingTab.additionalProps = additionalProps
                // Pass. reloadNonce Driver RN remount, ensure that deep chain parameters are effective
                existingTab.reloadNonce &+= 1
                systemTabs[type] = existingTab
            }
            logger.debug("🔄 Return existing system tags: \(type.title)")
            return existingTab
        }

        // Create new system tab
        let newTab = Tab(
            id: UUID(),
            title: type.title,
            systemTabType: type,
            backgroundColor: "#1e1e1e",  // Dark background for system tabs
            additionalProps: additionalProps
        )
        systemTabs[type] = newTab
        logger.notice(" Created System Label: \(type.title)")

        // Send Notification To RN
        NotificationCenter.default.post(
            name: .tabOpenedFromNative,
            object: nil,
            userInfo: tabOpenedUserInfo(for: newTab, systemTabType: type)
        )

        return newTab
    }

    /// Reload system tab by incrementing reloadNonce
    func reloadSystemTab(_ type: SystemTabType) {
        guard var tab = systemTabs[type] else {
            logger.warning("⚠️ [reloadSystemTab] System tag does not exist: \(type.title)")
            return
        }

        logger.notice("🔄 [reloadSystemTab] Refresh System Label: \(type.title), reloadNonce: \(tab.reloadNonce) -> \(tab.reloadNonce + 1)")
        tab.reloadNonce &+= 1
        systemTabs[type] = tab
        objectWillChange.send()
    }

    private func tabIdUserInfo(_ tabId: UUID) -> [String: Any] {
        [BrowserTabNotificationContract.tabId: tabId.uuidString]
    }

    private func tabOpenedUserInfo(for tab: Tab, systemTabType: SystemTabType? = nil) -> [String: Any] {
        var userInfo: [String: Any] = [
            BrowserTabNotificationContract.tabId: tab.id.uuidString,
            BrowserTabNotificationContract.title: tab.title
        ]
        if let systemTabType {
            userInfo[BrowserTabNotificationContract.systemTabType] = systemTabType.rawValue
        }
        if let icon = tab.icon {
            userInfo[BrowserTabNotificationContract.icon] = icon
        }
        if let appUrl = tab.url?.absoluteString {
            userInfo[BrowserTabNotificationContract.appUrl] = appUrl
        }
        return userInfo
    }

    private func tabSwitchedUserInfo(tabId: UUID, previousTabId: UUID?) -> [String: Any] {
        var userInfo = tabIdUserInfo(tabId)
        if let previousTabId {
            userInfo[BrowserTabNotificationContract.previousTabId] = previousTabId.uuidString
        }
        return userInfo
    }

    private func tabUpdatedUserInfo(
        tabId: UUID,
        title: String?,
        icon: String?,
        appUrl: URL?
    ) -> [String: Any] {
        var userInfo = tabIdUserInfo(tabId)
        if let title {
            userInfo[BrowserTabNotificationContract.title] = title
        }
        if let icon {
            userInfo[BrowserTabNotificationContract.icon] = icon
        }
        if let appUrl = appUrl?.absoluteString {
            userInfo[BrowserTabNotificationContract.appUrl] = appUrl
        }
        return userInfo
    }

    func switchToTab(id: UUID) {
        let previousTabId = currentTabId
        let targetTab = tabs.first(where: { $0.id == id })

        if let tab = targetTab {
            logger.debug("🔄 Switch to Tab: \(tab.title)")
        }
        currentTabId = id
        // Ensure Actro is in focus mode when switching tabs
        actroMode = .focus(tabId: id)

        // Send Notification To RN
        NotificationCenter.default.post(
            name: .tabSwitchedFromNative,
            object: nil,
            userInfo: tabSwitchedUserInfo(tabId: id, previousTabId: previousTabId)
        )
        persistBrowserPageData()
    }

    func closeTab(id: UUID) {
        // Cleans up caches View
        ActroReactNativeHost.shared.removeViewForTab(tabId: id)

        // Clean up the snapshot.
        tabSnapshots.removeValue(forKey: id)
        deleteSnapshot(for: id)

        // Get the slot position of the tab being closed
        guard let closedPosition = tabSlotPositions[id] else {
            logger.warning(" Cannot find tab slot position")
            return
        }

        // If this is the last tab
        if tabs.count == 1 {
            logger.debug("📱 Close Last Tab")
            currentTabId = nil
        }

        // Switch to the adjacent label if the current tab is closed
        if currentTabId == id {
            // Cellular side does not switch to adjacent label
            if UIDevice.current.userInterfaceIdiom == .phone {
                currentTabId = nil
            } else if let index = tabs.firstIndex(where: { $0.id == id }) {
                var nextTabId: UUID? = nil
                if index > 0 {
                    nextTabId = tabs[index - 1].id
                } else if index < tabs.count - 1 {
                    nextTabId = tabs[index + 1].id
                }

                if let nextId = nextTabId {
                    // If you're in Focus mode, close the current tab Auto-to-neighborship after tab It's... Focus Mode
                    if actroMode.isInFocusMode {
                        switchToTab(id: nextId)
                    } else {
                        currentTabId = nextId
                    }
                }
            }
        }

        // Remove Tab
        tabs.removeAll(where: { $0.id == id })
        saveTabs()

        // Remove the location map of this tab
        tabSlotPositions.removeValue(forKey: id)

        // Reorganization position: reduce all positions larger than the deleted position1
        for tabId in tabs.map({ $0.id }) {
            if let position = tabSlotPositions[tabId], position > closedPosition {
                tabSlotPositions[tabId] = position - 1
            }
        }

        logger.debug("🗑️ Close Tab, Location \(closedPosition)Remaining \(self.tabs.count) Tab")

        // Send Notification To RN
        NotificationCenter.default.post(
            name: .tabClosedFromNative,
            object: nil,
            userInfo: tabIdUserInfo(id)
        )

        if tabs.isEmpty, UIDevice.current.userInterfaceIdiom != .phone {
            logger.debug("🚪 Close last tab, switch to OverView Mode")
            actroMode =  .overview
        }
    }

    func reorderTab(fromIndex source: Int, toIndex destination: Int) {
        guard source >= 0, source < tabs.count,
              destination >= 0, destination <= tabs.count,
              source != destination else { return }

        let movedTab = tabs.remove(at: source)
        let insertIndex = destination > source ? destination - 1 : destination
        tabs.insert(movedTab, at: insertIndex)

        logger.debug("🔄 Tab Reorder: From Index \(source) Move To \(insertIndex)")

        // Send Notification To RN
        let orderedTabIds = tabs.map { $0.id.uuidString }
        NotificationCenter.default.post(
            name: .tabsReorderedFromNative,
            object: nil,
            userInfo: ["tabIds": orderedTabIds]
        )

        saveTabs()
    }

    /// Save tabs to UserDefaults (only essential fields) — Save Now
    /// For structural change only (additions/deletions) Tab, reorder, etc.) , change properties please use scheduleSaveTabs()
    func saveTabs() {
        // Cancel the shivering tasks to be performed and avoid duplication of preservation
        saveTabsWorkItem?.cancel()
        saveTabsWorkItem = nil

        savePersistedTabsOnly()
        persistBrowserPageData()
    }

    private func savePersistedTabsOnly() {
        let persistedTabs = tabs.map { PersistedTab(from: $0) }
        do {
            let data = try JSONEncoder().encode(persistedTabs)
            UserDefaults.standard.set(data, forKey: tabsKey)

            logger.debug("💾 [BrowserViewModel] Saved \(persistedTabs.count) Tab")
        } catch {
            logger.error("❌ [BrowserViewModel] Failed to save tab: \(error)")
        }
    }

    /// Divelproof saving:2 Multiple calls in seconds are only last time.
    /// Apply to HF properties changes (titles, red dots, collections, etc.) to avoid each change triggering disk I/O.
    func scheduleSaveTabs() {
        saveTabsWorkItem?.cancel()
        let workItem = DispatchWorkItem { [weak self] in
            self?.saveTabs()
        }
        saveTabsWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.0, execute: workItem)
    }

    // MARK: - Unread Status

    /// Clear Assigned tab Unread Red Point Status
    func clearUnread(for tabId: UUID) {
        guard let index = tabs.firstIndex(where: { $0.id == tabId }),
              tabs[index].hasUnreadMessage else { return }
        tabs[index].hasUnreadMessage = false
        scheduleSaveTabs()
    }

    // MARK: - Memory Pressure Handling

    /// Automatically recover resources when warning is stored in the system:
    /// 1. Release all inactive Tab It's... RootView(Maximity memory consumption, per approximately 40MB)
    /// 2. Release a copy of the snapshot memory (a copy of the disk is retained, the label list is loaded from the disk as necessary when rolling)
    func handleMemoryWarning() {
        let activeTabId = currentTabId
        var releasedCount = 0

        // 1. Release all inactive Tab It's... RootView
        for tab in tabs where tab.id != activeTabId {
            ActroReactNativeHost.shared.removeViewForTab(tabId: tab.id)
            releasedCount += 1
        }

        // 2. Release a copy of the snapshot memory (reduced disk copy, loaded as required)
        let snapshotCount = tabSnapshots.count
        tabSnapshots.removeAll()

        // Immediate sustainability to ensure data security
        saveTabs()

        logger.warning("⚠️ [BrowserViewModel] Memory Warning Recovery: Release \(releasedCount) A backstage. RootViewClear \(snapshotCount) Memory Snapshot")
    }

    // MARK: - Tab Group Management

    /// Create a new tab group
    @discardableResult
    func createGroup(name: String = "", color: TabGroupColor = .gray) -> TabGroup {
        let group = TabGroup(name: name, color: color)
        tabGroups.append(group)
        logger.debug("📁 Create New Group: \(group.id), Name: \(name.isEmpty ? "(No name)" : name)")
        return group
    }

    /// Move a tab to a specific index in the tabs array
    /// - Parameters:
    ///   - tabId: The tab to move
    ///   - toIndex: The target index (will be clamped to valid range)
    func moveTab(tabId: UUID, toIndex: Int) {
        guard let fromIndex = tabs.firstIndex(where: { $0.id == tabId }) else { return }
        guard fromIndex != toIndex else { return }

        let tab = tabs.remove(at: fromIndex)
        let targetIndex = min(max(0, toIndex), tabs.count)
        tabs.insert(tab, at: targetIndex)
        logger.debug("📍 Tab Move: \(tab.title) From Location \(fromIndex) Present. \(targetIndex)")
    }

    /// Move a tab to a specific position and optionally change its group
    /// - Parameters:
    ///   - tabId: The tab to move
    ///   - toIndex: Target index in the tabs array
    ///   - groupId: New group ID (nil to ungroup)
    func moveTabToPosition(tabId: UUID, toIndex: Int, groupId: UUID?) {
        guard let fromIndex = tabs.firstIndex(where: { $0.id == tabId }) else { return }

        let oldGroupId = tabs[fromIndex].groupId

        // Update group membership
        tabs[fromIndex].groupId = groupId

        // Move to new position
        let tab = tabs.remove(at: fromIndex)
        let targetIndex = min(max(0, toIndex), tabs.count)
        tabs.insert(tab, at: targetIndex)

        logger.debug("📍 Tab Move: \(tab.title) To Location \(targetIndex), Group: \(groupId?.uuidString ?? "None")")

        // Clean up old empty group if needed
        if let oldId = oldGroupId, oldId != groupId {
            cleanupEmptyGroup(oldId)
        }
    }

    /// Insert a tab received from another window (cross-window drag)
    func insertTabFromCrossWindow(_ tab: Tab, at index: Int, groupId: UUID?) {
        var newTab = tab
        newTab.groupId = groupId

        let targetIndex = min(max(0, index), tabs.count)
        tabs.insert(newTab, at: targetIndex)
        tabSlotPositions[newTab.id] = tabs.count - 1
        currentTabId = newTab.id

        logger.notice(" Insert across Window Tab: \(newTab.title) To Location \(targetIndex), Group: \(groupId?.uuidString ?? "None")")
    }

    /// Remove a tab by ID (used for cross-window drag source cleanup)
    func removeTabById(_ tabId: UUID) {
        guard let index = tabs.firstIndex(where: { $0.id == tabId }) else { return }
        let tab = tabs[index]
        let oldGroupId = tab.groupId

        tabs.remove(at: index)
        tabSlotPositions.removeValue(forKey: tabId)

        // Update current tab if needed
        if currentTabId == tabId {
            currentTabId = tabs.first?.id
        }

        // Clean up empty group
        if let oldId = oldGroupId {
            cleanupEmptyGroup(oldId)
        }

        logger.debug("🗑️ Remove across Window Tab: \(tab.title)")
    }

    /// Calculate the index for inserting a tab into a group at a specific child position
    /// - Parameters:
    ///   - groupId: The target group
    ///   - childIndex: Position within the group's children (NSOutlineView child index)
    /// - Returns: The absolute index in the tabs array
    func indexForInsertingIntoGroup(groupId: UUID, atChildIndex childIndex: Int) -> Int {
        let groupTabs = tabs.enumerated().filter { $0.element.groupId == groupId }
        if childIndex >= groupTabs.count {
            // Insert at end of group
            if let lastGroupTab = groupTabs.last {
                return lastGroupTab.offset + 1
            }
        } else if childIndex >= 0 && childIndex < groupTabs.count {
            // Insert before the child at childIndex
            return groupTabs[childIndex].offset
        }
        // Fallback: find first tab of this group
        return tabs.firstIndex(where: { $0.groupId == groupId }) ?? tabs.count
    }

    /// Calculate the index for inserting at root level (ungrouped)
    /// - Parameter rootChildIndex: The child index at root level (from NSOutlineView)
    /// - Returns: The absolute index in the tabs array
    func indexForInsertingAtRoot(atChildIndex rootChildIndex: Int) -> Int {
        let organized = tabsOrganizedByGroups()
        var currentIndex = 0

        for (i, groupData) in organized.enumerated() {
            if i == rootChildIndex {
                return currentIndex
            }
            currentIndex += groupData.tabs.count
        }

        // Insert at end
        return tabs.count
    }

    /// Add a tab to a group
    func addTabToGroup(tabId: UUID, groupId: UUID) {
        guard let tabIndex = tabs.firstIndex(where: { $0.id == tabId }) else { return }
        tabs[tabIndex].groupId = groupId
        logger.debug("📁 Tab \(tabId) Add Group \(groupId)")
    }

    /// Remove a tab from its group
    func removeTabFromGroup(tabId: UUID) {
        guard let tabIndex = tabs.firstIndex(where: { $0.id == tabId }) else { return }
        let oldGroupId = tabs[tabIndex].groupId
        tabs[tabIndex].groupId = nil
        logger.debug("📁 Tab \(tabId) From Group \(String(describing: oldGroupId)) Remove")

        // Clean up empty groups
        if let groupId = oldGroupId {
            cleanupEmptyGroup(groupId)
        }
    }

    /// Delete a group (removes all tabs from the group but keeps the tabs)
    func deleteGroup(groupId: UUID) {
        // Remove all tabs from this group
        for i in tabs.indices {
            if tabs[i].groupId == groupId {
                tabs[i].groupId = nil
            }
        }
        // Remove the group
        tabGroups.removeAll { $0.id == groupId }
        logger.debug("🗑️ Group \(groupId) Deleted")
    }

    /// Close all tabs in a group
    /// - Parameters:
    ///   - groupId: The group to close
    ///   - historyVM: Optional HistoryViewModel to record the closed group for restore
    func closeGroup(groupId: UUID, historyVM: HistoryViewModel? = nil) {
        guard let group = tabGroups.first(where: { $0.id == groupId }) else { return }
        let tabsToClose = tabs.filter { $0.groupId == groupId }

        // Record to history before closing
        if let historyVM = historyVM {
            historyVM.recordClosedGroup(group, tabs: tabsToClose)
        }

        for tab in tabsToClose {
            closeTab(id: tab.id)
        }
        // Remove the group
        tabGroups.removeAll { $0.id == groupId }
        logger.debug("🗑️ Group \(groupId) And all of it. Tab Closed")
    }

    /// Restore a group from history
    func restoreGroup(from historyGroup: HistoryGroup) {
        // Create a new group with the same properties
        let newGroup = createGroup(name: historyGroup.name, color: historyGroup.color)

        // Restore each tab in the group
        for historyTab in historyGroup.tabs {
            if let appURL = historyTab.appURL {
                // Import the ACTR package
                importActrPackageToGroup(from: appURL, groupId: newGroup.id)
            }
        }

        logger.debug("♻️ Grouping resumed: \(historyGroup.name.isEmpty ? "Unnamed" : historyGroup.name) Organisation \(historyGroup.tabs.count) individual Tab")
    }

    /// Import ACTR package and add to a specific group
    private func importActrPackageToGroup(from fileURL: URL, groupId: UUID) {
        let resolvedURL = fileURL.standardizedFileURL
        guard FileManager.default.fileExists(atPath: resolvedURL.path) else {
            logger.error(" Not found .actr Documents:\(resolvedURL.path)")
            return
        }

        // Check if it's a directory (dev mode) or archive
        var isDirectory: ObjCBool = false
        FileManager.default.fileExists(atPath: resolvedURL.path, isDirectory: &isDirectory)

        let rootDir: URL
        var tempDir: URL? = nil

        if isDirectory.boolValue {
            // Directory mode - use directly
            rootDir = resolvedURL
            tempDir = nil
        } else {
            // Package archive mode - extract to temp
            let temp = FileManager.default.temporaryDirectory
                .appendingPathComponent("actr-\(UUID().uuidString)", isDirectory: true)
            do {
                try FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
                try unzipActrPackage(archive: resolvedURL, to: temp)
            } catch {
                logger.error(" [restoreGroup] .actr Loading failed:\(error.localizedDescription)")
                return
            }
            rootDir = temp
            tempDir = temp
        }

        // Mark as read to silence "written to, but never read" warnings.
        if tempDir != nil { }

        do {
            let manifestInfo = try readPackageManifest(fromRoot: rootDir)
            let tabTitle = manifestInfo.name ?? resolvedURL.deletingPathExtension().lastPathComponent

            let guestScript = readGuestScript(fromRoot: rootDir)
            var unifiedScript: String? = nil
            if let unifiedFileName = manifestInfo.unifiedScript {
                unifiedScript = readScript(fromRoot: rootDir, fileName: unifiedFileName)
            }

            var leftPanelScript: String? = nil
            var rightPanelScript: String? = nil
            if let leftFileName = manifestInfo.leftPanelScript {
                leftPanelScript = readScript(fromRoot: rootDir, fileName: leftFileName)
            }
            if let rightFileName = manifestInfo.rightPanelScript {
                rightPanelScript = readScript(fromRoot: rootDir, fileName: rightFileName)
            }

            // Create tab with groupId
            let newTab = Tab(
                title: tabTitle,
                url: resolvedURL,
                appId: manifestInfo.appId,
                groupId: groupId,
                guestScript: guestScript,
                leftPanelScript: leftPanelScript,
                rightPanelScript: rightPanelScript,
                unifiedScript: unifiedScript,
                contractName: manifestInfo.contractName,
                contractVersion: manifestInfo.contractVersion,
                permissions: manifestInfo.permissions,
                leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
            )
            tabs.append(newTab)
            tabSlotPositions[newTab.id] = tabs.count - 1
            leftPanelVisibility[newTab.id] = newTab.leftPanelDefaultVisible
            logger.notice(" [restoreGroup] Loaded .actr To Group: \(tabTitle)")

        } catch {
            logger.error(" [restoreGroup] .actr Loading failed:\(error.localizedDescription)")
        }
    }

    /// Update group properties
    func updateGroup(groupId: UUID, name: String? = nil, color: TabGroupColor? = nil, isCollapsed: Bool? = nil) {
        guard let index = tabGroups.firstIndex(where: { $0.id == groupId }) else { return }
        if let name = name {
            tabGroups[index].name = name
        }
        if let color = color {
            tabGroups[index].color = color
        }
        if let isCollapsed = isCollapsed {
            tabGroups[index].isCollapsed = isCollapsed
        }
        logger.debug("📝 Group \(groupId) Updated")
    }

    /// Toggle group collapsed state
    func toggleGroupCollapsed(groupId: UUID) {
        guard let index = tabGroups.firstIndex(where: { $0.id == groupId }) else { return }
        tabGroups[index].isCollapsed.toggle()
        logger.debug("📁 Group \(groupId) Collapse Status: \(self.tabGroups[index].isCollapsed)")
    }

    /// Get group for a tab
    func group(for tabId: UUID) -> TabGroup? {
        guard let tab = tabs.first(where: { $0.id == tabId }),
              let groupId = tab.groupId else { return nil }
        return tabGroups.first { $0.id == groupId }
    }

    /// Get all tabs in a group
    func tabs(in groupId: UUID) -> [Tab] {
        return tabs.filter { $0.groupId == groupId }
    }

    /// Get tabs organized by group while maintaining original tab order
    /// Groups appear at the position of their first member tab
    func tabsOrganizedByGroups() -> [TabGroupWithTabs] {
        var result: [TabGroupWithTabs] = []
        var processedGroupIds: Set<UUID> = []
        var currentUngroupedTabs: [Tab] = []

        for tab in tabs {
            if let groupId = tab.groupId {
                // Flush any pending ungrouped tabs first
                if !currentUngroupedTabs.isEmpty {
                    result.append(TabGroupWithTabs(group: nil, tabs: currentUngroupedTabs))
                    currentUngroupedTabs = []
                }

                // If this is the first tab of this group, add the whole group
                if !processedGroupIds.contains(groupId) {
                    processedGroupIds.insert(groupId)
                    if let group = tabGroups.first(where: { $0.id == groupId }) {
                        // Get all tabs in this group (maintaining their relative order)
                        let groupTabs = tabs.filter { $0.groupId == groupId }
                        result.append(TabGroupWithTabs(group: group, tabs: groupTabs))
                    }
                }
                // Skip - already added with group
            } else {
                // Ungrouped tab - collect them
                currentUngroupedTabs.append(tab)
            }
        }

        // Flush remaining ungrouped tabs
        if !currentUngroupedTabs.isEmpty {
            result.append(TabGroupWithTabs(group: nil, tabs: currentUngroupedTabs))
        }

        return result
    }

    /// Clean up empty group (called after removing a tab from a group)
    private func cleanupEmptyGroup(_ groupId: UUID) {
        let hasTabsInGroup = tabs.contains { $0.groupId == groupId }
        if !hasTabsInGroup {
            tabGroups.removeAll { $0.id == groupId }
            logger.debug("🧹 Empty Group \(groupId) Automatically deleted")
        }
    }

    /// Create a group from selected tabs
    func createGroupFromTabs(_ tabIds: [UUID], name: String = "", color: TabGroupColor = .gray) {
        let group = createGroup(name: name, color: color)
        for tabId in tabIds {
            addTabToGroup(tabId: tabId, groupId: group.id)
        }
    }

    // MARK: - Grid Slot Position Methods

    /// Initialize slot positions for all tabs
    func initializeSlotPositions() {
        for (index, tab) in tabs.enumerated() {
            tabSlotPositions[tab.id] = index
        }
        logger.debug("📍 Initialized slot positions: \(self.tabs.count) tabs")
    }

    /// Swap slot positions between two tabs
    func swapSlotPositions(tabId1: UUID?, tabId2: UUID?) {
        guard let id1 = tabId1, let id2 = tabId2 else {
            logger.warning(" Cannot swap: need two valid tab IDs")
            return
        }

        guard let index1 = tabs.firstIndex(where: { $0.id == id1 }),
              let index2 = tabs.firstIndex(where: { $0.id == id2 }) else {
            logger.warning(" Cannot swap: tab not found in tabs array")
            return
        }

        // Switch tabs Location of elements in array
        tabs.swapAt(index1, index2)

        // Synchronize Update slot positions
        if let pos1 = tabSlotPositions[id1], let pos2 = tabSlotPositions[id2] {
            tabSlotPositions[id1] = pos2
            tabSlotPositions[id2] = pos1
        } else {
            // slot positions Re-initiation when missing
            initializeSlotPositions()
        }

        logger.debug("🔄 Swapped tabs at indices: \(index1) ↔ \(index2)")
    }

    /// Get tabs sorted by their slot position
    func tabsSortedBySlotPosition() -> [(tab: Tab, position: Int)] {
        return tabs.compactMap { tab in
            guard let position = tabSlotPositions[tab.id] else { return nil }
            return (tab, position)
        }.sorted { $0.position < $1.position }
    }

    func updateCurrentTabTitle(_ title: String) {
        updateCurrentTabInfo(title: title, icon: nil, appUrl: nil)
    }

    /// Update current tab info (title, icon, appUrl)
    func updateCurrentTabInfo(title: String? = nil, icon: String? = nil, appUrl: URL? = nil) {
        guard let currentTabId = currentTabId,
              let index = tabs.firstIndex(where: { $0.id == currentTabId }) else { return }

        if let title = title {
            tabs[index].title = title
        }
        if let icon = icon {
            tabs[index].icon = icon
        }
        if let appUrl = appUrl {
            tabs[index].url = appUrl
        }

        // Send Notification To RN
        NotificationCenter.default.post(
            name: .tabUpdatedFromNative,
            object: nil,
            userInfo: tabUpdatedUserInfo(
                tabId: currentTabId,
                title: title,
                icon: icon,
                appUrl: appUrl
            )
        )

        scheduleSaveTabs()
    }

    func closeOtherTabs(keepTabId: UUID) {
        let tabsToClose = tabs.filter { $0.id != keepTabId }
        for tab in tabsToClose {
            closeTab(id: tab.id)
        }
    }

    func closeCurrentTab() {
        guard let id = currentTabId else { return }
        closeTab(id: id)
    }

    /// Reload the current tab by re-reading its source .actr package
    func reloadCurrentTab() {
        guard let id = currentTabId,
              let index = tabs.firstIndex(where: { $0.id == id }) else {
            logger.warning("⚠️ [reloadCurrentTab] Cannot Refresh: No Current tab")
            return
        }

        logger.notice("🔄 [reloadCurrentTab] Start Refresh tab: \(self.tabs[index].title), reloadNonce: \(self.tabs[index].reloadNonce)")

        // If there is no source URL, only add reloadNonce Force RN Remount
        guard let sourceURL = tabs[index].url else {
            logger.notice("🔄 [reloadCurrentTab] Unsourced URL, only mandatory RN remount")
            tabs[index].reloadNonce &+= 1
            objectWillChange.send()
            return
        }

        logger.debug("🔄 [reloadCurrentTab] It's a source. URLReread package")

        // Re-read the .actr package
        let resolvedURL = sourceURL.standardizedFileURL
        guard FileManager.default.fileExists(atPath: resolvedURL.path) else {
            logger.error(" [reloadCurrentTab] Source file does not exist:\(resolvedURL.path)")
            return
        }

        // Check if it's a directory (dev mode) or archive
        var isDirectory: ObjCBool = false
        FileManager.default.fileExists(atPath: resolvedURL.path, isDirectory: &isDirectory)

        let rootDir: URL

        if isDirectory.boolValue {
            // Directory mode - use directly
            rootDir = resolvedURL
        } else {
            // Package archive mode - extract to temp
            let tempDir = FileManager.default.temporaryDirectory
                .appendingPathComponent("actr-\(UUID().uuidString)", isDirectory: true)
            do {
                try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
                try unzipActrPackage(archive: resolvedURL, to: tempDir)
            } catch {
                logger.error(" [reloadCurrentTab] Depression failed:\(error.localizedDescription)")
                return
            }
            rootDir = tempDir
        }

        do {
            let manifestInfo = try readPackageManifest(fromRoot: rootDir)

            // Read scripts
            var unifiedScript: String? = nil
            if let unifiedFileName = manifestInfo.unifiedScript {
                unifiedScript = readScript(fromRoot: rootDir, fileName: unifiedFileName)
            }

            var leftPanelScript: String? = nil
            var rightPanelScript: String? = nil
            if let leftFileName = manifestInfo.leftPanelScript {
                leftPanelScript = readScript(fromRoot: rootDir, fileName: leftFileName)
            }
            if let rightFileName = manifestInfo.rightPanelScript {
                rightPanelScript = readScript(fromRoot: rootDir, fileName: rightFileName)
            }

            let guestScript = readGuestScript(fromRoot: rootDir)

            // Update the tab in place
            tabs[index].unifiedScript = unifiedScript
            tabs[index].leftPanelScript = leftPanelScript
            tabs[index].rightPanelScript = rightPanelScript
            tabs[index].guestScript = guestScript
            tabs[index].appId = manifestInfo.appId
            tabs[index].contractName = manifestInfo.contractName
            tabs[index].contractVersion = manifestInfo.contractVersion
            tabs[index].permissions = manifestInfo.permissions
            tabs[index].leftPanelDefaultVisible = manifestInfo.leftPanelDefaultVisible
            tabs[index].rightPanelDefaultVisible = manifestInfo.rightPanelDefaultVisible
            leftPanelVisibility[tabs[index].id] = manifestInfo.leftPanelDefaultVisible

            if let name = manifestInfo.name {
                tabs[index].title = name
            }

            logger.notice(" [reloadCurrentTab] Refresh finished: \(self.tabs[index].title)")

            // Add Refresh nonce,Alarm RN Side remount
            tabs[index].reloadNonce &+= 1

            // Force UI update by triggering objectWillChange
            objectWillChange.send()

        } catch {
            logger.error(" [reloadCurrentTab] Synchronising "%s"\(error.localizedDescription)")
        }
    }

    // MARK: - Per-Tab Left Panel Visibility

    func isLeftPanelVisible(for tabId: UUID?) -> Bool {
        guard let id = tabId else { return true }
        return leftPanelVisibility[id] ?? false
    }

    func toggleLeftPanelForCurrentTab() {
        // Focus Use in Mode focus tabId, or use currentTabId
        let id: UUID
        if case .focus(let focusTabId) = actroMode {
            id = focusTabId
        } else if let currentId = currentTabId {
            id = currentId
        } else {
            logger.warning(" [toggleLeftPanel] Could not close temporary folder: %s tab, cannot be switched")
            return
        }

        var visibility = leftPanelVisibility
        let current = visibility[id] ?? true
        visibility[id] = !current
        leftPanelVisibility = visibility  // Revalue the entire dictionary to ensure that it triggers @Published
        logger.debug("🔄 [toggleLeftPanel] Tab \(id.uuidString.prefix(8)) Left Panel: \(current) -> \(!current)")
    }

    // MARK: - .actr Apply Load

    /// From Local .actr Package to create a new tab and load basic information
    func importACTRPackage(from fileURL: URL) {
        let resolvedURL = fileURL.standardizedFileURL
        guard FileManager.default.fileExists(atPath: resolvedURL.path) else {
            logger.error(" Not found .actr Documents:\(resolvedURL.path)")
            return
        }

        // Check if it's a directory (dev mode) or archive
        var isDirectory: ObjCBool = false
        FileManager.default.fileExists(atPath: resolvedURL.path, isDirectory: &isDirectory)

        let rootDir: URL

        if isDirectory.boolValue {
            // Directory mode - use directly
            rootDir = resolvedURL
        } else {
            // Package archive mode - extract to temp
            let tempDir = FileManager.default.temporaryDirectory
                .appendingPathComponent("actr-\(UUID().uuidString)", isDirectory: true)
            do {
                try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
                try unzipActrPackage(archive: resolvedURL, to: tempDir)
            } catch {
                logger.error(" .actr Loading failed:\(error.localizedDescription)")
                return
            }
            rootDir = tempDir
        }

        do {
            let manifestInfo = try readPackageManifest(fromRoot: rootDir)
            let tabTitle = manifestInfo.name ?? resolvedURL.deletingPathExtension().lastPathComponent

            // Read guest Script
            let guestScript = readGuestScript(fromRoot: rootDir)

            // v2.0: Read Priority unified script (Single Engine Structure)
            var unifiedScript: String? = nil
            if let unifiedFileName = manifestInfo.unifiedScript {
                unifiedScript = readScript(fromRoot: rootDir, fileName: unifiedFileName)
                logger.notice(" [BrowserViewModel] From manifest.toml Read unified: \(unifiedFileName), Script exists: \(unifiedScript != nil)")
            }

            // From manifest.toml Read Panel Script File Name
            var leftPanelScript: String? = nil
            var rightPanelScript: String? = nil

            if let leftFileName = manifestInfo.leftPanelScript {
                leftPanelScript = readScript(fromRoot: rootDir, fileName: leftFileName)
                logger.debug("🔍 [BrowserViewModel] From manifest.toml Read leftPanel: \(leftFileName), Script exists: \(leftPanelScript != nil)")
            }

            if let rightFileName = manifestInfo.rightPanelScript {
                rightPanelScript = readScript(fromRoot: rootDir, fileName: rightFileName)
                logger.debug("🔍 [BrowserViewModel] From manifest.toml Read rightPanel: \(rightFileName), Script exists: \(rightPanelScript != nil)")
            }

            // v2.0: Use Priority Single Engine Structure
            if let unified = unifiedScript {
                logger.debug("🚀 [BrowserViewModel] Use Single Engine Structure (unified bundle)")
                appendRuntimeTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    appId: manifestInfo.appId,
                    unifiedScript: unified,
                    contractName: manifestInfo.contractName,
                    contractVersion: manifestInfo.contractVersion,
                    permissions: manifestInfo.permissions,
                    leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                    rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                )
            }
            // Legacy: Double Engine Structure
            else if leftPanelScript != nil || rightPanelScript != nil {
                logger.debug("🔄 [BrowserViewModel] Use Legacy Double Engine Structure")
                appendRuntimeTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    appId: manifestInfo.appId,
                    leftPanelScript: leftPanelScript,
                    rightPanelScript: rightPanelScript,
                    contractName: manifestInfo.contractName,
                    contractVersion: manifestInfo.contractVersion,
                    permissions: manifestInfo.permissions,
                    leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                    rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                )
            } else if let script = guestScript {
                // Old architecture: one compatible guest.js
                logger.debug("🔄 [BrowserViewModel] Use compatible mode (guestScript)")
                appendRuntimeTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    appId: manifestInfo.appId,
                    guestScript: script,
                    contractName: manifestInfo.contractName,
                    contractVersion: manifestInfo.contractVersion,
                    permissions: manifestInfo.permissions
                )
            } else {
                // No script, display application information page
                let html = renderACTRInfoHTML(
                    name: manifestInfo.name ?? "Unknown App",
                    version: manifestInfo.version ?? "0.0.0",
                    description: manifestInfo.description ?? "No description provided.",
                    sourcePath: resolvedURL.path
                )
                appendRuntimeTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    appId: manifestInfo.appId,
                    htmlContent: html
                )
            }
        } catch {
            logger.error(" .actr Loading failed:\(error.localizedDescription)")
            let fallbackHTML = renderACTRInfoHTML(
                name: resolvedURL.deletingPathExtension().lastPathComponent,
                version: "unknown",
                description: "Loading failed:\(error.localizedDescription)",
                sourcePath: resolvedURL.path
            )
            appendRuntimeTab(
                title: resolvedURL.deletingPathExtension().lastPathComponent,
                sourceURL: resolvedURL,
                htmlContent: fallbackHTML
            )
        }
    }

    /// Async version of importACTRPackage - runs file I/O on background thread
    /// Call this from background thread, it will dispatch UI updates to main thread
    private func importACTRPackageAsync(from fileURL: URL) {
        let resolvedURL = fileURL.standardizedFileURL
        guard FileManager.default.fileExists(atPath: resolvedURL.path) else {
            logger.error(" Not found .actr Documents:\(resolvedURL.path)")
            return
        }

        // Check if it's a directory (dev mode) or archive
        var isDirectory: ObjCBool = false
        FileManager.default.fileExists(atPath: resolvedURL.path, isDirectory: &isDirectory)

        let rootDir: URL

        if isDirectory.boolValue {
            // Directory mode - use directly
            rootDir = resolvedURL
        } else {
            // Package archive mode - extract to temp
            let tempDir = FileManager.default.temporaryDirectory
                .appendingPathComponent("actr-\(UUID().uuidString)", isDirectory: true)
            do {
                try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
                try unzipActrPackage(archive: resolvedURL, to: tempDir)
            } catch {
                logger.error(" .actr Loading failed:\(error.localizedDescription)")
                return
            }
            rootDir = tempDir
        }

        do {
            let manifestInfo = try readPackageManifest(fromRoot: rootDir)
            let tabTitle = manifestInfo.name ?? resolvedURL.deletingPathExtension().lastPathComponent

            let guestScript = readGuestScript(fromRoot: rootDir)

            var unifiedScript: String? = nil
            if let unifiedFileName = manifestInfo.unifiedScript {
                unifiedScript = readScript(fromRoot: rootDir, fileName: unifiedFileName)
            }

            var leftPanelScript: String? = nil
            var rightPanelScript: String? = nil
            if let leftFileName = manifestInfo.leftPanelScript {
                leftPanelScript = readScript(fromRoot: rootDir, fileName: leftFileName)
            }
            if let rightFileName = manifestInfo.rightPanelScript {
                rightPanelScript = readScript(fromRoot: rootDir, fileName: rightFileName)
            }

            // Update UI on main thread
            DispatchQueue.main.async { [weak self] in
                if let unified = unifiedScript {
                    self?.appendRuntimeTab(
                        title: tabTitle,
                        sourceURL: resolvedURL,
                        appId: manifestInfo.appId,
                        unifiedScript: unified,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                    )
                } else if leftPanelScript != nil || rightPanelScript != nil {
                    self?.appendRuntimeTab(
                        title: tabTitle,
                        sourceURL: resolvedURL,
                        appId: manifestInfo.appId,
                        leftPanelScript: leftPanelScript,
                        rightPanelScript: rightPanelScript,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                    )
                } else if let script = guestScript {
                    self?.appendRuntimeTab(
                        title: tabTitle,
                        sourceURL: resolvedURL,
                        appId: manifestInfo.appId,
                        guestScript: script,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions
                    )
                } else {
                    let html = self?.renderACTRInfoHTML(
                        name: manifestInfo.name ?? "Unknown App",
                        version: manifestInfo.version ?? "0.0.0",
                        description: manifestInfo.description ?? "No description provided.",
                        sourcePath: resolvedURL.path
                    ) ?? ""
                    self?.appendRuntimeTab(
                        title: tabTitle,
                        sourceURL: resolvedURL,
                        appId: manifestInfo.appId,
                        htmlContent: html
                    )
                }
            }
        } catch {
            logger.error(" .actr Loading failed:\(error.localizedDescription)")
            DispatchQueue.main.async { [weak self] in
                let fallbackHTML = self?.renderACTRInfoHTML(
                    name: resolvedURL.deletingPathExtension().lastPathComponent,
                    version: "unknown",
                    description: "Loading failed:\(error.localizedDescription)",
                    sourcePath: resolvedURL.path
                ) ?? ""
                self?.appendRuntimeTab(
                    title: resolvedURL.deletingPathExtension().lastPathComponent,
                    sourceURL: resolvedURL,
                    htmlContent: fallbackHTML
                )
            }
        }
    }

    /// Load from development directory (not packaged .actr)
    /// Used for hot-reload development workflow and bookmark/history loading
    func loadFromDevDirectory(_ directoryURL: URL) {
        guard FileManager.default.fileExists(atPath: directoryURL.path) else {
            logger.error(" Can not get folder: %s: %s\(directoryURL.path)")
            return
        }

        do {
            let manifestInfo = try readPackageManifest(fromRoot: directoryURL)
            let tabTitle = manifestInfo.name ?? directoryURL.lastPathComponent
            logger.debug("📂 [loadFromDevDirectory] Load Development Directory: \(tabTitle)")

            let guestScript = readGuestScript(fromRoot: directoryURL)

            var unifiedScript: String? = nil
            if let unifiedFileName = manifestInfo.unifiedScript {
                unifiedScript = readScript(fromRoot: directoryURL, fileName: unifiedFileName)
                logger.notice(" [loadFromDevDirectory] Read unified: \(unifiedFileName), Length: \(unifiedScript?.count ?? 0)")
            }

            var leftPanelScript: String? = nil
            var rightPanelScript: String? = nil
            if let leftFileName = manifestInfo.leftPanelScript {
                leftPanelScript = readScript(fromRoot: directoryURL, fileName: leftFileName)
                logger.notice(" [loadFromDevDirectory] Read leftPanel: \(leftFileName), Length: \(leftPanelScript?.count ?? 0)")
            }
            if let rightFileName = manifestInfo.rightPanelScript {
                rightPanelScript = readScript(fromRoot: directoryURL, fileName: rightFileName)
                logger.notice(" [loadFromDevDirectory] Read rightPanel: \(rightFileName), Length: \(rightPanelScript?.count ?? 0)")
            }

            // Update UI on main thread
            DispatchQueue.main.async { [weak self] in
                if let unified = unifiedScript {
                    self?.appendRuntimeTab(
                        title: tabTitle,
                        sourceURL: directoryURL,
                        appId: manifestInfo.appId,
                        unifiedScript: unified,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                    )
                } else if leftPanelScript != nil || rightPanelScript != nil {
                    self?.appendRuntimeTab(
                        title: tabTitle,
                        sourceURL: directoryURL,
                        appId: manifestInfo.appId,
                        leftPanelScript: leftPanelScript,
                        rightPanelScript: rightPanelScript,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                    )
                } else if let script = guestScript {
                    self?.appendRuntimeTab(
                        title: tabTitle,
                        sourceURL: directoryURL,
                        appId: manifestInfo.appId,
                        guestScript: script,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions
                    )
                } else {
                    logger.warning(" [loadFromDevDirectory] No script file found")
                }
            }
        } catch {
            logger.error(" Development directory load failed:\(error.localizedDescription)")
        }
    }

    private func appendRuntimeTab(
        title: String,
        sourceURL: URL,
        appId: String? = nil,
        htmlContent: String? = nil,
        guestScript: String? = nil,
        leftPanelScript: String? = nil,
        rightPanelScript: String? = nil,
        unifiedScript: String? = nil,
        contractName: String? = nil,
        contractVersion: Int? = nil,
        permissions: [String] = [],
        leftPanelDefaultVisible: Bool = false,
        rightPanelDefaultVisible: Bool = false
    ) {
        let newTab = Tab(
            title: title,
            url: sourceURL,
            appId: appId,
            htmlContent: htmlContent,
            guestScript: guestScript,
            leftPanelScript: leftPanelScript,
            rightPanelScript: rightPanelScript,
            unifiedScript: unifiedScript,
            contractName: contractName,
            contractVersion: contractVersion,
            permissions: permissions,
            leftPanelDefaultVisible: leftPanelDefaultVisible,
            rightPanelDefaultVisible: rightPanelDefaultVisible
        )
        tabs.append(newTab)
        currentTabId = newTab.id
        tabSlotPositions[newTab.id] = tabs.count - 1
        leftPanelVisibility[newTab.id] = leftPanelDefaultVisible

        // Automatically enter Focus Mode
        actroMode = .focus(tabId: newTab.id)

        logger.notice(" Loaded .actr: \(title)")

        if let unified = unifiedScript {
            logger.debug("  ├─ Unified script (Single Engine): \(unified.count) Bytes")
            logger.debug("  ├─ Default for left panel\(leftPanelDefaultVisible ? "Show" : "Hide")")
            logger.debug("  └─ Default for right panel\(rightPanelDefaultVisible ? "Show" : "Hide")")
        } else if let leftScript = leftPanelScript {
            logger.debug("  ├─ Left Panel Script (Legacy): \(leftScript.count) Bytes")
            if let rightScript = rightPanelScript {
                logger.debug("  ├─ Right Extension Panel Script (Legacy): \(rightScript.count) Bytes")
                logger.debug("  └─ Default for right panel\(rightPanelDefaultVisible ? "Show" : "Hide")")
            }
        }

        NotificationCenter.default.post(
            name: .tabOpenedFromNative,
            object: nil,
            userInfo: tabOpenedUserInfo(for: newTab)
        )
    }

    private func unzipActrPackage(archive: URL, to destination: URL) throws {
        let fileManager = FileManager.default

        if !fileManager.fileExists(atPath: destination.path) {
            try fileManager.createDirectory(
                at: destination,
                withIntermediateDirectories: true
            )
        }

        try fileManager.unzipItem(
            at: archive,
            to: destination
        )
    }

    private func readPackageManifest(fromRoot root: URL) throws -> ActrPackageManifestInfo {
        try ActrPackageManifestReader.read(fromRoot: root)
    }

    /// Universal script reading method
    private func readScript(fromRoot root: URL, fileName: String) -> String? {
        let fm = FileManager.default

        // If fileName Include path (e. g.) "dist/unified-app.js") Directly spell-read
        if fileName.contains("/") {
            let fullPath = root.appendingPathComponent(fileName)
            if let script = try? String(contentsOf: fullPath, encoding: .utf8) {
                logger.notice(" I found it. \(fileName): \(fullPath.path)")
                return script
            }
        }

        // First look in root directory
        if let direct = try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: nil).first(where: { $0.lastPathComponent == fileName }) {
            if let script = try? String(contentsOf: direct, encoding: .utf8) {
                logger.notice(" I found it. \(fileName): \(direct.path)")
                return script
            }
        }

        // If root directory is not available, recursively search
        if let enumerator = fm.enumerator(at: root, includingPropertiesForKeys: nil) {
            for case let fileURL as URL in enumerator {
                if fileURL.lastPathComponent == fileName {
                    if let script = try? String(contentsOf: fileURL, encoding: .utf8) {
                        logger.notice(" I found it. \(fileName): \(fileURL.path)")
                        return script
                    }
                }
            }
        }

        return nil
    }

    /// Read guest.js (Backward compatibility)
    private func readGuestScript(fromRoot root: URL) -> String? {
        return readScript(fromRoot: root, fileName: "guest.js")
    }

    private func renderACTRInfoHTML(name: String, version: String, description: String, sourcePath: String) -> String {
        """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="utf-8">
            <meta name="viewport" content="width=device-width, initial-scale=1.0">
            <style>
                body {
                    font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, sans-serif;
                    padding: 32px;
                    color: #e6e8ee;
                    margin: 0;
                    min-height: 100vh;
                    background: linear-gradient(135deg, #0f172a 0%, #111827 50%, #0b1220 100%);
                }
                .card {
                    max-width: 860px;
                    margin: 0 auto;
                    padding: 28px;
                    background: rgba(255, 255, 255, 0.04);
                    border: 1px solid rgba(255, 255, 255, 0.08);
                    border-radius: 16px;
                    box-shadow: 0 15px 50px rgba(0,0,0,0.35);
                    backdrop-filter: blur(10px);
                }
                h1 { margin: 0 0 12px 0; font-size: 28px; letter-spacing: -0.02em; }
                .meta { color: #9ca3af; margin-bottom: 18px; }
                .pill { display: inline-block; padding: 6px 10px; margin-right: 8px;
                        border-radius: 10px; background: rgba(96, 165, 250, 0.12); color: #bfdbfe; font-size: 12px; }
                .desc { line-height: 1.6; color: #d1d5db; margin-bottom: 20px; }
                .path { font-family: 'Menlo', 'SFMono-Regular', ui-monospace, monospace;
                        background: rgba(255,255,255,0.05); padding: 10px 12px; border-radius: 10px; }
            </style>
        </head>
        <body>
            <div class="card">
                <h1>🧩 \(name)</h1>
                <div class="meta">
                    <span class="pill">version \(version)</span>
                    <span class="pill">ACTR package</span>
                </div>
                <div class="desc">\(description)</div>
                <div class="path">\(sourcePath)</div>
            </div>
        </body>
        </html>
        """
    }
}
