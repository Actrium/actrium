import Foundation
import UIKit
import Combine
import ZIPFoundation

private let logger = AppLogger.viewModel

/// Canvas display mode - stored in ViewModel to survive view recreation
enum CanvasMode: Equatable {
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
    // 默认所有标签页左侧面板可见；仅在用户切换时记录覆盖值
    @Published private var leftPanelVisibility: [UUID: Bool] = [:]

    // MARK: - Persistence
    private let tabsKey = "persistedTabs"
    private var cancellables = Set<AnyCancellable>()

    /// 持久化防抖：属性变更（标题、红点、收藏等）高频触发时，合并为延迟保存
    private var saveTabsWorkItem: DispatchWorkItem?
    
    /// Current fullscreen tab ID (for iPhone only, excluding system tabs)
    @Published var fullscreenTabId: UUID?

    /// 标记下次 onDisappear 截图是否应跳过（RN 退出全屏时已提前截图）
    var skipNextDisappearSnapshot = false
    
    // MARK: - System Tabs (单独管理，不混入 tabs 数组)
    @Published var systemTabs: [SystemTabType: Tab] = [:]

    // MARK: - System Tab Management
    @Published var currentSystemTabType: SystemTabType? = nil
    /// 记录打开全局搜索前的系统标签类型，关闭搜索时恢复
    var previousSystemTabType: SystemTabType? = nil

    // MARK: - Tab Groups
    @Published var tabGroups: [TabGroup] = []

    /// Canvas display mode (Overview vs Focus)
    /// Stored here to survive view recreation when DevTools layout changes
    @Published var canvasMode: CanvasMode = .overview

    // MARK: - Grid Slot Position Management

    /// Tab ID → grid slot index mapping
    @Published var tabSlotPositions: [UUID: Int] = [:]

    /// Tab snapshots for overview mode
    @Published var tabSnapshots: [UUID: UIImage] = [:]

