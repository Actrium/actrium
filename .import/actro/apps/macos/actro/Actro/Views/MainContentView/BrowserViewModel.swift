import Foundation
import AppKit // Import AppKit for NSImage
import Combine

private let logger = AppLogger.viewModel

/// Actro display mode - stored in ViewModel to survive view recreation
enum ActroMode: Equatable {
    case overview
    case focus(tabId: UUID)

    var isInFocusMode: Bool {
        if case .focus = self { return true }
        return false
    }
}

/// Protocol for objects that publish actroMode changes
/// Used by ViewBridgeCoordinatorManager to subscribe to mode changes
protocol ActroModePublisher: AnyObject {
    var actroMode: ActroMode { get }
    var actroModePublisher: AnyPublisher<ActroMode, Never> { get }
}

@MainActor
class BrowserViewModel: ObservableObject, ActroModePublisher {
    // MARK: - ActroModePublisher conformance
    var actroModePublisher: AnyPublisher<ActroMode, Never> {
        $actroMode.eraseToAnyPublisher()
    }
    @Published var tabs: [Tab] = []
    @Published var currentTabId: UUID?
    // 默认所有标签页左侧面板可见；仅在用户切换时记录覆盖值
    @Published private var leftPanelVisibility: [UUID: Bool] = [:]

    // MARK: - Tab Groups
    @Published var tabGroups: [TabGroup] = []

    // 🔴 开发调试：启动时直接进入 Focus 模式
    @Published var shouldStartInFocusMode: Bool = true

    /// Actro display mode (Overview vs Focus)
    /// Stored here to survive view recreation when DevTools layout changes
    @Published var actroMode: ActroMode = .overview

    /// Flag to trigger address bar focus (set by addNewTab, consumed by view)
    @Published var shouldAutoFocusAddressBar: Bool = false

    /// Current tab loading state (for web content)
    @Published var isCurrentTabLoading: Bool = false

    /// Current tab loading progress (0.0 to 1.0)
    @Published var loadingProgress: Double = 0

    // MARK: - Grid Slot Position Management

    /// Tab ID → grid slot index mapping
    @Published var tabSlotPositions: [UUID: Int] = [:]

    /// Toolbar 实际高度（从视图测量得到）
    @Published var toolbarHeight: CGFloat = 52 // 默认值，会在视图中更新

    /// History view model reference for recording ACTR app visits
    weak var historyVM: HistoryViewModel?
    private let browserDataRepository = BrowserDataRepository.shared

    var currentTab: Tab? {
        guard let currentTabId = currentTabId else { return nil }
        return tabs.first(where: { $0.id == currentTabId })
    }

    private var sshObservers: [NSObjectProtocol] = []

    init() {
        // Initialize slot positions first
        initializeSlotPositions()

        // Create initial StartUp tab
        let startTab = Tab(
            title: "Start Page",
            systemTabType: .startPage,
            backgroundColor: "#1e1e1e" // Fixed dark color
        )
        tabs.append(startTab)
        currentTabId = startTab.id
        tabSlotPositions[startTab.id] = 0
        actroMode = .focus(tabId: startTab.id)

        setupSSHObservers()
    }

    private func persistBrowserPageSession() {
        browserDataRepository.saveSession(tabs: tabs, currentTabId: currentTabId)
    }

    private func setupSSHObservers() {
        sshObservers.append(
            NotificationCenter.default.addObserver(
                forName: .sshCloseTab,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                guard let self,
                      let sessionId = notification.userInfo?["sessionId"] as? String,
                      let tab = self.tabs.first(where: { $0.sshSessionId == sessionId }) else { return }
                self.closeTab(id: tab.id)
            }
        )

        sshObservers.append(
            NotificationCenter.default.addObserver(
                forName: .sshTitleChanged,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                guard let self,
                      let sessionId = notification.userInfo?["sessionId"] as? String,
                      let title = notification.userInfo?["title"] as? String,
                      let tabIndex = self.tabs.firstIndex(where: { $0.sshSessionId == sessionId }) else { return }
                self.tabs[tabIndex].title = title
            }
        )

        sshObservers.append(
            NotificationCenter.default.addObserver(
                forName: .sshReconnect,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                guard let self,
                      let sessionId = notification.userInfo?["sessionId"] as? String,
                      let tabIndex = self.tabs.firstIndex(where: { $0.sshSessionId == sessionId }) else { return }
                // Disconnect old session, assign new sessionId — XPC service will create actual SSH session
                TerminalXPCBridge.shared.disconnectSession(sessionId: sessionId)
                let newSessionId = UUID().uuidString
                self.tabs[tabIndex].sshSessionId = newSessionId
                self.objectWillChange.send()
            }
        )

        // Remote desktop observers
        sshObservers.append(
            NotificationCenter.default.addObserver(
                forName: .remoteDesktopCloseTab,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                guard let self,
                      let sessionId = notification.userInfo?["sessionId"] as? String,
                      let tab = self.tabs.first(where: { $0.remoteDesktopSessionId == sessionId }) else { return }
                self.closeTab(id: tab.id)
            }
        )
        sshObservers.append(
            NotificationCenter.default.addObserver(
                forName: .remoteDesktopReconnect,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                guard let self,
                      let sessionId = notification.userInfo?["sessionId"] as? String,
                      let tabIndex = self.tabs.firstIndex(where: { $0.remoteDesktopSessionId == sessionId }) else { return }
                RemoteDesktopXPCBridge.shared.disconnectSession(sessionId: sessionId)
                let newSessionId = UUID().uuidString
                self.tabs[tabIndex].remoteDesktopSessionId = newSessionId
                self.objectWillChange.send()
            }
        )
    }

    @discardableResult
    func addNewTab(focusAddressBar: Bool = true) -> Tab {
        logger.debug("🆕 开始创建新标签页...")
        let newTab = Tab(
            title: "New Tab",
            systemTabType: .startPage,
            backgroundColor: "#1e1e1e" // Fixed dark color
        )
        tabs.append(newTab)
        currentTabId = newTab.id

        // 简化：直接添加到末尾
        tabSlotPositions[newTab.id] = tabs.count - 1

        // 新建 Tab 直接进入 Focus 模式
        actroMode = .focus(tabId: newTab.id)
        // 只有纯粹新建空白 Tab 时才自动聚焦地址栏，从链接打开时不聚焦
        shouldAutoFocusAddressBar = focusAddressBar

        logger.notice(" 新标签页已创建: \(newTab.title), 位置: \(self.tabs.count - 1)")
        persistBrowserPageSession()
        return newTab
    }

    /// Open or switch to a system tab (History, Bookmarks)
    /// Returns the existing tab if already open, otherwise creates a new one
    @discardableResult
    func openSystemTab(_ type: SystemTabType) -> Tab {
        // Check if a system tab of this type is already open
        if let existingTab = tabs.first(where: { $0.systemTabType == type }) {
            // Switch to existing tab
            currentTabId = existingTab.id
            actroMode = .focus(tabId: existingTab.id)
            logger.debug("🔄 切换到已有系统标签: \(type.title)")
            persistBrowserPageSession()
            return existingTab
        }

        // Create new system tab with about: URL
        let newTab = Tab(
            title: type.title,
            url: type.aboutURL,
            systemTabType: type,
            backgroundColor: "#1e1e1e"  // Dark background for system tabs
        )
        tabs.append(newTab)
        currentTabId = newTab.id
        tabSlotPositions[newTab.id] = tabs.count - 1
        actroMode = .focus(tabId: newTab.id)

        logger.notice("📱 已创建系统标签: \(type.title) (\(type.aboutURL.absoluteString)), 位置: \(self.tabs.count - 1)")
        persistBrowserPageSession()
        return newTab
    }

    func switchToTab(id: UUID) {
        if let tab = tabs.first(where: { $0.id == id }) {
            logger.debug("🔄 切换到标签页: \(tab.title)")
        }
        currentTabId = id
        // Ensure actro is in focus mode when switching tabs
        actroMode = .focus(tabId: id)
        persistBrowserPageSession()
    }

    func closeTab(id: UUID) {
        // Get the slot position of the tab being closed
        guard let closedPosition = tabSlotPositions[id] else {
            logger.warning(" Cannot find tab slot position")
            return
        }

        // 如果这是最后一个标签页，关闭后退出应用
        if tabs.count == 1 {
            // Disconnect SSH/SFTP/RemoteDesktop session before exiting
            if let tab = tabs.first {
                if let sessionId = tab.sshSessionId {
                    TerminalXPCBridge.shared.disconnectSession(sessionId: sessionId)
                }
                if let sessionId = tab.sftpSessionId {
                    SFTPSessionBridge.shared.disconnectSession(sessionId: sessionId)
                    SFTPBrowserViewModel.removeViewModel(for: sessionId)
                }
                if let sessionId = tab.remoteDesktopSessionId {
                    RemoteDesktopXPCBridge.shared.disconnectSession(sessionId: sessionId)
                }
            }
            logger.debug("🚪 关闭最后一个标签页，应用即将退出")
            browserDataRepository.saveSession(tabs: [], currentTabId: nil)
            NSApplication.shared.terminate(nil)
            return
        }

        // 如果关闭的是当前标签，切换到相邻标签
        if currentTabId == id {
            let wasInFocusMode = actroMode.isInFocusMode
            if let index = tabs.firstIndex(where: { $0.id == id }) {
                var newTabId: UUID?
                if index > 0 {
                    newTabId = tabs[index - 1].id
                } else if index < tabs.count - 1 {
                    newTabId = tabs[index + 1].id
                }
                if let newTabId {
                    currentTabId = newTabId
                    // Preserve Focus mode when closing current tab
                    if wasInFocusMode {
                        actroMode = .focus(tabId: newTabId)
                    }
                }
            }
        }

        // Disconnect SSH/SFTP/RemoteDesktop session if this tab has one
        if let tab = tabs.first(where: { $0.id == id }) {
            if let sessionId = tab.sshSessionId {
                TerminalXPCBridge.shared.disconnectSession(sessionId: sessionId)
            }
            if let sessionId = tab.sftpSessionId {
                SFTPSessionBridge.shared.disconnectSession(sessionId: sessionId)
                SFTPBrowserViewModel.removeViewModel(for: sessionId)
            }
            if let sessionId = tab.remoteDesktopSessionId {
                RemoteDesktopXPCBridge.shared.disconnectSession(sessionId: sessionId)
            }
        }

        // 删除标签页
        tabs.removeAll(where: { $0.id == id })

        // 删除该标签的位置映射
        tabSlotPositions.removeValue(forKey: id)

        // 关闭标签时走统一渲染生命周期门面
        RendererLifecycleManager.shared.tabDidClose(id)

        // 重新整理位置：将所有大于被删除位置的位置减1
        for tabId in tabs.map({ $0.id }) {
            if let position = tabSlotPositions[tabId], position > closedPosition {
                tabSlotPositions[tabId] = position - 1
            }
        }

        logger.debug("🗑️ 关闭标签页，位置 \(closedPosition)，剩余 \(self.tabs.count) 个标签页")
        persistBrowserPageSession()
    }

    func reorderTab(fromIndex source: Int, toIndex destination: Int) {
        guard source >= 0, source < tabs.count,
              destination >= 0, destination <= tabs.count,
              source != destination else { return }

        let movedTab = tabs.remove(at: source)
        let insertIndex = destination > source ? destination - 1 : destination
        tabs.insert(movedTab, at: insertIndex)

        logger.debug("🔄 标签页重排: 从索引 \(source) 移动到 \(insertIndex)")
        persistBrowserPageSession()
    }

    // MARK: - Tab Group Management

    /// Create a new tab group
    @discardableResult
    func createGroup(name: String = "", color: TabGroupColor = .gray) -> TabGroup {
        let group = TabGroup(name: name, color: color)
        tabGroups.append(group)
        logger.debug("📁 创建新分组: \(group.id), 名称: \(name.isEmpty ? "(无名称)" : name)")
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
        logger.debug("📍 Tab 移动: \(tab.title) 从位置 \(fromIndex) 到 \(targetIndex)")
        persistBrowserPageSession()
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

        logger.debug("📍 Tab 移动: \(tab.title) 到位置 \(targetIndex), 分组: \(groupId?.uuidString ?? "无")")

        // Clean up old empty group if needed
        if let oldId = oldGroupId, oldId != groupId {
            cleanupEmptyGroup(oldId)
        }
        persistBrowserPageSession()
    }

    /// Insert a tab received from another window (cross-window drag)
    func insertTabFromCrossWindow(_ tab: Tab, at index: Int, groupId: UUID?) {
        var newTab = tab
        newTab.groupId = groupId

        let targetIndex = min(max(0, index), tabs.count)
        tabs.insert(newTab, at: targetIndex)
        tabSlotPositions[newTab.id] = tabs.count - 1
        currentTabId = newTab.id

        logger.notice(" 跨窗口插入 Tab: \(newTab.title) 到位置 \(targetIndex), 分组: \(groupId?.uuidString ?? "无")")
        persistBrowserPageSession()
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

        logger.debug("🗑️ 跨窗口移除 Tab: \(tab.title)")
        persistBrowserPageSession()
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
        logger.debug("📁 Tab \(tabId) 已加入分组 \(groupId)")
    }