    /// Toolbar 实际高度（从视图测量得到）
    @Published var toolbarHeight: CGFloat = 52 // 默认值，会在视图中更新

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
            logger.notice("📦 [BrowserViewModel] 已加载 \(self.tabs.count) 个持久化标签页")
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
            // 🟢 Trick: 给一个空脚本，强制 SwiftUI 认为这是一个 RN 标签页
            // 这样即使 content 是 HTML，外面也会包裹住 ActrApp 的 RN 壳子（地址栏等）
            unifiedScript: "/* welcome page shell */"
        )
        tabs.append(defaultTab)
        currentTabId = defaultTab.id

        // Initialize slot positions
        initializeSlotPositions()
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
                logger.error("❌ [BrowserViewModel] 无法创建 images 目录: \(error)")
            }
        }
        return imagesURL
    }

    /// Get the file URL for a tab snapshot
    private func snapshotURL(for tabId: UUID) -> URL {
        return imagesDirectoryURL.appendingPathComponent("\(tabId.uuidString).jpg")
    }

    /// Save a tab snapshot to disk — 缩略图使用较低压缩质量以减少磁盘占用
    private func saveSnapshot(_ image: UIImage, for tabId: UUID) {
        let fileURL = snapshotURL(for: tabId)
        // 缩略图尺寸小，0.5 的压缩质量视觉上无明显差异，但体积更小
        if let jpegData = image.jpegData(compressionQuality: 0.5) {
            do {
                try jpegData.write(to: fileURL)
                logger.debug("💾 [BrowserViewModel] 已保存截图(降采样): \(tabId.uuidString.prefix(8)), 大小: \(jpegData.count) bytes")
            } catch {
                logger.error("❌ [BrowserViewModel] 保存截图失败: \(error)")
            }
        }
    }

    /// Load a tab snapshot from disk — 加载后降采样，确保内存副本始终为缩略图
    private func loadSnapshot(for tabId: UUID) -> UIImage? {
        let fileURL = snapshotURL(for: tabId)
        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            return nil
        }
        do {
            let data = try Data(contentsOf: fileURL)
            if let image = UIImage(data: data) {
                // 磁盘可能保存了旧版全分辨率截图，降采样后返回
                // 保持宽高比降采样，避免拉伸
                return image.downsampled(toWidth: Self.snapshotTargetWidth)
            }
            return nil
        } catch {
            logger.error("❌ [BrowserViewModel] 加载截图失败: \(error)")
            return nil
        }
    }

    /// 按需加载快照：内存中无快照时从磁盘读取。
    /// 内存警告回收 tabSnapshots 后，卡片视图 onAppear 调用此方法恢复缩略图。
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
                logger.debug("🗑️ [BrowserViewModel] 已删除截图: \(tabId.uuidString.prefix(8))")
            } catch {
                logger.error("❌ [BrowserViewModel] 删除截图失败: \(error)")
            }
        }
    }

    /// Load persisted tabs from UserDefaults and create full Tab objects
    /// - Returns: true if tabs were loaded, false if no persisted data or error
    private func loadPersistedTabs() -> Bool {
        guard let data = UserDefaults.standard.data(forKey: tabsKey) else {
            logger.debug("📭 [BrowserViewModel] 没有找到持久化的标签页数据")
            return false
        }

        do {
            let persistedTabs = try JSONDecoder().decode([PersistedTab].self, from: data)
            guard !persistedTabs.isEmpty else {
                logger.debug("📭 [BrowserViewModel] 持久化的标签页数据为空")
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

            logger.notice("✅ [BrowserViewModel] 成功加载 \(self.tabs.count) 个持久化标签页")
            return true
        } catch {
            logger.error("❌ [BrowserViewModel] 加载持久化标签页失败: \(error)")
            return false
        }
    }
    
    /// 待加入列表的新标签页（用于"先弹出 sheet，后显示在列表"的交互）
    var pendingNewTab: Tab?

    @discardableResult
    func addNewTab(focus: Bool = true, deferAppend: Bool = false) -> Tab {
        logger.debug("🆕 开始创建新标签页...")

        // 创建默认 RN 标签页（与应用启动时相同）
        let newTab = Tab(
            title: TM.active.blackTab(),
            htmlContent: """
                <!DOCTYPE html>
                <html>
                ...
                </html>
                """,
            // 🟢 Trick: 给一个空脚本，强制 SwiftUI 认为这是一个 RN 标签页
            // 这样即使 content 是 HTML，外面也会包裹住 ActrApp 的 RN 壳子（地址栏等）
            unifiedScript: "/* welcome page shell */"
        )

        if deferAppend {
            // 延迟加入列表：先只记录 pending，等 sheet 弹出后再 append
            pendingNewTab = newTab
            currentTabId = newTab.id
        } else {
            tabs.append(newTab)
            tabSlotPositions[newTab.id] = tabs.count - 1
            currentTabId = newTab.id
        }

        // 根据调用者意图决定是否进入 Focus 模式（某些 UI 场景希望保持 Overview）
        if focus {
            canvasMode = .focus(tabId: newTab.id)
        }

        logger.notice(" 新标签页已创建: \(newTab.title), 位置: \(deferAppend ? "pending" : "\(self.tabs.count - 1)")")

        // 发送通知到 RN
        var userInfo: [String: Any] = [
            "tabId": newTab.id.uuidString,
            "title": newTab.title
        ]
        if let icon = newTab.icon {
            userInfo["icon"] = icon
        }
        if let appUrl = newTab.url?.absoluteString {
            userInfo["appUrl"] = appUrl
        }
        NotificationCenter.default.post(
            name: .tabOpenedFromNative,
            object: nil,
            userInfo: userInfo
        )

        if !deferAppend {
            saveTabs()
        }
        return newTab
    }

    /// 将 pending 的新标签页正式加入列表
    func commitPendingNewTab() {
        guard let pending = pendingNewTab else { return }
        tabs.append(pending)
        tabSlotPositions[pending.id] = tabs.count - 1
        pendingNewTab = nil
        saveTabs()
    }

    /// Save tab snapshot when set — 降采样后再存储，大幅减少内存占用
    func setTabSnapshot(_ image: UIImage, for tabId: UUID) {
        // 卡片预览只需要小缩略图，按宽度降采样，保持原始宽高比
        let thumbnail = image.downsampled(toWidth: Self.snapshotTargetWidth)
        tabSnapshots[tabId] = thumbnail
        // 磁盘副本也保存降采样版本，减少 I/O 体积
        saveSnapshot(thumbnail, for: tabId)
    }

    /// 快照降采样目标宽度（@1x pt）
    /// iPhone 卡片宽约 344pt，iPad 卡片更宽。
    /// 400pt @1x 在两个平台上清晰度足够，单张内存约 50KB。
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
        // 中文注释：login 在 iPad 端需要纳入普通 tabs 体系，
        // 现有的 TabBar / currentTab / Focus 模式都只围绕 tabs 工作。
        // 如果继续放在 systemTabs，登录页签虽然被创建，但宿主无法正确聚焦和展示。
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

                // 中文注释：统一走 switchToTab，复用宿主已有的页签切换通知和状态同步链路。
                switchToTab(id: existingTab.id)
                logger.debug("🔄 返回已有登录标签: \(type.title)")
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
            logger.notice(" 已创建登录标签: \(type.title)")

            var userInfo: [String: Any] = [
                "tabId": newTab.id.uuidString,
                "title": newTab.title,
                "systemTabType": type.rawValue
            ]
            if let icon = newTab.icon {
                userInfo["icon"] = icon
            }
            if let appUrl = newTab.url?.absoluteString {
                userInfo["appUrl"] = appUrl
            }
            NotificationCenter.default.post(
                name: .tabOpenedFromNative,
                object: nil,
                userInfo: userInfo
            )
            // 中文注释：新建完成后立即切换，保证 iPad 端选中态、Focus 模式与 RN 侧事件保持一致。
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
                // 通过 reloadNonce 驱动 RN remount，确保深链参数生效
                existingTab.reloadNonce &+= 1
                systemTabs[type] = existingTab
            }
            logger.debug("🔄 返回已有系统标签: \(type.title)")
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
        logger.notice(" 已创建系统标签: \(type.title)")
        
        // 发送通知到 RN
        var userInfo: [String: Any] = [
            "tabId": newTab.id.uuidString,
            "title": newTab.title,
            "systemTabType": type.rawValue
        ]
        if let icon = newTab.icon {
            userInfo["icon"] = icon
        }
        if let appUrl = newTab.url?.absoluteString {
            userInfo["appUrl"] = appUrl
        }
        NotificationCenter.default.post(
            name: .tabOpenedFromNative,
            object: nil,
            userInfo: userInfo
        )
        
        return newTab
    }
    
    /// Reload system tab by incrementing reloadNonce
    func reloadSystemTab(_ type: SystemTabType) {
        guard var tab = systemTabs[type] else {
            logger.warning("⚠️ [reloadSystemTab] 系统标签不存在: \(type.title)")
            return
        }
        
        logger.notice("🔄 [reloadSystemTab] 刷新系统标签: \(type.title), reloadNonce: \(tab.reloadNonce) -> \(tab.reloadNonce + 1)")
        tab.reloadNonce &+= 1
        systemTabs[type] = tab
        objectWillChange.send()
    }

    func switchToTab(id: UUID) {
        let previousTabId = currentTabId
        let targetTab = tabs.first(where: { $0.id == id })
        
        if let tab = targetTab {
            logger.debug("🔄 切换到标签页: \(tab.title)")
        }
        currentTabId = id
        // Ensure canvas is in focus mode when switching tabs
        canvasMode = .focus(tabId: id)
        
        // 发送通知到 RN
        var userInfo: [String: Any] = ["tabId": id.uuidString]
        if let prevId = previousTabId {
            userInfo["previousTabId"] = prevId.uuidString
        }
        NotificationCenter.default.post(
            name: .tabSwitchedFromNative,
            object: nil,
            userInfo: userInfo
        )
    }
    
    func closeTab(id: UUID) {
        // 清理缓存的 View
        CanvasReactNativeHost.shared.removeViewForTab(tabId: id)

        // 清理快照
        tabSnapshots.removeValue(forKey: id)
        deleteSnapshot(for: id)
        
        // Get the slot position of the tab being closed
        guard let closedPosition = tabSlotPositions[id] else {
            logger.warning(" Cannot find tab slot position")
            return
        }

        // 如果这是最后一个标签页
        if tabs.count == 1 {
            logger.debug("📱 关闭最后一个标签页")
            currentTabId = nil
        }

        // 如果关闭的是当前标签，切换到相邻标签
        if currentTabId == id {
            // 手机端不切换到相邻标签
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
                    // 如果处于 Focus 模式，则关闭当前 tab 后自动切换到相邻 tab 的 Focus 模式
                    if canvasMode.isInFocusMode {
                        switchToTab(id: nextId)
                    } else {
                        currentTabId = nextId
                    }
                }
            }
        }

        // 删除标签页
        tabs.removeAll(where: { $0.id == id })
        saveTabs()

        // 删除该标签的位置映射
        tabSlotPositions.removeValue(forKey: id)

        // 重新整理位置：将所有大于被删除位置的位置减1
        for tabId in tabs.map({ $0.id }) {
            if let position = tabSlotPositions[tabId], position > closedPosition {
                tabSlotPositions[tabId] = position - 1
            }
        }

        logger.debug("🗑️ 关闭标签页，位置 \(closedPosition)，剩余 \(self.tabs.count) 个标签页")
        
        // 发送通知到 RN
        NotificationCenter.default.post(
            name: .tabClosedFromNative,
            object: nil,
            userInfo: ["tabId": id.uuidString]
        )
        
        if tabs.isEmpty, UIDevice.current.userInterfaceIdiom != .phone {
            logger.debug("🚪 关闭最后一个标签页，切换到 OverView 模式")
            canvasMode =  .overview
        }
    }

    func reorderTab(fromIndex source: Int, toIndex destination: Int) {
        guard source >= 0, source < tabs.count,
              destination >= 0, destination <= tabs.count,
              source != destination else { return }

        let movedTab = tabs.remove(at: source)
        let insertIndex = destination > source ? destination - 1 : destination
        tabs.insert(movedTab, at: insertIndex)

        logger.debug("🔄 标签页重排: 从索引 \(source) 移动到 \(insertIndex)")
        
        // 发送通知到 RN
        let orderedTabIds = tabs.map { $0.id.uuidString }
        NotificationCenter.default.post(
            name: .tabsReorderedFromNative,
            object: nil,
            userInfo: ["tabIds": orderedTabIds]
        )

        saveTabs()
    }
    
    /// Save tabs to UserDefaults (only essential fields) — 立即保存
    /// 仅用于结构性变更（增删 Tab、重排等），属性变更请使用 scheduleSaveTabs()
    func saveTabs() {
        // 取消待执行的防抖任务，避免重复保存
        saveTabsWorkItem?.cancel()
        saveTabsWorkItem = nil

        let persistedTabs = tabs.map { PersistedTab(from: $0) }
        do {
            let data = try JSONEncoder().encode(persistedTabs)
            UserDefaults.standard.set(data, forKey: tabsKey)

            logger.debug("💾 [BrowserViewModel] 已保存 \(persistedTabs.count) 个标签页")
        } catch {
            logger.error("❌ [BrowserViewModel] 保存标签页失败: \(error)")
        }
    }

    /// 防抖保存：2 秒内多次调用只执行最后一次。
    /// 适用于高频属性变更（标题、红点、收藏等），避免每次修改都触发磁盘 I/O。
    func scheduleSaveTabs() {
        saveTabsWorkItem?.cancel()
        let workItem = DispatchWorkItem { [weak self] in
            self?.saveTabs()
        }
        saveTabsWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + 2.0, execute: workItem)
    }

    // MARK: - Unread Status

    /// 清除指定 tab 的未读红点状态
    func clearUnread(for tabId: UUID) {
        guard let index = tabs.firstIndex(where: { $0.id == tabId }),
              tabs[index].hasUnreadMessage else { return }
        tabs[index].hasUnreadMessage = false
        scheduleSaveTabs()
    }

    // MARK: - Memory Pressure Handling

    /// 系统内存警告时自动回收资源：
    /// 1. 释放所有非活跃 Tab 的 RootView（最大的内存消耗项，每个约 40MB）
    /// 2. 释放快照内存副本（磁盘副本保留，标签列表滚动时按需从磁盘加载）
    func handleMemoryWarning() {
        let activeTabId = currentTabId
        var releasedCount = 0

        // 1. 释放所有非活跃 Tab 的 RootView
        for tab in tabs where tab.id != activeTabId {
            CanvasReactNativeHost.shared.removeViewForTab(tabId: tab.id)
            releasedCount += 1
        }

        // 2. 释放快照内存副本（磁盘副本保留，按需加载）
        let snapshotCount = tabSnapshots.count
        tabSnapshots.removeAll()

        // 立即持久化，确保数据安全
        saveTabs()

        logger.warning("⚠️ [BrowserViewModel] 内存警告回收：释放 \(releasedCount) 个后台 RootView，清除 \(snapshotCount) 个内存快照")
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

        guard let index1 = tabs.firstIndex(where: { $0.id == id1 }),
              let index2 = tabs.firstIndex(where: { $0.id == id2 }) else {
            logger.warning(" Cannot swap: tab not found in tabs array")
            return
        }

        // 交换 tabs 数组中元素的位置
        tabs.swapAt(index1, index2)

        // 同步更新 slot positions
        if let pos1 = tabSlotPositions[id1], let pos2 = tabSlotPositions[id2] {
            tabSlotPositions[id1] = pos2
            tabSlotPositions[id2] = pos1
        } else {
            // slot positions 缺失时重新初始化
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

        // 发送通知到 RN
        var userInfo: [String: Any] = ["tabId": currentTabId.uuidString]
        if let title = title {
            userInfo["title"] = title
        }
        if let icon = icon {
            userInfo["icon"] = icon
        }
        if let appUrl = appUrl?.absoluteString {
            userInfo["appUrl"] = appUrl
        }
        NotificationCenter.default.post(
            name: .tabUpdatedFromNative,
            object: nil,
            userInfo: userInfo
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

        logger.debug("🔄 [reloadCurrentTab] 有源 URL，重新读取包")

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

    // MARK: - Per-Tab Left Panel Visibility

    func isLeftPanelVisible(for tabId: UUID?) -> Bool {
        guard let id = tabId else { return true }
        return leftPanelVisibility[id] ?? false
    }

    func toggleLeftPanelForCurrentTab() {
        // Focus 模式下使用 focus tabId，否则使用 currentTabId
        let id: UUID
        if case .focus(let focusTabId) = canvasMode {
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
                    unifiedScript: unified,
                    contractName: manifestInfo.contractName,
                    contractVersion: manifestInfo.contractVersion,
                    permissions: manifestInfo.permissions,
                    leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                    rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                )
            }
            // Legacy: 双 Engine 架构
            else if leftPanelScript != nil || rightPanelScript != nil {
                logger.debug("🔄 [BrowserViewModel] 使用 Legacy 双 Engine 架构")
                appendACTRTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    leftPanelScript: leftPanelScript,
                    rightPanelScript: rightPanelScript,
                    contractName: manifestInfo.contractName,
                    contractVersion: manifestInfo.contractVersion,
                    permissions: manifestInfo.permissions,
                    leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                    rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                )
            } else if let script = guestScript {
                // 旧架构：兼容单一的 guest.js
                logger.debug("🔄 [BrowserViewModel] 使用兼容模式 (guestScript)")
                appendACTRTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    guestScript: script,
                    contractName: manifestInfo.contractName,
                    contractVersion: manifestInfo.contractVersion,
                    permissions: manifestInfo.permissions
                )
            } else {
                // 没有脚本，显示应用信息页面
                let html = renderACTRInfoHTML(
                    name: manifestInfo.name ?? "Unknown App",
                    version: manifestInfo.version ?? "0.0.0",
                    description: manifestInfo.description ?? "No description provided.",
                    sourcePath: resolvedURL.path
                )
                appendACTRTab(title: tabTitle, sourceURL: resolvedURL, htmlContent: html)
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
                        unifiedScript: unified,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                    )
                } else if leftPanelScript != nil || rightPanelScript != nil {
                    self?.appendACTRTab(
                        title: tabTitle,
                        sourceURL: resolvedURL,
                        leftPanelScript: leftPanelScript,
                        rightPanelScript: rightPanelScript,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                    )
                } else if let script = guestScript {
                    self?.appendACTRTab(
                        title: tabTitle,
                        sourceURL: resolvedURL,
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
                    self?.appendACTRTab(title: tabTitle, sourceURL: resolvedURL, htmlContent: html)
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
                if let unified = unifiedScript {
                    self?.appendACTRTab(
                        title: tabTitle,
                        sourceURL: directoryURL,
                        unifiedScript: unified,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                    )
                } else if leftPanelScript != nil || rightPanelScript != nil {
                    self?.appendACTRTab(
                        title: tabTitle,
                        sourceURL: directoryURL,
                        leftPanelScript: leftPanelScript,
                        rightPanelScript: rightPanelScript,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                    )
                } else if let script = guestScript {
                    self?.appendACTRTab(
                        title: tabTitle,
                        sourceURL: directoryURL,
                        guestScript: script,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions
                    )
                } else {
                    logger.warning(" [loadFromDevDirectory] 没有找到任何脚本文件")
                }
            }
        } catch {
            logger.error(" 开发目录加载失败：\(error.localizedDescription)")
        }
    }

    private func appendACTRTab(
        title: String,
        sourceURL: URL,
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
        
        // 自动进入 Focus 模式
        canvasMode = .focus(tabId: newTab.id)

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
        
        // 发送通知到 RN
        var userInfo: [String: Any] = [
            "tabId": newTab.id.uuidString,
            "title": title
        ]
        if let icon = newTab.icon {
            userInfo["icon"] = icon
        }
        if let appUrl = newTab.url?.absoluteString {
            userInfo["appUrl"] = appUrl
        }
        NotificationCenter.default.post(
            name: .tabOpenedFromNative,
            object: nil,
            userInfo: userInfo
        )
    }

    private func unzipACTR(archive: URL, to destination: URL) throws {
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

    private func readManifestJSON(fromRoot root: URL) throws -> (name: String?, version: String?, description: String?, contractName: String?, contractVersion: Int?, permissions: [String], leftPanelDefaultVisible: Bool, rightPanelDefaultVisible: Bool, leftPanelScript: String?, rightPanelScript: String?, unifiedScript: String?) {
        guard let manifestURL = findManifestJSON(in: root) else {
            throw NSError(
                domain: "ACTRPackage",
                code: 1,
                userInfo: [NSLocalizedDescriptionKey: "包内未找到 manifest.json"]
            )
        }

        let data = try Data(contentsOf: manifestURL)
        let json = try JSONSerialization.jsonObject(with: data, options: []) as? [String: Any]
        let name = json?["name"] as? String
        let version = json?["version"] as? String
        let description = json?["description"] as? String

        // MARK: - Contracts / Permissions (Phase 1: gating + logging)
        let supportedContractName = "ask"
        let supportedContractVersion = 1
        var contractNameValue: String? = nil
        var contractVersionValue: Int? = nil
        var permissionsValue: [String] = []

        if let contract = json?["contract"] as? [String: Any] {
            let contractName = contract["name"] as? String
            let contractVersion: Int? = {
                if let v = contract["version"] as? Int { return v }
                if let v = contract["version"] as? Double { return Int(v) }
                return nil
            }()

            if let cn = contractName, let cv = contractVersion {
                if cn != supportedContractName {
                    throw NSError(
                        domain: "ACTRContract",
                        code: 1,
                        userInfo: [NSLocalizedDescriptionKey: "不支持的 contract：\(cn)（当前仅支持 \(supportedContractName)）"]
                    )
                }
                if cv > supportedContractVersion {
                    throw NSError(
                        domain: "ACTRContract",
                        code: 2,
                        userInfo: [NSLocalizedDescriptionKey: "contractVersion 太新：\(cn)@v\(cv)（当前宿主仅支持到 v\(supportedContractVersion)）"]
                    )
                }
                contractNameValue = cn
                contractVersionValue = cv
                logger.notice(" [BrowserViewModel] contract: \(cn)@v\(cv)")
            } else {
                logger.warning(" [BrowserViewModel] manifest.contract 字段不完整，跳过校验")
            }
        } else {
            logger.warning(" [BrowserViewModel] manifest 缺少 contract（兼容模式继续运行）")
        }

        if let permissions = json?["permissions"] as? [String] {
            permissionsValue = permissions
            if !permissions.isEmpty {
                logger.debug("🔐 [BrowserViewModel] permissions: \(permissions.joined(separator: ", "))")
            }
        }

        // v2.1: 简化格式 - 顶级字段优先
        let unifiedScript = json?["entry"] as? String
        let leftPanelDefaultVisible = json?["leftPanelDefaultVisible"] as? Bool ?? true
        let rightPanelDefaultVisible = json?["rightPanelDefaultVisible"] as? Bool ?? false

        // Legacy: layout 嵌套格式 (向后兼容)
        let layout = json?["layout"] as? [String: Any]
        let leftPanelScript = layout?["leftPanel"] as? String
        let rightPanelScript = layout?["rightPanel"] as? String

        return (name, version, description, contractNameValue, contractVersionValue, permissionsValue, leftPanelDefaultVisible, rightPanelDefaultVisible, leftPanelScript, rightPanelScript, unifiedScript)
    }

    private func findManifestJSON(in root: URL) -> URL? {
        let fm = FileManager.default
        if let direct = try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: nil).first(where: { $0.lastPathComponent == "manifest.json" }) {
            return direct
        }

        if let enumerator = fm.enumerator(at: root, includingPropertiesForKeys: nil) {
            for case let fileURL as URL in enumerator {
                if fileURL.lastPathComponent == "manifest.json" {
                    return fileURL
                }
            }
        }
        return nil
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