    /// Remove a tab from its group
    func removeTabFromGroup(tabId: UUID) {
        guard let tabIndex = tabs.firstIndex(where: { $0.id == tabId }) else { return }
        let oldGroupId = tabs[tabIndex].groupId
        tabs[tabIndex].groupId = nil
        logger.debug("📁 Tab \(tabId) 已从分组 \(String(describing: oldGroupId)) 移除")

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
        logger.debug("🗑️ 分组 \(groupId) 已删除")
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
        logger.debug("🗑️ 分组 \(groupId) 及其所有 Tab 已关闭")
    }

    /// Restore a group from history
    func restoreGroup(from historyGroup: HistoryGroup) {
        // Create a new group with the same properties
        let newGroup = createGroup(name: historyGroup.name, color: historyGroup.color)

        // Restore each tab in the group
        for historyTab in historyGroup.tabs {
            if let appURL = historyTab.appURL {
                // Import the ACTR package
                importACTRPackageToGroup(from: appURL, groupId: newGroup.id)
            }
        }

        logger.debug("♻️ 已恢复分组: \(historyGroup.name.isEmpty ? "Unnamed" : historyGroup.name) 包含 \(historyGroup.tabs.count) 个 Tab")
    }

    /// Import ACTR package and add to a specific group
    private func importACTRPackageToGroup(from fileURL: URL, groupId: UUID) {
        let resolvedURL = fileURL.standardizedFileURL
        guard FileManager.default.fileExists(atPath: resolvedURL.path) else {
            logger.error(" 找不到 .actr 文件：\(resolvedURL.path)")
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
            // Archive mode - extract to temp
            let temp = FileManager.default.temporaryDirectory
                .appendingPathComponent("actr-\(UUID().uuidString)", isDirectory: true)
            do {
                try FileManager.default.createDirectory(at: temp, withIntermediateDirectories: true)
                try unzipACTR(archive: resolvedURL, to: temp)
            } catch {
                logger.error(" [restoreGroup] .actr 装载失败：\(error.localizedDescription)")
                return
            }
            rootDir = temp
            tempDir = temp
        }

        // Mark as read to silence "written to, but never read" warnings.
        if tempDir != nil { }

        do {
            let manifestInfo = try readManifestJSON(fromRoot: rootDir)
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
            logger.notice(" [restoreGroup] 已加载 .actr 到分组: \(tabTitle)")
            persistBrowserPageSession()

        } catch {
            logger.error(" [restoreGroup] .actr 装载失败：\(error.localizedDescription)")
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
        logger.debug("📝 分组 \(groupId) 已更新")
    }

    /// Toggle group collapsed state
    func toggleGroupCollapsed(groupId: UUID) {
        guard let index = tabGroups.firstIndex(where: { $0.id == groupId }) else { return }
        tabGroups[index].isCollapsed.toggle()
        logger.debug("📁 分组 \(groupId) 折叠状态: \(self.tabGroups[index].isCollapsed)")
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
            logger.debug("🧹 空分组 \(groupId) 已自动删除")
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

        guard let pos1 = tabSlotPositions[id1],
              let pos2 = tabSlotPositions[id2] else {
            logger.warning(" Cannot swap: slot position not found")
            return
        }

        tabSlotPositions[id1] = pos2
        tabSlotPositions[id2] = pos1
        logger.debug("🔄 Swapped slot positions: \(pos1) ↔ \(pos2)")
    }

    /// Get tabs sorted by their slot position
    func tabsSortedBySlotPosition() -> [(tab: Tab, position: Int)] {
        return tabs.compactMap { tab in
            guard let position = tabSlotPositions[tab.id] else { return nil }
            return (tab, position)
        }.sorted { $0.position < $1.position }
    }

    func updateCurrentTabTitle(_ title: String) {
        guard let currentTabId = currentTabId,
              let index = tabs.firstIndex(where: { $0.id == currentTabId }) else { return }
        tabs[index].title = title
        persistBrowserPageSession()
    }

    /// Update title for a specific tab
    func updateTabTitle(for tabId: UUID, title: String) {
        guard let index = tabs.firstIndex(where: { $0.id == tabId }) else { return }
        tabs[index].title = title
        persistBrowserPageSession()
    }

    /// Update URL for a specific tab (for web navigation)
    func updateTabURL(for tabId: UUID, url: URL) {
        guard let index = tabs.firstIndex(where: { $0.id == tabId }) else {
            logger.warning("⚠️ [updateTabURL] Tab not found: \(tabId)")
            return
        }
        let oldURL = tabs[index].url?.absoluteString ?? "nil"
        tabs[index].url = url
        logger.notice("🔗 [updateTabURL] Tab \(tabId) URL updated from \(oldURL) to \(url.absoluteString)")
        persistBrowserPageSession()
    }

    /// Update background color for a specific tab
    func updateTabBackgroundColor(for tabId: UUID, color: String) {
        guard let index = tabs.firstIndex(where: { $0.id == tabId }) else { return }
        // Only update if changed
        if tabs[index].backgroundColor != color {
            logger.notice("🎨 [BrowserViewModel] Updating background color for tab \(self.tabs[index].title): \(color)")
            tabs[index].backgroundColor = color
        }
    }

    /// 导航当前 Tab 到指定 URL 或应用 ID
    /// 统一的导航入口 - 处理所有类型的输入
    /// - 如果输入是 about: URL，导航到对应系统应用
    /// - 如果输入是 actro:// URL，加载 Actro 应用
    /// - 如果输入是 http/https URL，作为 Web 内容加载
    /// - 如果输入是本地文件路径，加载 ACTR 应用
    /// - 否则作为 app id 处理
    func navigateCurrentTab(to input: String) {
        guard let currentTabId = currentTabId,
              let index = tabs.firstIndex(where: { $0.id == currentTabId }) else {
            logger.warning("⚠️ [navigateCurrentTab] 无当前 tab")
            return
        }

        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)

        // 1. 检查是否为本地文件路径
        if isLocalFilePath(trimmed) {
            let fileURL = URL(fileURLWithPath: trimmed).standardizedFileURL
            var isDirectory: ObjCBool = false
            if FileManager.default.fileExists(atPath: fileURL.path, isDirectory: &isDirectory) {
                if isDirectory.boolValue {
                    logger.notice("📂 [navigateCurrentTab] 加载开发目录: \(fileURL.path)")
                    loadFromDevDirectory(fileURL)
                } else {
                    logger.notice("📦 [navigateCurrentTab] 加载 ACTR 包: \(fileURL.path)")
                    importACTRPackage(from: fileURL)
                }
                return
            } else {
                logger.warning("⚠️ [navigateCurrentTab] 文件不存在: \(fileURL.path)")
                return
            }
        }

        // 2. 检查是否为 URL
        if let url = parseURL(trimmed) {
            // Check if it's an about: URL for system apps
            if let systemType = SystemTabType.from(url: url) {
                logger.notice("📱 [navigateCurrentTab] 导航到系统应用: \(url.absoluteString)")
                // Update Tab to system app
                tabs[index].systemTabType = systemType
                tabs[index].url = url
                tabs[index].title = systemType.title
                // Clear web/RN content
                tabs[index].unifiedScript = nil
                tabs[index].leftPanelScript = nil
                tabs[index].rightPanelScript = nil
                tabs[index].guestScript = nil
                tabs[index].htmlContent = nil
                tabs[index].contractName = nil
                objectWillChange.send()
                persistBrowserPageSession()
                return
            }

            // Check if it's an actro:// URL for Actro apps.
            // Legacy app-scheme links are still accepted as aliases.
            // Two formats supported:
            // - actro://bundleId - lookup by bundleId in database
            // - actro:///absolute/path - local file path (three slashes)
            if ActroAppURLScheme.isSupported(url.scheme) {
                var sourcePath: String?

                // Check if it's a local path (actro:///path/to/app)
                // URL parsing: actro:///path -> host is nil/empty, path is /path
                if let host = url.host, !host.isEmpty {
                    // bundleId mode: actro://com.example.app - lookup in database
                    logger.notice("📱 [navigateCurrentTab] 导航到 Actro 应用 (bundleId): \(host)")
                    let appRepo = AppRepository()
                    if let app = appRepo.getByBundleId(host) {
                        sourcePath = app.sourcePath
                        appRepo.updateLastUsed(bundleId: host)
                    } else {
                        logger.warning("⚠️ [navigateCurrentTab] 未找到应用: \(host)")
                        return
                    }
                } else {
                    // local path mode: actro:///Users/leo/path/to/app
                    sourcePath = url.path
                    logger.notice("📂 [navigateCurrentTab] 导航到本地 Actro 应用: \(sourcePath ?? "")")
                }

                // Load ACTR app into current tab
                if let path = sourcePath {
                    loadACTRAppIntoCurrentTab(from: URL(fileURLWithPath: path), tabIndex: index)
                }
                return
            }

            // Handle ssh:// URL — session created in XPC service, not here
            if url.scheme == "ssh" {
                logger.notice("[navigateCurrentTab] SSH connection: \(url.absoluteString)")
                let sessionId = UUID().uuidString
                tabs[index].url = url
                tabs[index].sshSessionId = sessionId
                tabs[index].sftpSessionId = nil
                tabs[index].title = url.host ?? "SSH"
                tabs[index].unifiedScript = nil
                tabs[index].leftPanelScript = nil
                tabs[index].rightPanelScript = nil
                tabs[index].guestScript = nil
                tabs[index].systemTabType = nil
                tabs[index].contractName = nil
                objectWillChange.send()
                persistBrowserPageSession()
                return
            }

            // Handle sftp:// URL
            if url.scheme == "sftp" {
                logger.notice("[navigateCurrentTab] SFTP connection: \(url.absoluteString)")
                let sessionId = UUID().uuidString
                tabs[index].url = url
                tabs[index].sftpSessionId = sessionId
                tabs[index].sshSessionId = nil
                tabs[index].title = "SFTP: \(url.host ?? "")"
                tabs[index].unifiedScript = nil
                tabs[index].leftPanelScript = nil
                tabs[index].rightPanelScript = nil
                tabs[index].guestScript = nil
                tabs[index].systemTabType = nil
                tabs[index].contractName = nil
                objectWillChange.send()
                persistBrowserPageSession()
                return
            }

            // Handle ardp:// URL — remote desktop connection
            if url.scheme == "ardp" {
                logger.notice("[navigateCurrentTab] Remote desktop connection: \(url.absoluteString)")
                let sessionId = UUID().uuidString
                tabs[index].url = url
                tabs[index].remoteDesktopSessionId = sessionId
                tabs[index].sshSessionId = nil
                tabs[index].sftpSessionId = nil
                tabs[index].title = "Remote: \(url.host ?? url.absoluteString)"
                tabs[index].unifiedScript = nil
                tabs[index].leftPanelScript = nil
                tabs[index].rightPanelScript = nil
                tabs[index].guestScript = nil
                tabs[index].systemTabType = nil
                tabs[index].contractName = nil
                objectWillChange.send()
                persistBrowserPageSession()
                return
            }

            logger.notice("🌐 [navigateCurrentTab] 导航到 URL: \(url.absoluteString)")
            // 更新 Tab 为 Web 内容
            tabs[index].url = url
            tabs[index].title = url.host ?? url.absoluteString
            // 清除 RN 相关内容
            tabs[index].unifiedScript = nil
            tabs[index].leftPanelScript = nil
            tabs[index].rightPanelScript = nil
            tabs[index].guestScript = nil
            tabs[index].systemTabType = nil
            tabs[index].contractName = nil
            objectWillChange.send()
            persistBrowserPageSession()
        } else {
            // Try to load as app bundle ID directly
            logger.notice("📱 [navigateCurrentTab] 尝试作为 bundle ID 加载: \(trimmed)")
            let appRepo = AppRepository()
            if let app = appRepo.getByBundleId(trimmed) {
                appRepo.updateLastUsed(bundleId: trimmed)
                loadACTRAppIntoCurrentTab(from: URL(fileURLWithPath: app.sourcePath), tabIndex: index)
            } else {
                logger.warning("⚠️ [navigateCurrentTab] 未找到应用: \(trimmed)")
            }
        }
    }

    /// Load ACTR app into the current tab (in-place update, not creating new tab)
    /// Used by actro:// URL navigation to update current tab content
    private func loadACTRAppIntoCurrentTab(from fileURL: URL, tabIndex: Int) {
        let resolvedURL = fileURL.standardizedFileURL

        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: resolvedURL.path, isDirectory: &isDirectory) else {
            logger.error("❌ [loadACTRAppIntoCurrentTab] 路径不存在: \(resolvedURL.path)")
            return
        }

        let rootDir: URL
        if isDirectory.boolValue {
            rootDir = resolvedURL
        } else {
            // Archive mode - extract to temp
            let tempDir = FileManager.default.temporaryDirectory
                .appendingPathComponent("actr-\(UUID().uuidString)", isDirectory: true)
            do {
                try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
                try unzipACTR(archive: resolvedURL, to: tempDir)
            } catch {
                logger.error("❌ [loadACTRAppIntoCurrentTab] 解压失败: \(error.localizedDescription)")
                return
            }
            rootDir = tempDir
        }

        do {
            let manifestInfo = try readManifestJSON(fromRoot: rootDir)
            let tabTitle = manifestInfo.name ?? resolvedURL.deletingPathExtension().lastPathComponent

            // Register app if appId is provided.
            if let appId = manifestInfo.appId {
                let sourceType: AppSourceType = isDirectory.boolValue ? .localDev : .downloaded
                registerApp(
                    name: tabTitle,
                    bundleId: appId,
                    version: manifestInfo.version,
                    description: manifestInfo.description,
                    sourceType: sourceType,
                    sourcePath: resolvedURL.path,
                    icon: manifestInfo.icon
                )
            }

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

            // Construct actro:// URL for History and address bar
            let appURL: URL
            if let appId = manifestInfo.appId,
               let url = URL(string: "\(ActroAppURLScheme.primary)://\(appId)") {
                appURL = url
            } else {
                // Local dev path: actro:///absolute/path
                appURL = ActroAppURLScheme.appURL(filePath: resolvedURL.path) ?? resolvedURL
            }

            // Update current tab in-place (like about: URL handling)
            logger.notice("📱 [loadACTRAppIntoCurrentTab] 更新当前 Tab: \(tabTitle), url=\(appURL.absoluteString)")

            tabs[tabIndex].title = tabTitle
            tabs[tabIndex].url = appURL
            tabs[tabIndex].appId = manifestInfo.appId
            tabs[tabIndex].systemTabType = nil
            tabs[tabIndex].htmlContent = nil

            // Set scripts based on available content
            if let unified = unifiedScript {
                tabs[tabIndex].unifiedScript = unified
                tabs[tabIndex].leftPanelScript = nil
                tabs[tabIndex].rightPanelScript = nil
                tabs[tabIndex].guestScript = nil
            } else if leftPanelScript != nil || rightPanelScript != nil {
                tabs[tabIndex].unifiedScript = nil
                tabs[tabIndex].leftPanelScript = leftPanelScript
                tabs[tabIndex].rightPanelScript = rightPanelScript
                tabs[tabIndex].guestScript = nil
            } else if let script = guestScript {
                tabs[tabIndex].unifiedScript = nil
                tabs[tabIndex].leftPanelScript = nil
                tabs[tabIndex].rightPanelScript = nil
                tabs[tabIndex].guestScript = script
            }

            tabs[tabIndex].contractName = manifestInfo.contractName
            tabs[tabIndex].contractVersion = manifestInfo.contractVersion
            tabs[tabIndex].permissions = manifestInfo.permissions
            tabs[tabIndex].leftPanelDefaultVisible = manifestInfo.leftPanelDefaultVisible
            tabs[tabIndex].rightPanelDefaultVisible = manifestInfo.rightPanelDefaultVisible

            // Update panel visibility
            leftPanelVisibility[tabs[tabIndex].id] = manifestInfo.leftPanelDefaultVisible

            objectWillChange.send()
            persistBrowserPageSession()

            // Record History visit for ACTR app
            historyVM?.recordVisit(tab: tabs[tabIndex])

        } catch {
            logger.error("❌ [loadACTRAppIntoCurrentTab] 加载失败: \(error.localizedDescription)")
        }
    }

    /// 检查输入是否是本地文件路径
    private func isLocalFilePath(_ input: String) -> Bool {
        // 以 / 开头是绝对路径
        if input.hasPrefix("/") {
            return true
        }
        // 以 ~ 开头是用户目录
        if input.hasPrefix("~") {
            return true
        }
        // 以 file:// 开头
        if input.hasPrefix("file://") {
            return true
        }
        // 以 .actr 结尾
        if input.hasSuffix(".actr") {
            return true
        }
        return false
    }

    /// 解析 URL 字符串
    /// - 如果是 about:/http/https/actro，直接解析
    /// - 如果看起来像域名（包含 . 且不包含空格），添加 https:// 前缀
    func parseURL(_ input: String) -> URL? {
        // 已经是完整 URL (http/https/about/actro/ssh)
        if input.hasPrefix("http://") || input.hasPrefix("https://") ||
           input.hasPrefix("about:") ||
           ActroAppURLScheme.hasSupportedPrefix(input) ||
           input.hasPrefix("ssh://") ||
           input.hasPrefix("sftp://") ||
           input.hasPrefix("ardp://") {
            return URL(string: input)
        }

        // 看起来像域名（包含 . 且不包含空格）
        if input.contains(".") && !input.contains(" ") {
            return URL(string: "https://\(input)")
        }

        // Fallback: treat as search query using configured search engine
        return SearchEngine.current.searchURL(for: input)
    }

    func closeCurrentTab() {
        guard let id = currentTabId else { return }
        closeTab(id: id)
    }

    // MARK: - Web Navigation

    /// Check if current web tab can go back
    var canGoBack: Bool {
        guard let tabId = currentTabId else { return false }
        return WebViewHolderManager.shared.existingHolder(for: tabId)?.canGoBack ?? false
    }

    /// Check if current web tab can go forward
    var canGoForward: Bool {
        guard let tabId = currentTabId else { return false }
        return WebViewHolderManager.shared.existingHolder(for: tabId)?.canGoForward ?? false
    }

    /// Navigate back in web history
    func goBack() {
        guard let tabId = currentTabId else { return }
        WebViewHolderManager.shared.existingHolder(for: tabId)?.goBack()
    }

    /// Navigate forward in web history
    func goForward() {
        guard let tabId = currentTabId else { return }
        WebViewHolderManager.shared.existingHolder(for: tabId)?.goForward()
    }

    /// Reload the current tab (web page or .actr package)
    func reloadCurrentTab() {
        guard let id = currentTabId,
              let index = tabs.firstIndex(where: { $0.id == id }) else {
            logger.warning("⚠️ [reloadCurrentTab] 无法刷新：没有当前 tab")
            return
        }

        logger.notice("🔄 [reloadCurrentTab] 开始刷新 tab: \(self.tabs[index].title), reloadNonce: \(self.tabs[index].reloadNonce)")

        // 如果没有源 URL，只增加 reloadNonce 强制 RN 重新挂载
        guard let sourceURL = tabs[index].url else {
            logger.notice("🔄 [reloadCurrentTab] 无源 URL，仅强制 RN remount")
            tabs[index].reloadNonce &+= 1
            objectWillChange.send()
            return
        }

        // For web content (http/https), use WebView's reload
        if sourceURL.scheme == "http" || sourceURL.scheme == "https" {
            logger.notice("🔄 [reloadCurrentTab] Web 内容，调用 WebView reload")
            WebViewHolderManager.shared.existingHolder(for: id)?.reload()
            return
        }

        logger.debug("🔄 [reloadCurrentTab] 本地包，重新读取")

        // Re-read the .actr package
        let resolvedURL = sourceURL.standardizedFileURL
        guard FileManager.default.fileExists(atPath: resolvedURL.path) else {
            logger.error(" [reloadCurrentTab] 源文件不存在：\(resolvedURL.path)")
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
            // Archive mode - extract to temp
            let tempDir = FileManager.default.temporaryDirectory
                .appendingPathComponent("actr-\(UUID().uuidString)", isDirectory: true)
            do {
                try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
                try unzipACTR(archive: resolvedURL, to: tempDir)
            } catch {
                logger.error(" [reloadCurrentTab] 解压失败：\(error.localizedDescription)")
                return
            }
            rootDir = tempDir
        }

        do {
            let manifestInfo = try readManifestJSON(fromRoot: rootDir)

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

            logger.notice(" [reloadCurrentTab] 刷新完成: \(self.tabs[index].title)")

            // 增加刷新 nonce，提醒 RN 侧 remount
            tabs[index].reloadNonce &+= 1

            // Force UI update by triggering objectWillChange
            objectWillChange.send()

        } catch {
            logger.error(" [reloadCurrentTab] 刷新失败：\(error.localizedDescription)")
        }
    }

    /// Stop loading the current tab
    func stopCurrentTab() {
        guard let tabId = currentTabId else { return }
        WebViewHolderManager.shared.existingHolder(for: tabId)?.stopLoading()
    }

    // MARK: - Per-Tab Left Panel Visibility

    func isLeftPanelVisible(for tabId: UUID?) -> Bool {
        guard let id = tabId else { return true }
        let visible = leftPanelVisibility[id] ?? true
        logger.debug("🔍 [isLeftPanelVisible] Tab \(id.uuidString.prefix(8)): \(visible)")
        return visible
    }

    func toggleLeftPanelForCurrentTab() {
        // Focus 模式下使用 focus tabId，否则使用 currentTabId
        let id: UUID
        if case .focus(let focusTabId) = actroMode {
            id = focusTabId
        } else if let currentId = currentTabId {
            id = currentId
        } else {
            logger.warning(" [toggleLeftPanel] 无法确定当前 tab，无法切换")
            return
        }

        var visibility = leftPanelVisibility
        let current = visibility[id] ?? true
        visibility[id] = !current
        leftPanelVisibility = visibility  // 重新赋值整个字典，确保触发 @Published
        logger.debug("🔄 [toggleLeftPanel] Tab \(id.uuidString.prefix(8)) 左侧面板: \(current) -> \(!current)")
    }

    // MARK: - .actr 应用加载

    /// 从本地 .actr 包创建一个新标签并装载应用基础信息
    func importACTRPackage(from fileURL: URL) {
        let resolvedURL = fileURL.standardizedFileURL
        guard FileManager.default.fileExists(atPath: resolvedURL.path) else {
            logger.error(" 找不到 .actr 文件：\(resolvedURL.path)")
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
            // Archive mode - extract to temp
            let tempDir = FileManager.default.temporaryDirectory
                .appendingPathComponent("actr-\(UUID().uuidString)", isDirectory: true)
            do {
                try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
                try unzipACTR(archive: resolvedURL, to: tempDir)
            } catch {
                logger.error(" .actr 装载失败：\(error.localizedDescription)")
                return
            }
            rootDir = tempDir
        }

        do {
            let manifestInfo = try readManifestJSON(fromRoot: rootDir)
            let tabTitle = manifestInfo.name ?? resolvedURL.deletingPathExtension().lastPathComponent

            // Register as downloaded app if appId is provided.
            if let appId = manifestInfo.appId {
                registerApp(
                    name: tabTitle,
                    bundleId: appId,
                    version: manifestInfo.version,
                    description: manifestInfo.description,
                    sourceType: .downloaded,
                    sourcePath: resolvedURL.path,
                    icon: manifestInfo.icon
                )
            }

            // 读取 guest 脚本
            let guestScript = readGuestScript(fromRoot: rootDir)

            // v2.0: 优先读取 unified script (Single Engine 架构)
            var unifiedScript: String? = nil
            if let unifiedFileName = manifestInfo.unifiedScript {
                unifiedScript = readScript(fromRoot: rootDir, fileName: unifiedFileName)
                logger.notice(" [BrowserViewModel] 从 manifest.json 读取 unified: \(unifiedFileName), 脚本存在: \(unifiedScript != nil)")
            }

            // Legacy: 从 manifest.json 读取面板脚本文件名
            var leftPanelScript: String? = nil
            var rightPanelScript: String? = nil

            if let leftFileName = manifestInfo.leftPanelScript {
                leftPanelScript = readScript(fromRoot: rootDir, fileName: leftFileName)
                logger.debug("🔍 [BrowserViewModel] 从 manifest.json 读取 leftPanel: \(leftFileName), 脚本存在: \(leftPanelScript != nil)")
            }

            if let rightFileName = manifestInfo.rightPanelScript {
                rightPanelScript = readScript(fromRoot: rootDir, fileName: rightFileName)
                logger.debug("🔍 [BrowserViewModel] 从 manifest.json 读取 rightPanel: \(rightFileName), 脚本存在: \(rightPanelScript != nil)")
            }

            // v2.0: 优先使用 Single Engine 架构
            if let unified = unifiedScript {
                logger.debug("🚀 [BrowserViewModel] 使用 Single Engine 架构 (unified bundle)")
                appendACTRTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    appId: manifestInfo.appId,
                    unifiedScript: unified,
                    contractName: manifestInfo.contractName,
                    contractVersion: manifestInfo.contractVersion,
                    permissions: manifestInfo.permissions,
                    leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                    rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible,
                    themeColor: manifestInfo.themeColor
                )
            }
            // Legacy: 双 Engine 架构
            else if leftPanelScript != nil || rightPanelScript != nil {
                logger.debug("🔄 [BrowserViewModel] 使用 Legacy 双 Engine 架构")
                appendACTRTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    appId: manifestInfo.appId,
                    leftPanelScript: leftPanelScript,
                    rightPanelScript: rightPanelScript,
                    contractName: manifestInfo.contractName,
                    contractVersion: manifestInfo.contractVersion,
                    permissions: manifestInfo.permissions,
                    leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                    rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible,
                    themeColor: manifestInfo.themeColor
                )
            } else if let script = guestScript {
                // 旧架构：兼容单一的 guest.js
                logger.debug("🔄 [BrowserViewModel] 使用兼容模式 (guestScript)")
                appendACTRTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    appId: manifestInfo.appId,
                    guestScript: script,
                    contractName: manifestInfo.contractName,
                    contractVersion: manifestInfo.contractVersion,
                    permissions: manifestInfo.permissions,
                    themeColor: manifestInfo.themeColor
                )
            } else {
                // 没有脚本，显示应用信息页面
                let html = renderACTRInfoHTML(
                    name: manifestInfo.name ?? "Unknown App",
                    version: manifestInfo.version ?? "0.0.0",
                    description: manifestInfo.description ?? "No description provided.",
                    sourcePath: resolvedURL.path
                )
                appendACTRTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    appId: manifestInfo.appId,
                    htmlContent: html
                )
            }
        } catch {
            logger.error(" .actr 装载失败：\(error.localizedDescription)")
            let fallbackHTML = renderACTRInfoHTML(
                name: resolvedURL.deletingPathExtension().lastPathComponent,
                version: "unknown",
                description: "加载失败：\(error.localizedDescription)",
                sourcePath: resolvedURL.path
            )
            appendACTRTab(
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
            logger.error(" 找不到 .actr 文件：\(resolvedURL.path)")
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
            // Archive mode - extract to temp
            let tempDir = FileManager.default.temporaryDirectory
                .appendingPathComponent("actr-\(UUID().uuidString)", isDirectory: true)
            do {
                try FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
                try unzipACTR(archive: resolvedURL, to: tempDir)
            } catch {
                logger.error(" .actr 装载失败：\(error.localizedDescription)")
                return
            }
            rootDir = tempDir
        }

        do {
            let manifestInfo = try readManifestJSON(fromRoot: rootDir)
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
                    self?.appendACTRTab(
                        title: tabTitle,
                        sourceURL: resolvedURL,
                        appId: manifestInfo.appId,
                        unifiedScript: unified,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible,
                        themeColor: manifestInfo.themeColor
                    )
                } else if leftPanelScript != nil || rightPanelScript != nil {
                    self?.appendACTRTab(
                        title: tabTitle,
                        sourceURL: resolvedURL,
                        appId: manifestInfo.appId,
                        leftPanelScript: leftPanelScript,
                        rightPanelScript: rightPanelScript,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible,
                        themeColor: manifestInfo.themeColor
                    )
                } else if let script = guestScript {
                    self?.appendACTRTab(
                        title: tabTitle,
                        sourceURL: resolvedURL,
                        appId: manifestInfo.appId,
                        guestScript: script,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        themeColor: manifestInfo.themeColor
                    )
                } else {
                    let html = self?.renderACTRInfoHTML(
                        name: manifestInfo.name ?? "Unknown App",
                        version: manifestInfo.version ?? "0.0.0",
                        description: manifestInfo.description ?? "No description provided.",
                        sourcePath: resolvedURL.path
                    ) ?? ""
                    self?.appendACTRTab(
                        title: tabTitle,
                        sourceURL: resolvedURL,
                        appId: manifestInfo.appId,
                        htmlContent: html
                    )
                }
            }
        } catch {
            logger.error(" .actr 装载失败：\(error.localizedDescription)")
            DispatchQueue.main.async { [weak self] in
                let fallbackHTML = self?.renderACTRInfoHTML(
                    name: resolvedURL.deletingPathExtension().lastPathComponent,
                    version: "unknown",
                    description: "加载失败：\(error.localizedDescription)",
                    sourcePath: resolvedURL.path
                ) ?? ""
                self?.appendACTRTab(
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
            logger.error(" 找不到开发目录：\(directoryURL.path)")
            return
        }

        do {
            let manifestInfo = try readManifestJSON(fromRoot: directoryURL)
            let tabTitle = manifestInfo.name ?? directoryURL.lastPathComponent
            logger.debug("📂 [loadFromDevDirectory] 加载开发目录: \(tabTitle)")

            // Register as local dev app if appId is provided.
            if let appId = manifestInfo.appId {
                registerApp(
                    name: tabTitle,
                    bundleId: appId,
                    version: manifestInfo.version,
                    description: manifestInfo.description,
                    sourceType: .localDev,
                    sourcePath: directoryURL.path,
                    icon: manifestInfo.icon
                )
            }

            let guestScript = readGuestScript(fromRoot: directoryURL)

            var unifiedScript: String? = nil
            if let unifiedFileName = manifestInfo.unifiedScript {
                unifiedScript = readScript(fromRoot: directoryURL, fileName: unifiedFileName)
                logger.notice(" [loadFromDevDirectory] 读取 unified: \(unifiedFileName), 长度: \(unifiedScript?.count ?? 0)")
            }

            var leftPanelScript: String? = nil
            var rightPanelScript: String? = nil
            if let leftFileName = manifestInfo.leftPanelScript {
                leftPanelScript = readScript(fromRoot: directoryURL, fileName: leftFileName)
                logger.notice(" [loadFromDevDirectory] 读取 leftPanel: \(leftFileName), 长度: \(leftPanelScript?.count ?? 0)")
            }
            if let rightFileName = manifestInfo.rightPanelScript {
                rightPanelScript = readScript(fromRoot: directoryURL, fileName: rightFileName)
                logger.notice(" [loadFromDevDirectory] 读取 rightPanel: \(rightFileName), 长度: \(rightPanelScript?.count ?? 0)")
            }

            // Update UI on main thread
            DispatchQueue.main.async { [weak self] in
                // System app type takes priority (no scripts needed)
                if let systemType = manifestInfo.systemAppType {
                    logger.notice(" [loadFromDevDirectory] 系统应用: \(tabTitle) (type: \(systemType.rawValue))")
                    self?.appendSystemTab(
                        title: tabTitle,
                        systemType: systemType,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                    )
                } else if let unified = unifiedScript {
                    self?.appendACTRTab(
                        title: tabTitle,
                        sourceURL: directoryURL,
                        appId: manifestInfo.appId,
                        unifiedScript: unified,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible,
                        themeColor: manifestInfo.themeColor
                    )
                } else if leftPanelScript != nil || rightPanelScript != nil {
                    self?.appendACTRTab(
                        title: tabTitle,
                        sourceURL: directoryURL,
                        appId: manifestInfo.appId,
                        leftPanelScript: leftPanelScript,
                        rightPanelScript: rightPanelScript,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible,
                        themeColor: manifestInfo.themeColor
                    )
                } else if let script = guestScript {
                    self?.appendACTRTab(
                        title: tabTitle,
                        sourceURL: directoryURL,
                        appId: manifestInfo.appId,
                        guestScript: script,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        themeColor: manifestInfo.themeColor
                    )
                } else {
                    logger.warning(" [loadFromDevDirectory] 没有找到任何脚本文件")
                }
            }
        } catch {
            logger.error(" 开发目录加载失败：\(error.localizedDescription)")
        }
    }

    /// Register an Actro app in the database
    /// This allows the app to be opened via actro://bundleId URL
    private func registerApp(
        name: String,
        bundleId: String,
        version: String?,
        description: String?,
        sourceType: AppSourceType,
        sourcePath: String,
        icon: String?
    ) {
        let appRepo = AppRepository()

        // Check if already registered
        if let existing = appRepo.getByBundleId(bundleId) {
            // Update existing entry
            var updated = existing
            updated.sourcePath = sourcePath
            if let v = version { updated.version = v }
            if let d = description { updated.description = d }
            if let i = icon { updated.icon = i }
            appRepo.update(updated)
            logger.notice("📱 [registerApp] 更新应用: \(bundleId)")
        } else {
            // Create new entry
            let app = InstalledActroApp(
                name: name,
                bundleId: bundleId,
                version: version,
                description: description,
                sourceType: sourceType,
                sourcePath: sourcePath,
                icon: icon
            )
            appRepo.add(app)
            logger.notice("📱 [registerApp] 注册新应用: \(bundleId) -> \(sourcePath)")
        }
    }

    /// Append a system tab (debug, history, bookmarks)
    private func appendSystemTab(
        title: String,
        systemType: SystemTabType,
        leftPanelDefaultVisible: Bool = false,
        rightPanelDefaultVisible: Bool = false
    ) {
        let newTab = Tab(
            title: title,
            systemTabType: systemType,
            leftPanelDefaultVisible: leftPanelDefaultVisible,
            rightPanelDefaultVisible: rightPanelDefaultVisible
        )
        tabs.append(newTab)
        tabSlotPositions[newTab.id] = tabs.count - 1
        leftPanelVisibility[newTab.id] = leftPanelDefaultVisible
        logger.notice(" 已加载系统应用：\(title)")
        persistBrowserPageSession()
    }

    private func appendACTRTab(
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
        leftPanelDefaultVisible: Bool = true,
        rightPanelDefaultVisible: Bool = false,
        themeColor: String? = nil
    ) {
        // Use actro:// URL for History and address bar
        let appURL = ActroAppURLScheme.appURL(filePath: sourceURL.path) ?? sourceURL
        let newTab = Tab(
            title: title,
            url: appURL,
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
            rightPanelDefaultVisible: rightPanelDefaultVisible,
            backgroundColor: themeColor
        )
        logger.debug("📝 [appendACTRTab] 创建新 Tab: \(title), ID=\(newTab.id.uuidString.prefix(8)), bg=\(newTab.backgroundColor)")
        tabs.append(newTab)
        currentTabId = newTab.id
        tabSlotPositions[newTab.id] = tabs.count - 1
        leftPanelVisibility[newTab.id] = leftPanelDefaultVisible

        logger.notice(" 已加载 .actr：\(title)")

        if let unified = unifiedScript {
            logger.debug("  ├─ 统一脚本 (Single Engine): \(unified.count) 字节")
            logger.debug("  ├─ 左侧面板默认\(leftPanelDefaultVisible ? "显示" : "隐藏")")
            logger.debug("  └─ 右侧面板默认\(rightPanelDefaultVisible ? "显示" : "隐藏")")
        } else if let leftScript = leftPanelScript {
            logger.debug("  ├─ 左侧面板脚本 (Legacy): \(leftScript.count) 字节")
            if let rightScript = rightPanelScript {
                logger.debug("  ├─ 右侧扩展面板脚本 (Legacy): \(rightScript.count) 字节")
                logger.debug("  └─ 右侧面板默认\(rightPanelDefaultVisible ? "显示" : "隐藏")")
            }
        }

        // Record History visit for ACTR app
        historyVM?.recordVisit(tab: newTab)
        persistBrowserPageSession()
    }

    private func unzipACTR(archive: URL, to destination: URL) throws {
        // Use /usr/bin/ditto for more reliable unzipping on macOS
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
        process.arguments = ["-xk", archive.path, destination.path]

        let pipe = Pipe()
        process.standardOutput = pipe
        process.standardError = pipe

        try process.run()
        process.waitUntilExit()

        if process.terminationStatus != 0 {
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            let output = String(data: data, encoding: .utf8) ?? ""
            throw NSError(
                domain: "ACTRUnzipError",
                code: Int(process.terminationStatus),
                userInfo: [NSLocalizedDescriptionKey: "解压失败: \(output)"]
            )
        }
    }

    private func readManifestJSON(fromRoot root: URL) throws -> ActrPackageManifestInfo {
        try ActrPackageManifestReader.read(fromRoot: root)
    }

    /// 通用脚本读取方法
    private func readScript(fromRoot root: URL, fileName: String) -> String? {
        let fm = FileManager.default

        // 如果 fileName 包含路径（如 "dist/unified-app.js"），直接拼接读取
        if fileName.contains("/") {
            let fullPath = root.appendingPathComponent(fileName)
            if let script = try? String(contentsOf: fullPath, encoding: .utf8) {
                logger.notice(" 找到 \(fileName): \(fullPath.path)")
                return script
            }
        }

        // 首先在根目录查找
        if let direct = try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: nil).first(where: { $0.lastPathComponent == fileName }) {
            if let script = try? String(contentsOf: direct, encoding: .utf8) {
                logger.notice(" 找到 \(fileName): \(direct.path)")
                return script
            }
        }

        // 如果根目录没有，递归查找
        if let enumerator = fm.enumerator(at: root, includingPropertiesForKeys: nil) {
            for case let fileURL as URL in enumerator {
                if fileURL.lastPathComponent == fileName {
                    if let script = try? String(contentsOf: fileURL, encoding: .utf8) {
                        logger.notice(" 找到 \(fileName): \(fileURL.path)")
                        return script
                    }
                }
            }
        }

        return nil
    }

    /// 读取 guest.js (向后兼容)
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
