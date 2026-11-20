import Foundation
import AppKit // Import AppKit for NSImage
import Combine
import CryptoKit
import UniformTypeIdentifiers
import ActrBindings
import ActriumCore  // AppleActrPackageSignatureError (15-B B8 error pages)

private let logger = AppLogger.viewModel

/// Actrium display mode - stored in ViewModel to survive view recreation
enum ActriumMode: Equatable {
    case overview
    case focus(tabId: UUID)

    var isInFocusMode: Bool {
        if case .focus = self { return true }
        return false
    }
}

/// Protocol for objects that publish actriumMode changes
/// Used by ViewBridgeCoordinatorManager to subscribe to mode changes
protocol ActriumModePublisher: AnyObject {
    var actriumMode: ActriumMode { get }
    var actriumModePublisher: AnyPublisher<ActriumMode, Never> { get }
}

@MainActor
class BrowserViewModel: ObservableObject, ActriumModePublisher {
    // MARK: - ActriumModePublisher conformance
    var actriumModePublisher: AnyPublisher<ActriumMode, Never> {
        $actriumMode.eraseToAnyPublisher()
    }
    @Published var tabs: [Tab] = []
    @Published var currentTabId: UUID? {
        didSet {
            guard currentTabId != oldValue else { return }
            if let oldValue, deferredRealmLinks[oldValue] != nil {
                cancelRealmRefresh(oldValue)
                pendingInviteRedeems.removeValue(forKey: oldValue)
            }
            openSelectedDeferredRealm()
        }
    }
    // Default that all tabs are left-side panel visible; overrides are recorded only when user switching
    @Published private var leftPanelVisibility: [UUID: Bool] = [:]

    // MARK: - Tab Groups
    @Published var tabGroups: [TabGroup] = []

    // 🔴 Development debugging: Go straight into the action Focus Mode
    @Published var shouldStartInFocusMode: Bool = true

    /// Actrium display mode (Overview vs Focus)
    /// Stored here to survive view recreation when DevTools layout changes
    @Published var actriumMode: ActriumMode = .overview

    enum AddressBarFocusPresentation: Equatable {
        case immediate
        case afterTabStripSettles
    }

    @Published private(set) var addressBarFocusPresentation: AddressBarFocusPresentation = .immediate

    /// Flag to trigger address bar focus (set by addNewTab, consumed by view).
    @Published var shouldAutoFocusAddressBar: Bool = false {
        didSet {
            if !shouldAutoFocusAddressBar { addressBarFocusPresentation = .immediate }
        }
    }

    /// Current tab loading state (for web content)
    @Published var isCurrentTabLoading: Bool = false

    /// Current tab loading progress (0.0 to 1.0)
    @Published var loadingProgress: Double = 0

    // MARK: - Package Direct-Link Download (15-B B1)
    // Confirm-before-connect flow: confirm → download (size cap) → SHA-256 → import.

    /// Package link awaiting user confirmation (confirm sheet is presented).
    @Published var pendingPackageDownload: PendingPackageDownload?

    /// In-flight or failed package download (progress sheet is presented).
    @Published var activePackageDownload: ActivePackageDownload?

    /// Notice for formal actr:// addresses (realm resolution pending in prototype).
    @Published var actrUriNotice: ActrUriNotice?

    @Published private var pendingInviteRedeems: [UUID: PendingInviteRedeem] = [:]

    var pendingInviteRedeem: PendingInviteRedeem? {
        currentTabId.flatMap { pendingRealmInvite(for: $0) }
    }

    func pendingRealmInvite(for tabId: UUID) -> PendingInviteRedeem? {
        guard let pending = pendingInviteRedeems[tabId],
              tabs.contains(where: { pending.matches($0) }) else { return nil }
        return pending
    }

    @Published private var inviteRedemptionId: UUID?
    var inviteRedemptionInFlight: Bool { inviteRedemptionId != nil }
    private var inviteRedemptionTail: Task<RealmJoinService.VerifiedProfile, Error>?
    private let pendingRealmInvites: PendingRealmInvites
    private let realmPreferences: UserDefaults
    private let realmTrustDirectory: URL?
    private var realmTrustObserver: AnyCancellable?

    /// Realm names remembered for session restore. Admission checks the current device separately.
    private var redeemedRealms: Set<String> {
        get { Set(realmPreferences.stringArray(forKey: "actr.redeemedRealms") ?? []) }
        set { realmPreferences.set(Array(newValue), forKey: "actr.redeemedRealms") }
    }

    private func markRedeemed(_ realmName: String) {
        var updated = redeemedRealms
        if updated.insert(realmName).inserted { redeemedRealms = updated }
    }

    /// Current download task handle, kept for cancellation.
    private var packageDownloadTask: Task<Void, Never>?

    private struct LoadedRealmFront {
        let address: URL
        let signingKeyId: String
        let selection: RealmFrontSelection
    }

    private struct VerifiedRealmFront {
        let bytes: Data
        let trustedKey: Data
        let serviceType: ActrBindings.ActrType
    }

    private struct LocalFrontScope: Equatable {
        let realm: String
        let provider: String
        let issuer: String
        let front: RealmPackageInfo
        let service: RealmPackageInfo
        let publicKey: Data
    }

    private struct LocalFrontApproval {
        let scope: LocalFrontScope
        let file: LocalRealmFrontFile
    }

    private var localFrontApprovals: [UUID: LocalFrontApproval] = [:]
    private var deferredRealmLinks: [UUID: ActrUriInfo] = [:]

    private var loadedRealmFronts: [UUID: LoadedRealmFront] = [:]
    private var realmRefreshes: [UUID: (id: UUID, task: Task<Void, Never>, inviteCode: String?)] = [:]
    private static var backgroundRealmDocuments: [URL: Task<Data, Error>] = [:]
    private var realmRefreshObservation: AnyCancellable?
    @Published private var realmUpdateOffers: [UUID: RealmFrontUpdateOffer] = [:]
    private var deferredRealmUpdates: [UUID: RealmFrontSelection] = [:]

    var currentRealmUpdate: RealmFrontUpdateOffer? {
        guard let tab = currentTab, let offer = realmUpdateOffers[tab.id],
              tab.url == offer.address, tab.reloadNonce == offer.navigationNonce else { return nil }
        return offer
    }

    func confirmRealmUpdate(_ offer: RealmFrontUpdateOffer) {
        guard currentRealmUpdate == offer, let tab = currentTab else { return }
        realmUpdateOffers.removeValue(forKey: tab.id)
        cancelRealmRefresh(tab.id)
        refreshRealmFront(tab, background: false, approved: offer.selection)
    }

    func deferRealmUpdate(_ offer: RealmFrontUpdateOffer) {
        guard currentRealmUpdate == offer else { return }
        deferredRealmUpdates[offer.tabId] = offer.selection
        realmUpdateOffers.removeValue(forKey: offer.tabId)
    }

    // MARK: - Instance Creation (15-B B6)
    // Confirm → serviceCreate → URI replace (replace, not push; history keeps only the
    // final instance address; on failure the package URL stays unchanged).

    /// Package import awaiting instance-creation confirmation (confirm sheet).
    @Published var pendingInstanceCreation: PendingInstanceCreation?

    /// Prototype instance store (15 §5: the mock ActorId skeleton keeps the
    /// interface identical to the real ID; switch to the real actr-ffi
    /// backend once delivered — shell call sites unchanged).
    private let serviceInstanceStore: ActrServiceInstanceProviding = MockActrServiceInstanceStore()

    // MARK: - Grid Slot Position Management

    /// Tab ID → grid slot index mapping
    @Published var tabSlotPositions: [UUID: Int] = [:]

    /// Toolbar Actual height (from view measurements)
    @Published var toolbarHeight: CGFloat = 52 // Default value, updated in view

    /// History view model reference for recording ACTR app visits
    weak var historyVM: HistoryViewModel?
    private lazy var appRepository = AppRepository()
    private let saveBrowserSession: ([Tab], UUID?) -> Void
    private let chooseLocalFrontFile: (NSOpenPanel) -> URL?
    private let fetchRealmDocument: @MainActor (URL, String) async throws -> Data
    private let joinOpenRealm: @MainActor (RealmJoinService.VerifiedProfile) async throws -> Void
    private let inviteMembershipExchange: RealmJoinService.InviteExchange?
    private let confirmRealmMembership: @MainActor (RealmJoinService.VerifiedProfile) async throws -> Bool
    private let downloadRealmArchive: @MainActor (URL) async throws -> URL

    var currentTab: Tab? {
        guard let currentTabId = currentTabId else { return nil }
        return tabs.first(where: { $0.id == currentTabId })
    }

    init(
        chooseLocalFrontFile: @escaping (NSOpenPanel) -> URL? = { panel in
            panel.runModal() == .OK ? panel.url : nil
        },
        realmPreferences: UserDefaults = .standard,
        realmTrustDirectory: URL? = nil,
        appRepository: AppRepository? = nil,
        pendingRealmInvites: PendingRealmInvites? = nil,
        fetchRealmDocument: (@MainActor (URL, String) async throws -> Data)? = nil,
        joinOpenRealm: (@MainActor (RealmJoinService.VerifiedProfile) async throws -> Void)? = nil,
        inviteMembershipExchange: RealmJoinService.InviteExchange? = nil,
        confirmRealmMembership: (@MainActor (RealmJoinService.VerifiedProfile) async throws -> Bool)? = nil,
        downloadRealmArchive: (@MainActor (URL) async throws -> URL)? = nil,
        saveBrowserSession: @escaping ([Tab], UUID?) -> Void = { tabs, currentTabId in
            BrowserDataRepository.shared.saveSession(tabs: tabs, currentTabId: currentTabId)
        }
    ) {
        self.chooseLocalFrontFile = chooseLocalFrontFile
        self.realmPreferences = realmPreferences
        self.realmTrustDirectory = realmTrustDirectory
        self.pendingRealmInvites = pendingRealmInvites ?? .shared
        self.fetchRealmDocument = fetchRealmDocument ?? Self.fetchBoundedDocument(from:insecureMessage:)
        self.joinOpenRealm = joinOpenRealm ?? {
            try await RealmJoinService.joinOpen($0, label: "Mac", preferences: realmPreferences)
        }
        self.inviteMembershipExchange = inviteMembershipExchange
        self.confirmRealmMembership = confirmRealmMembership ?? {
            try await RealmJoinService.hasConfirmedMembership($0, preferences: realmPreferences)
        }
        self.downloadRealmArchive = downloadRealmArchive ?? { try await ActrPackageDownloadService.download(from: $0) }
        self.saveBrowserSession = saveBrowserSession
        if let appRepository { self.appRepository = appRepository }
        // Initialize slot positions first
        initializeSlotPositions()

        // Create initial StartUp tab
        let startTab = Tab(
            title: "Start Page",
            systemTabType: .startPage,
            backgroundColor: AppColors.nativeTabBackgroundHex
        )
        tabs.append(startTab)
        currentTabId = startTab.id
        tabSlotPositions[startTab.id] = 0
        actriumMode = .focus(tabId: startTab.id)

        realmRefreshObservation = NotificationCenter.default
            .publisher(for: NSApplication.didBecomeActiveNotification)
            .map { _ in () }
            .merge(with: Timer.publish(every: 300, on: .main, in: .common).autoconnect().map { _ in () })
            .sink { [weak self] _ in
                Task { @MainActor [weak self] in self?.refreshLoadedRealmFronts() }
            }

        realmTrustObserver = NotificationCenter.default
            .publisher(for: RealmProfileStore.didRetire, object: realmPreferences)
            .merge(with: NotificationCenter.default.publisher(
                for: RealmProfileStore.didUpdateAuthorityKeys, object: realmPreferences
            ))
            .sink { [weak self] notice in
                if let realm = notice.userInfo?["realm"] as? String {
                    self?.stopRetiredRealm(realm)
                } else if let domain = notice.userInfo?["domain"] as? String,
                          let keyIds = notice.userInfo?["signingKeyIds"] as? [String] {
                    self?.stopRevokedRealmRuntimes(domain: domain, trustedKeys: Set(keyIds))
                }
            }
    }

    // Tab runtime cleanup is explicit on the main actor.
    nonisolated deinit {}

    private func persistBrowserPageSession() {
        saveBrowserSession(tabs, currentTabId)
    }

    func requestAddressBarFocus(
        presentation: AddressBarFocusPresentation = .immediate
    ) {
        addressBarFocusPresentation = presentation
        shouldAutoFocusAddressBar = true
    }

    @discardableResult
    func addNewTab(focusAddressBar: Bool = true) -> Tab {
        logger.debug("🆕 Start New Tab...")
        let newTab = Tab(
            title: "New Tab",
            systemTabType: .startPage,
            backgroundColor: AppColors.nativeTabBackgroundHex
        )
        tabs.append(newTab)
        currentTabId = newTab.id

        // Simplification: Add directly to the end
        tabSlotPositions[newTab.id] = tabs.count - 1

        // New Tab Direct access. Focus Mode
        actriumMode = .focus(tabId: newTab.id)
        // Only a pure new blank Tab , and not focus when opening links
        if focusAddressBar {
            requestAddressBarFocus(presentation: .afterTabStripSettles)
        } else {
            shouldAutoFocusAddressBar = false
        }

        logger.notice(" New Tab Created: \(newTab.title), Location: \(self.tabs.count - 1)")
        // A native Start Page has no restorable HTTP(S) URL. Persist after
        // navigation instead of rewriting SQLite on the ⌘T path.
        return newTab
    }

    /// Open or switch to a system tab (History, Bookmarks)
    /// Returns the existing tab if already open, otherwise creates a new one
    @discardableResult
    func openSystemTab(_ type: SystemTabType) -> Tab {
        defer { persistBrowserPageSession() }
        shouldAutoFocusAddressBar = false
        // Check if a system tab of this type is already open
        if let existingTab = tabs.first(where: { $0.systemTabType == type }) {
            // Switch to existing tab
            currentTabId = existingTab.id
            actriumMode = .focus(tabId: existingTab.id)
            logger.debug("🔄 Switch to an existing system label: \(type.title)")
            return existingTab
        }

        // Create new system tab with about: URL
        let newTab = Tab(
            title: type.title,
            url: type.aboutURL,
            systemTabType: type,
            backgroundColor: AppColors.nativeTabBackgroundHex
        )
        tabs.append(newTab)
        currentTabId = newTab.id
        tabSlotPositions[newTab.id] = tabs.count - 1
        actriumMode = .focus(tabId: newTab.id)

        logger.notice("📱 Created System Label: \(type.title) (\(type.aboutURL.absoluteString)), Location: \(self.tabs.count - 1)")
        return newTab
    }

    func switchToTab(id: UUID) {
        if let tab = tabs.first(where: { $0.id == id }) {
            logger.debug("🔄 Switch to Tab: \(tab.title)")
        }
        currentTabId = id
        // Ensure actrium is in focus mode when switching tabs
        actriumMode = .focus(tabId: id)
        persistBrowserPageSession()
    }

    func resolveCapabilityPermission(
        tabId: UUID,
        capability: String,
        choice: CapabilityPermissionChoice
    ) {
        guard let index = tabs.firstIndex(where: { $0.id == tabId }) else {
            return
        }
        let tab = tabs[index]
        let stored = CapabilityPermissionStore.shared.decide(
            tabId: tabId,
            appId: tab.appId,
            version: nil,
            capability: capability,
            choice: choice
        )
        guard stored else { return }
        tabs[index].capabilityRevision += 1
        objectWillChange.send()
        persistBrowserPageSession()
    }

    private func releaseTabRuntime(_ id: UUID, preserveRealmLoad: Bool = false) {
        localFrontApprovals.removeValue(forKey: id)
        if !preserveRealmLoad {
            cancelRealmRefresh(id)
            pendingInviteRedeems.removeValue(forKey: id)
            deferredRealmLinks.removeValue(forKey: id)
        }
        loadedRealmFronts.removeValue(forKey: id)
        realmUpdateOffers.removeValue(forKey: id)
        deferredRealmUpdates.removeValue(forKey: id)
        TabSnapshotCacheManager.shared.invalidate(tabId: id)
        WebViewHolderManager.shared.remove(tabId: id)
        ViewBridgeHostViewManager.shared.remove(tabId: id)
        RealmServiceBindings.shared.remove(id)
        RendererLifecycleManager.shared.tabDidClose(id)
        CapabilityPermissionStore.shared.clearSessionGrants(tabId: id)
    }

    func closeTab(id: UUID) {
        releaseTabRuntime(id)
        // Get the slot position of the tab being closed
        guard let closedPosition = tabSlotPositions[id] else {
            logger.warning(" Cannot find tab slot position")
            return
        }

        // If this is the last tab, exit application after closing
        if tabs.count == 1 {
            logger.debug("🚪 Close last tab, apply the exit is about to be completed")
            saveBrowserSession([], nil)
            NSApplication.shared.terminate(nil)
            return
        }

        // Switch to the adjacent label if the current tab is closed
        if currentTabId == id {
            let wasInFocusMode = actriumMode.isInFocusMode
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
                        actriumMode = .focus(tabId: newTabId)
                    }
                }
            }
        }

        // Remove Tab
        tabs.removeAll(where: { $0.id == id })

        // Remove the location map of this tab
        tabSlotPositions.removeValue(forKey: id)

        // Reorganization position: reduce all positions larger than the deleted position1
        for tabId in tabs.map({ $0.id }) {
            if let position = tabSlotPositions[tabId], position > closedPosition {
                tabSlotPositions[tabId] = position - 1
            }
        }

        logger.debug("🗑️ Close Tab, Location \(closedPosition)Remaining \(self.tabs.count) Tab")
        persistBrowserPageSession()
    }

    func reorderTab(fromIndex source: Int, toIndex destination: Int) {
        guard source >= 0, source < tabs.count,
              destination >= 0, destination <= tabs.count,
              source != destination else { return }

        let movedTab = tabs.remove(at: source)
        let insertIndex = destination > source ? destination - 1 : destination
        tabs.insert(movedTab, at: insertIndex)

        logger.debug("🔄 Tab Reorder: From Index \(source) Move To \(insertIndex)")
        persistBrowserPageSession()
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

        logger.debug("📍 Tab Move: \(tab.title) To Location \(targetIndex), Group: \(groupId?.uuidString ?? "None")")

        // Clean up old empty group if needed
        if let oldId = oldGroupId, oldId != groupId {
            cleanupEmptyGroup(oldId)
        }
        persistBrowserPageSession()
    }

    /// Keep pointer-driven toolbar moves in memory until mouse-up. Persisting
    /// every crossed slot would stall the drag and could remove a group while
    /// its last tab is passing through another section.
    func moveTabInteractively(tabId: UUID, toIndex: Int, groupId: UUID?) {
        guard let fromIndex = tabs.firstIndex(where: { $0.id == tabId }) else { return }
        tabs[fromIndex].groupId = groupId
        let tab = tabs.remove(at: fromIndex)
        tabs.insert(tab, at: min(max(0, toIndex), tabs.count))
    }

    func moveGroupInteractively(groupId: UUID, toIndex: Int) {
        let movingTabs = tabs.filter { $0.groupId == groupId }
        guard !movingTabs.isEmpty else { return }
        tabs.removeAll { $0.groupId == groupId }
        tabs.insert(contentsOf: movingTabs, at: min(max(0, toIndex), tabs.count))
    }

    func commitInteractiveTabReorder() {
        let populatedGroupIDs = Set(tabs.compactMap(\.groupId))
        tabGroups.removeAll { !populatedGroupIDs.contains($0.id) }
        tabSlotPositions = Dictionary(
            uniqueKeysWithValues: tabs.enumerated().map { ($0.element.id, $0.offset) }
        )
        persistBrowserPageSession()
    }

    /// Insert a tab received from another window (cross-window drag)
    func insertTabFromCrossWindow(_ tab: Tab, at index: Int, groupId: UUID?, select: Bool = true) {
        var newTab = tab
        newTab.groupId = groupId
        if let address = newTab.url, address.scheme?.lowercased() == "actr" {
            let linkAddress = ActrLinkInvite.strippingInviteArgs(from: address.absoluteString)
            newTab.url = linkAddress.flatMap { URL(string: $0) }
            let link = newTab.url.flatMap {
                try? ActriumCore.parseActrUri(
                    uri: $0.absoluteString, installationId: InstallationIdProvider.getOrCreate()
                )
            }
            // Transfer payloads contain serialized Guest content, so reopen remote
            // Realm addresses through signed discovery in the destination.
            if link?.isLocal != true {
                Self.clearRealmContent(&newTab)
                deferredRealmLinks[newTab.id] = link
            }
        }

        let targetIndex = min(max(0, index), tabs.count)
        tabs.insert(newTab, at: targetIndex)
        tabSlotPositions[newTab.id] = tabs.count - 1
        if select {
            currentTabId = newTab.id
            if actriumMode.isInFocusMode { actriumMode = .focus(tabId: newTab.id) }
        }

        logger.notice(" Insert across Window Tab: \(newTab.title) To Location \(targetIndex), Group: \(groupId?.uuidString ?? "None")")
        persistBrowserPageSession()
    }

    private func openSelectedDeferredRealm() {
        guard let id = currentTabId, let link = deferredRealmLinks[id],
              realmRefreshes[id] == nil, pendingInviteRedeem == nil,
              let index = tabs.firstIndex(where: { $0.id == id }),
              let address = tabs[index].url else { return }
        handleResolvedRealmLink(link, input: address.absoluteString, tabIndex: index)
    }

    /// Remove a tab by ID (used for cross-window drag source cleanup)
    func removeTabById(_ tabId: UUID) {
        guard let index = tabs.firstIndex(where: { $0.id == tabId }) else { return }
        releaseTabRuntime(tabId)
        let tab = tabs[index]
        let oldGroupId = tab.groupId

        tabs.remove(at: index)
        if let removedPosition = tabSlotPositions.removeValue(forKey: tabId) {
            for id in tabs.map(\.id) {
                if let position = tabSlotPositions[id], position > removedPosition {
                    tabSlotPositions[id] = position - 1
                }
            }
        }

        // Update current tab if needed
        if currentTabId == tabId {
            currentTabId = tabs.isEmpty ? nil : tabs[max(0, index - 1)].id
        }
        if case .focus(let focusedId) = actriumMode, focusedId == tabId {
            actriumMode = currentTabId.map { .focus(tabId: $0) } ?? .overview
        }

        // Clean up empty group
        if let oldId = oldGroupId {
            cleanupEmptyGroup(oldId)
        }

        logger.debug("🗑️ Remove across Window Tab: \(tab.title)")
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

        if isDirectory.boolValue {
            // Directory mode - use directly
            rootDir = resolvedURL
        } else {
            // Package archive mode — digest-addressed cache (15-B B2, 05 §15).
            do {
                let digest = try ActrPackageDownloadService.sha256FileSync(at: resolvedURL)
                let archiveURL = try ActrPackageCache.installArchive(at: resolvedURL, digest: digest)
                rootDir = try ActrPackageCache.extractArchiveIfNeeded(archiveURL, digest: digest) {
                    try self.validateExtractedPackage(at: $0)
                }
            } catch {
                logger.error(" [restoreGroup] .actr Loading failed:\(error.localizedDescription)")
                return
            }
        }

        do {
            let manifestInfo = try readPackageManifest(fromRoot: rootDir, requireSignature: !isDirectory.boolValue)
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
                guestExtractedPath: isDirectory.boolValue ? nil : rootDir.path,
                guestEntry: !isDirectory.boolValue && unifiedScript != nil
                    ? manifestInfo.unifiedScript
                    : nil,
                capabilityManifestJSON: manifestInfo.capabilityManifestJSON,
                localStorageOrigin: manifestInfo.storageOrigin(devDirectory: isDirectory.boolValue ? resolvedURL : nil),
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
            persistBrowserPageSession()

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
        // WebKit can report a URL from the old page after the tab has changed content.
        guard case .web = tabs[index].contentType else { return }
        let oldURL = tabs[index].url?.absoluteString ?? "nil"
        tabs[index].url = url
        logger.notice("🔗 [updateTabURL] Tab \(tabId) URL updated from \(oldURL) to \(url.absoluteString)")
        persistBrowserPageSession()
    }

    func webTabDidActivateLink(_ tabId: UUID) {
        guard let tab = tabs.first(where: { $0.id == tabId }), case .web = tab.contentType else { return }
        localFrontApprovals.removeValue(forKey: tabId)
        deferredRealmLinks.removeValue(forKey: tabId)
        pendingInviteRedeems.removeValue(forKey: tabId)
        cancelRealmRefresh(tabId)
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

    /// Navigation Current Tab To Assign URL Or apply ID
    func openURLInNewTab(_ url: URL) {
        if url.scheme?.lowercased() == "actr",
           let code = ActrLinkInvite.invite(from: url.absoluteString), hasPendingInvite(code) { return }
        // These flows manage their own tabs when they recover an instance or import a package.
        if !ActrInstanceAddress.isLocalInstanceURL(url), !ActrPackageLinkPolicy.isPackageDownloadURL(url) {
            addNewTab(focusAddressBar: false)
        }
        navigateCurrentTab(to: url.absoluteString)
    }

    /// Unified navigation portal - Process all types of input
    /// - If the input is about: URLNavigator to the corresponding system application
    /// - If the input is actro:// URLLoad Actrium Apply
    /// - If the input is http/https URL, as Web Load
    /// - Load if input is a local file path ACTR Apply
    /// - Otherwise, as app id Process
    func navigateCurrentTab(to input: String, fromAddressBar: Bool = false) {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        let isStandaloneInvite = trimmed.hasPrefix("ri_")
        let isRealmAddress = trimmed.lowercased().hasPrefix("actr://")
        guard !isStandaloneInvite || fromAddressBar else { return }
        let inviteCode = isStandaloneInvite ? trimmed : (isRealmAddress ? ActrLinkInvite.invite(from: trimmed) : nil)
        if let inviteCode, hasPendingInvite(inviteCode) { return }

        // Address-bar navigation must always land somewhere: create a tab
        // when the window has none (e.g. right after a relaunch).
        if currentTabId == nil || tabs.firstIndex(where: { $0.id == currentTabId }) == nil {
            addNewTab(focusAddressBar: false)
        }
        guard let currentTabId = currentTabId,
              let index = tabs.firstIndex(where: { $0.id == currentTabId }) else {
            logger.warning("⚠️ [navigateCurrentTab] No Current tab after creating one")
            return
        }

        cancelRealmRefresh(currentTabId)
        localFrontApprovals.removeValue(forKey: currentTabId)
        deferredRealmLinks.removeValue(forKey: currentTabId)
        pendingInviteRedeems.removeValue(forKey: currentTabId)

        // A standalone invite goes through the Realm flow before address or search handling.
        if isStandaloneInvite {
            do {
                let address = try RealmJoinService.address(forInvite: trimmed)
                openRealmAddress(address, tabIndex: index, inviteText: trimmed)
            } catch {
                presentRealmLinkError(error)
            }
            return
        }
        if isRealmAddress {
            openRealmAddress(trimmed, tabIndex: index)
            return
        }

        // 1. Check for local file path
        if isLocalFilePath(trimmed) {
            let fileURL = URL(fileURLWithPath: trimmed).standardizedFileURL
            var isDirectory: ObjCBool = false
            if FileManager.default.fileExists(atPath: fileURL.path, isDirectory: &isDirectory) {
                if isDirectory.boolValue {
                    logger.notice("📂 [navigateCurrentTab] Load Development Directory: \(fileURL.path)")
                    loadFromDevDirectory(fileURL)
                } else {
                    logger.notice("📦 [navigateCurrentTab] Load ACTR Package: \(fileURL.path)")
                    importACTRPackage(from: fileURL)
                }
                return
            } else {
                logger.warning("⚠️ [navigateCurrentTab] File does not exist: \(fileURL.path)")
                return
            }
        }

        // 2. Check for yes or no URL
        if let url = parseURL(trimmed) {
            // Check if it's an about: URL for system apps
            if let systemType = SystemTabType.from(url: url) {
                logger.notice("📱 [navigateCurrentTab] Navigation to System Application: \(url.absoluteString)")
                RealmServiceBindings.shared.remove(currentTabId)
                // Update Tab to system app
                tabs[index].systemTabType = systemType
                tabs[index].url = url
                tabs[index].title = systemType.title
                // Clear web/RN content
                tabs[index].unifiedScript = nil
                tabs[index].capabilityManifestJSON = nil
                tabs[index].localStorageOrigin = nil
                tabs[index].localStoragePublisherIdentity = nil
                tabs[index].preparedLocalStorage = nil
                tabs[index].leftPanelScript = nil
                tabs[index].rightPanelScript = nil
                tabs[index].guestScript = nil
                tabs[index].guestExtractedPath = nil
                tabs[index].guestEntry = nil
                tabs[index].htmlContent = nil
                tabs[index].contractName = nil
                tabs[index].legacyActorBridge = false
                objectWillChange.send()
                persistBrowserPageSession()
                return
            }

            // Resolve registered package IDs and absolute package paths.
            if ActriumAppURLScheme.isSupported(url.scheme) {
                var sourcePath: String?

                if let bundleId = ActriumAppURLScheme.bundleId(from: url) {
                    logger.notice("📱 [navigateCurrentTab] Navigator to Actrium Apply (bundleId): \(bundleId)")
                    if let app = appRepository.getByBundleId(bundleId) {
                        sourcePath = app.sourcePath
                        appRepository.updateLastUsed(bundleId: bundleId)
                    } else {
                        logger.warning("⚠️ [navigateCurrentTab] No applications found: \(bundleId)")
                        return
                    }
                } else {
                    sourcePath = url.path
                    logger.notice("📂 [navigateCurrentTab] Navigator to Local Actrium Apply: \(sourcePath ?? "")")
                }

                // Load ACTR app into current tab
                if let path = sourcePath {
                    loadActrPackageIntoCurrentTab(from: URL(fileURLWithPath: path), tabIndex: index)
                }
                return
            }

            // 3. Package direct link (https URL pointing at an .actr archive):
            // enter the confirm-before-connect download flow instead of loading
            // it as a web page (15-B B1).
            if ActrPackageLinkPolicy.isPackageDownloadURL(url) {
                logger.notice("📦 [navigateCurrentTab] Package direct link: \(url.absoluteString)")
                // Cache hit for a previously downloaded link (15-B B2): the same
                // package reopens without downloading or re-extracting.
                if let digest = ActrPackageCache.cachedDigest(forDownloadURL: url) {
                    logger.notice("📦 [navigateCurrentTab] Cache hit, sha256=\(digest)")
                    importACTRPackage(
                        from: ActrPackageCache.archiveURL(forDigest: digest),
                        knownDigest: digest)
                } else {
                    pendingPackageDownload = PendingPackageDownload(url: url)
                }
                return
            }

            logger.notice("🌐 [navigateCurrentTab] Navigator to URL: \(url.absoluteString)")
            RealmServiceBindings.shared.remove(currentTabId)
            // Update Tab Yes Web Contents
            tabs[index].url = url
            tabs[index].title = url.host ?? url.absoluteString
            // Clear RN Relevant
            tabs[index].unifiedScript = nil
            tabs[index].capabilityManifestJSON = nil
            tabs[index].localStorageOrigin = nil
            tabs[index].localStoragePublisherIdentity = nil
            tabs[index].preparedLocalStorage = nil
            tabs[index].leftPanelScript = nil
            tabs[index].rightPanelScript = nil
            tabs[index].guestScript = nil
            tabs[index].guestExtractedPath = nil
            tabs[index].guestEntry = nil
            tabs[index].systemTabType = nil
            tabs[index].contractName = nil
            tabs[index].legacyActorBridge = false
            objectWillChange.send()
            persistBrowserPageSession()
        } else {
            // Try to load as app bundle ID directly
            logger.notice("📱 [navigateCurrentTab] Try as bundle ID Load: \(trimmed)")
            if let app = appRepository.getByBundleId(trimmed) {
                appRepository.updateLastUsed(bundleId: trimmed)
                loadActrPackageIntoCurrentTab(from: URL(fileURLWithPath: app.sourcePath), tabIndex: index)
            } else {
                logger.warning("⚠️ [navigateCurrentTab] No applications found: \(trimmed)")
            }
        }
    }

    // MARK: - Package Direct-Link Download (15-B B1)
    // confirm → download (size cap) → SHA-256 digest → hand over to the import pipeline.

    func confirmPendingPackageDownload() {
        guard let pending = pendingPackageDownload else { return }
        pendingPackageDownload = nil
        beginPackageDownload(pending)
    }

    func cancelPendingPackageDownload() {
        pendingPackageDownload = nil
    }

    func cancelActivePackageDownload() {
        packageDownloadTask?.cancel()
        packageDownloadTask = nil
        activePackageDownload = nil
    }

    func retryActivePackageDownload() {
        guard let active = activePackageDownload else { return }
        let pending = active.pending
        activePackageDownload = nil
        beginPackageDownload(pending)
    }

    private func beginPackageDownload(_ pending: PendingPackageDownload) {
        let active = ActivePackageDownload(pending: pending)
        activePackageDownload = active
        packageDownloadTask = Task { [weak self] in
            await self?.performPackageDownload(active)
        }
    }

    // MARK: - Instance Creation (15-B B6)

    private func offerInstanceCreation(
        for tab: Tab,
        packageTitle: String,
        packagePath: String,
        packageDigest: String,
        displayName: String?,
        permissions: [String]
    ) {
        guard pendingInstanceCreation == nil else { return }
        pendingInstanceCreation = PendingInstanceCreation(
            tabId: tab.id,
            packageTitle: packageTitle,
            packagePath: packagePath,
            packageDigest: packageDigest,
            displayName: displayName,
            permissions: permissions
        )
    }

    func cancelInstanceCreation() {
        pendingInstanceCreation = nil
    }

    // MARK: - Invite Code Redemption

    private func hasPendingInvite(_ code: String) -> Bool {
        pendingRealmInvites.contains(code: code) || realmRefreshes.values.contains { $0.inviteCode == code }
    }

    func cancelInviteRedeem(_ pending: PendingInviteRedeem) {
        guard pendingInviteRedeems[pending.tabId]?.id == pending.id else { return }
        pendingInviteRedeems.removeValue(forKey: pending.tabId)
        deferredRealmLinks.removeValue(forKey: pending.tabId)
    }

    func invalidateRealmNavigationForWindowClose() {
        for tabId in Array(realmRefreshes.keys) { cancelRealmRefresh(tabId) }
        deferredRealmLinks.removeAll()
        pendingInviteRedeems.removeAll()
    }

    func confirmInviteRedeem(_ pending: PendingInviteRedeem, code: String) {
        let trimmedCode = code.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedCode.isEmpty, !hasPendingInvite(trimmedCode),
              pendingInviteRedeems[pending.tabId]?.id == pending.id,
              let index = tabs.firstIndex(where: pending.matches) else { return }
        cancelInviteRedeem(pending)
        openRealmAddress(pending.uri, tabIndex: index, inviteText: trimmedCode)
    }

    private func redeemQueuedInvite(
        _ admission: RealmJoinService.VerifiedProfile, invite: RealmJoinService.Invite, load: UUID,
        checkCurrent: @escaping @MainActor () throws -> Void,
        validateProfile: @escaping @MainActor (RealmJoinService.VerifiedProfile) throws -> Void
    ) async throws -> RealmJoinService.VerifiedProfile {
        defer { pendingRealmInvites.finish(load) }
        let previous = inviteRedemptionTail
        let task = Task { @MainActor in
            _ = await previous?.result
            let admitted = try await RealmJoinService.joinWithInvite(
                admission, invite: invite, label: Host.current().localizedName ?? "Mac",
                preferences: realmPreferences, exchange: inviteMembershipExchange,
                checkMembership: confirmRealmMembership, checkCurrent: checkCurrent, validateProfile: validateProfile,
                beforeExchange: { pendingRealmInvites.exchangeStarted(load) }
            )
            markRedeemed(admitted.profile.info.realmName)
            if let result = admitted.result {
                logger.notice("🎟️ [InviteRedeem] joined \(result.realmName): member=\(result.memberId), role=\(result.role), key=\(result.keyId)")
            }
            return admitted.profile
        }
        inviteRedemptionTail = task
        inviteRedemptionId = load
        defer {
            if inviteRedemptionId == load {
                inviteRedemptionTail = nil
                inviteRedemptionId = nil
            }
        }
        return try await task.value
    }

    func confirmInstanceCreation() {
        guard let pending = pendingInstanceCreation else { return }
        pendingInstanceCreation = nil
        // The idempotency key covers retries within this single create; pressing
        // Creating again later means a new instance = a new key (15 §5).
        let idempotencyKey = UUID().uuidString
        Task { [weak self] in
            guard let self else { return }
            do {
                let handle = try await self.serviceInstanceStore.serviceCreate(
                    packagePath: pending.packagePath,
                    packageDigest: pending.packageDigest,
                    displayName: pending.displayName,
                    idempotencyKey: idempotencyKey
                )
                self.applyInstanceAddress(pending: pending, handle: handle)
            } catch {
                // Red line (15-B): on failure the URI stays the package address —
                // no silent degradation, the tab keeps rendering the package.
                logger.error("❌ [InstanceCreation] failed: \(error.localizedDescription)")
            }
        }
    }

    /// URI replace (replace, not push, 04 §4): the address bar swaps to the instance
    /// address; back does not return to the confirm page (the shell address is
    /// state, not a navigation stack); history records only the final address.
    private func applyInstanceAddress(pending: PendingInstanceCreation, handle: ActrServiceHandle) {
        guard let index = tabs.firstIndex(where: { $0.id == pending.tabId }),
              let instanceURL = ActrInstanceAddress.instanceURL(actorId: handle.actorId)
        else { return }
        let oldURL = tabs[index].url?.absoluteString ?? "nil"
        tabs[index].url = instanceURL
        if let displayName = pending.displayName, !displayName.isEmpty {
            tabs[index].title = displayName
        }
        logger.notice(
            "🔁 [InstanceCreation] URI replace: \(oldURL) → \(instanceURL.absoluteString), actorId=\(handle.actorId)"
        )
        historyVM?.recordVisit(tab: tabs[index])
        objectWillChange.send()
        persistBrowserPageSession()
    }

    private func performPackageDownload(_ active: ActivePackageDownload) async {
        do {
            let localURL = try await ActrPackageDownloadService.download(from: active.pending.url)
            defer { try? FileManager.default.removeItem(at: localURL) }
            if activePackageDownload?.id == active.id {
                activePackageDownload?.phase = .verifying
            }
            let digest = try await ActrPackageDownloadService.sha256Digest(of: localURL)
            // User cancelled while verifying — keep the cancelled state.
            guard activePackageDownload?.id == active.id else { return }
            logger.notice("✅ [PackageDownload] \(active.pending.filename) downloaded, sha256=\(digest)")

            // Keep the archive in the digest-addressed cache (15-B B2, 05 §15) and
            // remember the link → digest mapping so re-opening the same link skips
            // the download; the import pipeline reuses the cached extraction.
            let cachedArchiveURL = try ActrPackageCache.installArchive(at: localURL, digest: digest)
            ActrPackageCache.rememberDownloadURL(active.pending.url, digest: digest)

            // Hand over to the existing import pipeline (manifest → runtime tab).
            // ed25519 verification (15-B B3) hooks into this chain next.
            activePackageDownload = nil
            importACTRPackage(from: cachedArchiveURL, knownDigest: digest)
        } catch {
            // Download already cancelled/replaced by the user — keep the new state.
            guard activePackageDownload?.id == active.id else { return }
            let message = (error as? ActrPackageDownloadError)?.errorDescription
                ?? error.localizedDescription
            logger.error("❌ [PackageDownload] failed: \(message)")
            activePackageDownload?.phase = .failed
            activePackageDownload?.errorMessage = message
        }
    }

    /// Load ACTR app into the current tab (in-place update, not creating new tab)
    /// Used by actro:// URL navigation to update current tab content
    private func loadActrPackageIntoCurrentTab(from fileURL: URL, tabIndex: Int) {
        let resolvedURL = fileURL.standardizedFileURL

        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: resolvedURL.path, isDirectory: &isDirectory) else {
            logger.error("❌ [loadActrPackageIntoCurrentTab] Path does not exist: \(resolvedURL.path)")
            return
        }

        let rootDir: URL
        if isDirectory.boolValue {
            rootDir = resolvedURL
        } else {
            // Package archive mode — digest-addressed cache (15-B B2, 05 §15).
            do {
                let digest = try ActrPackageDownloadService.sha256FileSync(at: resolvedURL)
                let archiveURL = try ActrPackageCache.installArchive(at: resolvedURL, digest: digest)
                rootDir = try ActrPackageCache.extractArchiveIfNeeded(archiveURL, digest: digest) {
                    try self.validateExtractedPackage(at: $0)
                }
            } catch {
                logger.error("❌ [loadActrPackageIntoCurrentTab] Unpressure failed: \(error.localizedDescription)")
                return
            }
        }

        do {
            let manifestInfo = try readPackageManifest(fromRoot: rootDir, requireSignature: !isDirectory.boolValue)
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
               let url = URL(string: "\(ActriumAppURLScheme.primary)://\(appId)") {
                appURL = url
            } else {
                // Local dev path: actro:///absolute/path
                appURL = ActriumAppURLScheme.appURL(filePath: resolvedURL.path) ?? resolvedURL
            }

            RealmServiceBindings.shared.remove(tabs[tabIndex].id)
            // Update current tab in-place (like about: URL handling)
            logger.notice("📱 [loadActrPackageIntoCurrentTab] Update Current Tab: \(tabTitle), url=\(appURL.absoluteString)")

            tabs[tabIndex].title = tabTitle
            tabs[tabIndex].url = appURL
            let previousAppId = tabs[tabIndex].appId
            tabs[tabIndex].appId = manifestInfo.appId
            if previousAppId != manifestInfo.appId {
                CapabilityPermissionStore.shared.clearSessionGrants(tabId: tabs[tabIndex].id)
            }
            tabs[tabIndex].legacyActorBridge = ActriumApp.isLegacyActorBridgePackage(resolvedURL)
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
            } else {
                tabs[tabIndex].unifiedScript = nil
                tabs[tabIndex].leftPanelScript = nil
                tabs[tabIndex].rightPanelScript = nil
                tabs[tabIndex].guestScript = guestScript
            }

            tabs[tabIndex].contractName = manifestInfo.contractName
            tabs[tabIndex].contractVersion = manifestInfo.contractVersion
            tabs[tabIndex].permissions = manifestInfo.permissions
            tabs[tabIndex].capabilityManifestJSON = manifestInfo.capabilityManifestJSON
            tabs[tabIndex].localStorageOrigin = manifestInfo.storageOrigin(
                devDirectory: isDirectory.boolValue ? resolvedURL : nil
            )
            tabs[tabIndex].localStoragePublisherIdentity = nil
            tabs[tabIndex].preparedLocalStorage = nil
            tabs[tabIndex].leftPanelDefaultVisible = manifestInfo.leftPanelDefaultVisible
            tabs[tabIndex].rightPanelDefaultVisible = manifestInfo.rightPanelDefaultVisible

            // Archive-backed tabs retain their digest source for cache protection.
            // Only unified packages expose an entry to the renderer.
            tabs[tabIndex].guestExtractedPath = isDirectory.boolValue ? nil : rootDir.path
            tabs[tabIndex].guestEntry = !isDirectory.boolValue && unifiedScript != nil
                ? manifestInfo.unifiedScript
                : nil

            // Update panel visibility
            leftPanelVisibility[tabs[tabIndex].id] = manifestInfo.leftPanelDefaultVisible

            objectWillChange.send()
            persistBrowserPageSession()

            // Record History visit for ACTR app
            historyVM?.recordVisit(tab: tabs[tabIndex])

        } catch {
            logger.error("❌ [loadActrPackageIntoCurrentTab] Loading failed: \(error.localizedDescription)")
        }
    }

    private func openRealmAddress(_ input: String, tabIndex: Int, inviteText: String? = nil) {
        do {
            guard let linkAddress = ActrLinkInvite.strippingInviteArgs(from: input) else {
                throw RealmServiceError(message: "The Realm address is invalid.")
            }
            let info = try ActriumCore.parseActrUri(
                uri: linkAddress, installationId: InstallationIdProvider.getOrCreate()
            )
            guard inviteText == nil || !info.isLocal else {
                throw RealmServiceError(message: "Local Realms do not accept invitations.")
            }
            let invite = info.isLocal ? nil : (inviteText.map(RealmJoinService.Invite.explicit)
                ?? ActrLinkInvite.invite(from: input).map(RealmJoinService.Invite.sharedLink))
            handleResolvedRealmLink(info, input: linkAddress, tabIndex: tabIndex, invite: invite)
        } catch {
            logger.warning("⚠️ [navigateCurrentTab] actr:// input rejected: \(error)")
            presentRealmLinkError(error)
        }
    }

    /// Handle a sanitized Realm link already resolved by `ActriumCore.parseActrUri`
    /// (which delegates to `realm_link::open_realm_link` for ownership and
    /// dispatch). A local instance address focuses an open tab or recovers
    /// the instance through the instance store; a remote link kicks off
    /// authority/Realm Profile discovery, and the tab also updates to the
    /// resolved address.
    private func handleResolvedRealmLink(
        _ info: ActrUriInfo,
        input linkAddress: String,
        tabIndex: Int,
        invite: RealmJoinService.Invite? = nil
    ) {
        // The core parser guarantees a RealmId for every resolved link.
        guard let realmId = info.realmId else { return }
        let realmDisplay: String
        if let serial = info.serial {
            realmDisplay = "\(serial).\(realmId)@\(info.domain)"
        } else {
            realmDisplay = "\(realmId)@\(info.domain)"
        }
        logger.notice("🔗 [navigateCurrentTab] Resolved actr:// link: \(realmDisplay), local=\(info.isLocal)")

        // 04 §3: a local instance address opens the same instance — focus it
        // when already open, otherwise recover it through the instance store.
        if info.isLocal {
            if let existing = tabs.first(where: { $0.url?.absoluteString == linkAddress }) {
                switchToTab(id: existing.id)
                return
            }
            guard let address = URL(string: linkAddress) else { return }
            if serviceInstanceStore.resolveActorId(byAddressLabel: realmId) != nil {
                restoreInstanceTab(address: address, title: realmDisplay, isActiveTab: true)
            } else {
                logger.error("❌ [navigateCurrentTab] local instance not found for \(realmDisplay)")
                actrUriNotice = ActrUriNotice(
                    uri: linkAddress,
                    domain: info.domain,
                    detail: "The local instance was not found or has been cleared (SERVICE_NOT_FOUND); the address cannot be opened."
                )
            }
            return
        }

        guard let address = URL(string: linkAddress),
              let discoveryURL = info.discoveryUrl, let url = URL(string: discoveryURL)
        else { return }
        let tab = tabs[tabIndex]
        cancelRealmRefresh(tab.id)
        discoverAuthority(
            url: url, domain: info.domain, realmId: realmId, address: address,
            title: realmDisplay, replacing: tab, background: false, approved: nil, invite: invite
        )
    }

    private func unloadRealmFront(at tabIndex: Int) {
        releaseTabRuntime(tabs[tabIndex].id, preserveRealmLoad: true)
        Self.clearRealmContent(&tabs[tabIndex])
    }

    private static func clearRealmContent(_ tab: inout Tab) {
        tab.localStorageOrigin = nil
        tab.localStoragePublisherIdentity = nil
        tab.preparedLocalStorage = nil
        tab.reloadNonce &+= 1
        tab.appId = nil
        tab.systemTabType = nil
        tab.htmlContent = nil
        tab.unifiedScript = nil
        tab.leftPanelScript = nil
        tab.rightPanelScript = nil
        tab.guestScript = nil
        tab.guestExtractedPath = nil
        tab.guestEntry = nil
        tab.contractName = nil
        tab.contractVersion = nil
        tab.capabilityManifestJSON = nil
        tab.permissions = []
        tab.serviceCallUrl = nil
        tab.legacyActorBridge = false
        tab.realmProfileSigner = nil
    }

    private func stopRetiredRealm(_ realmName: String) {
        redeemedRealms.remove(realmName)
        let installation = InstallationIdProvider.getOrCreate()
        let affected = tabs.indices.filter { index in
            guard let address = tabs[index].url, address.scheme?.lowercased() == "actr",
                  let link = try? ActriumCore.parseActrUri(
                    uri: address.absoluteString, installationId: installation
                  ), !link.isLocal, let realmId = link.realmId else { return false }
            return "\(realmId)@\(link.domain)" == realmName
        }
        stopRealmRuntimes(affected)
    }

    private func stopRevokedRealmRuntimes(domain: String, trustedKeys: Set<String>) {
        let suffix = "@\(domain)"
        let affected = tabs.indices.filter { index in
            guard tabs[index].url?.scheme?.lowercased() == "actr",
                  let signer = tabs[index].realmProfileSigner else { return false }
            return signer.realmName.hasSuffix(suffix) && !trustedKeys.contains(signer.keyId)
        }
        stopRealmRuntimes(affected, preserveRealmLoad: true)
    }

    private func stopRealmRuntimes(_ affected: [Int], preserveRealmLoad: Bool = false) {
        guard !affected.isEmpty else { return }
        for index in affected {
            let tabId = tabs[index].id
            releaseTabRuntime(tabId, preserveRealmLoad: preserveRealmLoad)
            Self.clearRealmContent(&tabs[index])
        }
        objectWillChange.send()
        persistBrowserPageSession()
    }

    private func cancelRealmRefresh(_ id: UUID) {
        guard let refresh = realmRefreshes.removeValue(forKey: id) else { return }
        pendingRealmInvites.cancelBeforeExchange(refresh.id)
        refresh.task.cancel()
    }

    private func refreshLoadedRealmFronts() {
        guard NSApplication.shared.isActive else { return }
        for tab in tabs where tab.url != nil && loadedRealmFronts[tab.id]?.address == tab.url {
            refreshRealmFront(tab, background: true)
        }
    }

    private func refreshRealmFront(_ tab: Tab, background: Bool, approved: RealmFrontSelection? = nil) {
        guard realmRefreshes[tab.id] == nil, let address = tab.url,
              let info = try? ActriumCore.parseActrUri(
                  uri: address.absoluteString, installationId: InstallationIdProvider.getOrCreate()
              ), !info.isLocal, let realmId = info.realmId,
              let discoveryURL = info.discoveryUrl, let url = URL(string: discoveryURL)
        else { return }
        discoverAuthority(
            url: url, domain: info.domain, realmId: realmId, address: address,
            title: tab.title, replacing: tab, background: background, approved: approved
        )
    }

    /// Realm discovery (verification-funnel layer 1) followed by Realm
    /// Profile discovery (layer 2): fetch the authority descriptor, validate
    /// it, then fetch and verify the Realm Profile it points to. The package
    /// target that comes out feeds package acquisition: the archive digest is
    /// compared against the profile's authenticated `packageSha256` and the
    /// manifest signature is verified with the publisher keys from the
    /// profile funnel (15-B B3) before the package is cached, imported in the
    /// originating tab (same-tab replace) and its publisher keys pinned for
    /// restart restore.
    private func discoverAuthority(
        url: URL,
        domain: String,
        realmId: String,
        address: URL,
        title: String,
        replacing tab: Tab,
        background: Bool,
        approved: RealmFrontSelection?,
        invite: RealmJoinService.Invite? = nil
    ) {
        let replacingTabId = tab.id
        let refreshId = UUID()
        let realmName = "\(realmId)@\(domain)"
        let reservations = pendingRealmInvites
        if let invite { reservations.register(refreshId, code: invite.code, for: realmName) }
        let task = Task { @MainActor [weak self] in
            defer { reservations.cancelBeforeExchange(refreshId) }
            guard let self else { return }
            defer {
                if realmRefreshes[replacingTabId]?.id == refreshId {
                    realmRefreshes.removeValue(forKey: replacingTabId)
                    if pendingInviteRedeems[replacingTabId] == nil {
                        deferredRealmLinks.removeValue(forKey: replacingTabId)
                    }
                }
                openSelectedDeferredRealm()
            }
            var expectedURL = tab.url
            var navigationNonce = tab.reloadNonce
            @MainActor func tabIndex() -> Int? {
                guard self.realmRefreshes[replacingTabId]?.id == refreshId else { return nil }
                return self.tabs.firstIndex {
                    $0.id == replacingTabId && $0.reloadNonce == navigationNonce
                }
            }
            @MainActor func checkCurrent() throws {
                try Task.checkCancellation()
                guard tabIndex() != nil else { throw CancellationError() }
            }
            let installedProfileName = "\(realmName)/\(replacingTabId.uuidString)"
            var candidateBinding: RealmServiceBinding?
            defer {
                if let candidateBinding { Task { await candidateBinding.stop() } }
            }
            do {
                let data = try await fetchRealmUpdateDocument(
                    from: url, background: background,
                    insecureMessage: NSLocalizedString(
                        "The authority information was served over an insecure connection.",
                        comment: "Actr authority insecure transport error"))
                guard let body = String(data: data, encoding: .utf8) else {
                    throw NSError(
                        domain: "ActrDiscovery", code: 3,
                        userInfo: [NSLocalizedDescriptionKey: NSLocalizedString(
                            "The authority information is not valid text.",
                            comment: "Actr authority encoding error")])
                }

                try Task.checkCancellation()
                guard tabIndex() != nil else { return }
                let authority = try RealmProfileStore.acceptPinnedAuthority(
                    body: body,
                    domain: domain,
                    realmName: realmName,
                    preferences: realmPreferences,
                    fallbackPins: readRealmTrust(.authority, name: domain)
                )
                try saveRealmTrust(authority.pinsJson, .authority, name: domain)
                let descriptor = authority.descriptor
                // Authority revocation can stop the old renderer during discovery.
                guard realmRefreshes[replacingTabId]?.id == refreshId,
                      let refreshed = tabs.first(where: { $0.id == replacingTabId })
                else { return }
                navigationNonce = refreshed.reloadNonce
                let activeKeys = descriptor.signingKeys.filter { $0.status == "active" }
                logger.notice("🌐 [navigateCurrentTab] Authority accepted for \(domain): realms=\(descriptor.realmsHref), keys sequence=\(descriptor.keysSequence), active keys=\(activeKeys.count)")

                // Realm Profile discovery: GET <realms_href>/<realm_id>.
                var profilePath = descriptor.realmsHref
                if profilePath.hasSuffix("/") { profilePath.removeLast() }
                profilePath += "/\(realmId)"
                guard let profileURL = URL(string: profilePath) else {
                    throw NSError(
                        domain: "ActrDiscovery", code: 4,
                        userInfo: [NSLocalizedDescriptionKey: String(
                            format: NSLocalizedString(
                                "The Realm Profile location '%@' is invalid.",
                                comment: "Actr realm profile URL error"),
                            profilePath)])
                }
                let savedInstalledProfile = try? readRealmTrust(.installedProfile, name: installedProfileName)
                var guardedAuthority = (pins: authority.pinsJson, signedKeys: authority.signedKeysJson)
                var verifiedUpdate: RealmProfileUpdateInfo?
                @MainActor func validateProfile(_ data: Data) throws {
                    guard let document = String(data: data, encoding: .utf8) else {
                        throw CoreError.RealmProfileInvalid(message: "The Realm Profile is not valid text.")
                    }
                    guard let pinned = RealmProfileStore.pinnedAuthority(for: realmName, preferences: self.realmPreferences),
                          try self.readRealmTrust(.authority, name: domain) == pinned.pins else { throw CancellationError() }
                    verifiedUpdate = try ActriumCore.acceptRealmProfileUpdate(
                        body: document, realmName: realmName, signingKeysJson: pinned.signedKeys,
                        previousStateJson: self.readRealmTrust(.profile, name: realmName),
                        authorityPinsJson: pinned.pins, installedProfileJson: savedInstalledProfile
                    )
                    guardedAuthority = pinned
                }

                var admission: RealmJoinService.VerifiedProfile
                do {
                    admission = try await RealmJoinService.resolveProfile(
                        at: profileURL, realmName: realmName, signingKeysJSON: descriptor.signingKeysJson,
                        preferences: realmPreferences,
                        fetch: {
                            try await self.fetchRealmUpdateDocument(
                                from: $0, background: background,
                                insecureMessage: NSLocalizedString(
                                    "The Realm Profile was served over an insecure connection.",
                                    comment: "Actr realm profile insecure transport error"))
                        },
                        checkCurrent: {
                            try checkCurrent()
                            guard try self.readRealmTrust(.authority, name: domain) == authority.pinsJson
                            else { throw CancellationError() }
                        },
                        validateProfile: validateProfile
                    )
                } catch CoreError.RealmRetired(let name) {
                    if let retired = verifiedUpdate, retired.profile == nil {
                        try saveRealmTrust(retired.stateJson, .profile, name: realmName)
                    }
                    if !background { presentRealmLinkError(CoreError.RealmRetired(message: name)) }
                    return
                }
                guard let initial = verifiedUpdate, initial.profile != nil else { return }
                try saveRealmTrust(initial.stateJson, .profile, name: realmName)
                @MainActor func profileIsCurrent() throws -> Bool {
                    guard let verifiedUpdate, tabIndex() != nil,
                          RealmProfileStore.isCurrent(admission.accepted, preferences: self.realmPreferences),
                          try self.readRealmTrust(.authority, name: domain) == guardedAuthority.pins,
                          try self.readRealmTrust(.profile, name: realmName) == verifiedUpdate.stateJson
                    else { return false }
                    return true
                }

                let isLoadingTab = expectedURL != address || loadedRealmFronts[replacingTabId]?.address != address
                if !background || isLoadingTab {
                    let admitted: Bool
                    if let invite {
                        admission = try await redeemQueuedInvite(
                            admission, invite: invite, load: refreshId, checkCurrent: checkCurrent,
                            validateProfile: { current in
                                // A competing navigation may have advanced the accepted
                                // Profile while the invite was waiting for its turn.
                                try validateProfile(current.body)
                                guard let verifiedUpdate, verifiedUpdate.profile != nil else {
                                    throw CoreError.RealmRetired(message: realmName)
                                }
                                try self.saveRealmTrust(verifiedUpdate.stateJson, .profile, name: realmName)
                            }
                        )
                        admitted = true
                    } else if admission.isOpen {
                        await pendingRealmInvites.wait(for: realmName)
                        guard try profileIsCurrent() else { return }
                        try await joinOpenRealm(admission)
                        admitted = true
                    } else {
                        admitted = try await confirmRealmMembership(admission)
                    }

                    try Task.checkCancellation()
                    guard try profileIsCurrent() else { return }
                    guard admitted else {
                        if !background {
                            pendingInviteRedeems[replacingTabId] = PendingInviteRedeem(
                                tabId: replacingTabId, navigationNonce: navigationNonce,
                                uri: address.absoluteString, domain: domain
                            )
                        }
                        return
                    }
                    markRedeemed(realmName)
                }

                guard let accepted = verifiedUpdate, let availableProfile = accepted.profile,
                      let index = tabIndex() else { return }
                let currentProfileData = admission.body
                let profileBody = String(decoding: currentProfileData, as: UTF8.self)
                if isLoadingTab, localFrontApprovals[replacingTabId] == nil {
                    unloadRealmFront(at: index)
                    tabs[index].title = title
                    tabs[index].url = address
                    expectedURL = address
                    navigationNonce = tabs[index].reloadNonce
                    objectWillChange.send()
                    persistBrowserPageSession()
                }

                let previous = loadedRealmFronts[replacingTabId]
                let available = RealmFrontSelection(front: accepted.frontSelectionKey, service: accepted.serviceSelectionKey)
                let installedBody = previous == nil ? savedInstalledProfile : nil
                let installed = installedBody.flatMap { body in
                    try? ActriumCore.acceptRealmProfileUpdate(
                        body: body, realmName: realmName, signingKeysJson: guardedAuthority.signedKeys,
                        previousStateJson: nil, authorityPinsJson: guardedAuthority.pins, installedProfileJson: nil
                    )
                }
                let installedSelection: RealmFrontSelection? = installed.flatMap { snapshot in
                    guard snapshot.profile != nil else { return nil }
                    return RealmFrontSelection(front: snapshot.frontSelectionKey, service: snapshot.serviceSelectionKey)
                }
                let canReloadLocalFront = !background && previous?.selection == available
                    && localFrontApprovals[replacingTabId] != nil
                    && availableProfile.packages.contains { $0.reference == availableProfile.frontPackage && $0.localPath != nil }
                let action = canReloadLocalFront ? .loadAvailable : RealmFrontRefreshAction(
                    running: previous?.selection,
                    installed: installedSelection,
                    available: available,
                    approved: approved
                )
                let profile: RealmProfileInfo
                let profileData: Data
                let next: LoadedRealmFront
                switch action {
                case .keepRunning:
                    realmUpdateOffers.removeValue(forKey: replacingTabId)
                    deferredRealmUpdates.removeValue(forKey: replacingTabId)
                    let bindings = RealmServiceBindings.shared
                    if let activeBinding = bindings.binding(for: replacingTabId) {
                        let connection = try RealmServiceBinding.Connection(
                            profile: availableProfile, verifiedProfileBody: currentProfileData
                        )
                        if connection != activeBinding.connection {
                            let replacement = try await activeBinding.replacingConnection(connection)
                            candidateBinding = replacement
                            try await replacement.prepareForSwitch()
                            let replaced = try await bindings.replaceConnection(
                                activeBinding, with: replacement, tabId: replacingTabId
                            ) {
                                try profileIsCurrent()
                            }
                            guard replaced else { return }
                            candidateBinding = nil
                            logger.notice("Realm \(realmName): refreshed Service connection at Profile sequence \(availableProfile.sequence)")
                        }
                    }
                    loadedRealmFronts[replacingTabId] = LoadedRealmFront(
                        address: address, signingKeyId: accepted.signingKeyId, selection: available
                    )
                    if let index = tabIndex() { tabs[index].realmProfileSigner = (realmName, accepted.signingKeyId) }
                    try saveRealmTrust(profileBody, .installedProfile, name: installedProfileName)
                    return
                case .offerUpdate(let baseline):
                    let offer = RealmFrontUpdateOffer(
                        tabId: replacingTabId, address: address, navigationNonce: navigationNonce,
                        previous: baseline, selection: available
                    )
                    if (!background || deferredRealmUpdates[replacingTabId] != available),
                       realmUpdateOffers[replacingTabId] != offer {
                        realmUpdateOffers[replacingTabId] = offer
                        logger.notice("Realm \(realmName): update available at Profile sequence \(availableProfile.sequence)")
                    }
                    // A restored tab can reopen its installed version while the offer is visible.
                    guard previous == nil, let installed, let installedBody, let restored = installed.profile else { return }
                    profile = restored
                    profileData = Data(installedBody.utf8)
                    next = LoadedRealmFront(
                        address: address, signingKeyId: installed.signingKeyId,
                        selection: baseline
                    )
                case .loadAvailable:
                    profile = availableProfile
                    profileData = currentProfileData
                    next = LoadedRealmFront(
                        address: address, signingKeyId: accepted.signingKeyId, selection: available
                    )
                }
                logger.notice("📄 [navigateCurrentTab] Realm Profile accepted: \(profile.displayName) (\(profile.realmName)), sequence=\(profile.sequence), package=\(profile.packageHref)")

                let front = profile.packages.first { $0.reference == profile.frontPackage }
                if background, front?.localPath != nil { return }
                let publisher = try await realmPublisher(for: profile.packageType)
                try Task.checkCancellation()
                guard try profileIsCurrent() else { return }
                let publisherDomain = publisher.domain
                if let front, let localPath = front.localPath {
                    let selectedProfile = RealmProfileStore.AcceptedRealm(info: profile, signingKeyId: next.signingKeyId)
                    if let installedTabId = try await loadLocalRealmFront(
                        front, policy: localPath, accepted: selectedProfile, profileData: profileData,
                        publisher: publisher, address: address.absoluteString, tabId: replacingTabId,
                        isCurrent: { (try? profileIsCurrent()) == true }
                    ) {
                        loadedRealmFronts[installedTabId] = next
                        let installedName = "\(realmName)/\(installedTabId.uuidString)"
                        do {
                            try saveRealmTrust(String(decoding: profileData, as: UTF8.self), .installedProfile, name: installedName)
                        } catch {
                            logger.warning("Realm \(realmName): could not save installed Profile: \(error)")
                        }
                        if action == .loadAvailable {
                            realmUpdateOffers.removeValue(forKey: installedTabId)
                            deferredRealmUpdates.removeValue(forKey: installedTabId)
                        }
                    }
                    return
                }
                if localFrontApprovals[replacingTabId] != nil, approved != available {
                    throw RealmServiceError(message: "The Realm changed the Front source. Open its Realm address again.")
                }
                let packageKeys = publisher.packageSigningKeys

                let digest = profile.packageSha256.lowercased()
                guard ActrPackageCache.isValidDigest(digest) else {
                    throw CoreError.RealmProfileInvalid(message: "The Front package digest is invalid.")
                }
                let publisherKeys = publisher.fixedArchiveVerificationKeys
                let cachedArchiveURL = ActrPackageCache.archiveURL(forDigest: digest)
                let verified: VerifiedRealmFront
                if let cached = try? Self.verifyRealmFront(at: cachedArchiveURL, profile: profile, publisherKeys: publisherKeys) {
                    verified = cached
                } else {
                    guard let packageURL = URL(string: profile.packageHref),
                          packageURL.scheme?.lowercased() == "https" else {
                        throw NSError(
                            domain: "ActrDiscovery", code: 10,
                            userInfo: [NSLocalizedDescriptionKey: NSLocalizedString(
                                "The package location is invalid.",
                                comment: "Actr package URL error")])
                    }
                    let downloadedURL = try await downloadRealmArchive(packageURL)
                    defer { try? FileManager.default.removeItem(at: downloadedURL) }
                    let actualDigest = try await ActrPackageDownloadService.sha256Digest(of: downloadedURL)
                    guard actualDigest == digest else {
                        throw NSError(
                            domain: "ActrDiscovery", code: 25,
                            userInfo: [NSLocalizedDescriptionKey: String(
                                format: NSLocalizedString(
                                    "The downloaded package does not match the digest in the Realm Profile (PACKAGE_DIGEST_MISMATCH): expected %@…, got %@…",
                                    comment: "Actr package digest mismatch"),
                                String(digest.prefix(12)), String(actualDigest.prefix(12)))])
                    }
                    verified = try Self.verifyRealmFront(at: downloadedURL, profile: profile, publisherKeys: publisherKeys)
                    _ = try ActrPackageCache.installArchive(at: downloadedURL, digest: digest, replacingRejectedArchive: true)
                    ActrPackageCache.rememberDownloadURL(packageURL, digest: digest)
                }
                logger.notice("🔐 [navigateCurrentTab] Verified Realm Front \(profile.packageType) for Service \(profile.serviceType)")

                // Verify the manifest signature with the publisher keys the
                // profile funnel authenticated, then pin them per publisher
                // domain so a restart restore re-verifies with the same keys.
                let frontDirectory = try ActrPackageCache.extractArchiveIfNeeded(cachedArchiveURL, digest: digest) {
                    try self.validateExtractedPackage(
                        at: $0,
                        requireSignature: true,
                        additionalTrustedKeys: publisherKeys
                    )
                }
                ActrPackageTrustStore.recordVerifiedPublisherKeys(
                    domain: publisherDomain,
                    keys: packageKeys.compactMap { key in
                        guard let data = Data(base64Encoded: key.publicKeyBase64),
                              data.count == 32
                        else { return nil }
                        return ActrPackageTrustStore.TrustedKey(
                            publicKey: data,
                            keyId: key.id,
                            origin: "discovery:\(publisherDomain)"
                        )
                    }
                )
                logger.notice("🔐 [navigateCurrentTab] Package verified: digest matches the Realm Profile, manifest signature verified with \(publisherKeys.count) publisher key(s)")

                let manifest = try readPackageManifest(
                    fromRoot: frontDirectory, requireSignature: true, additionalTrustedKeys: publisherKeys
                )
                candidateBinding = try manifest.capabilityManifestJSON.map { capabilities in
                    try RealmServiceBinding(
                        profile: profile,
                        verifiedProfileBody: profileData,
                        front: verified.bytes,
                        serviceType: verified.serviceType,
                        trustedFrontKey: verified.trustedKey,
                        capabilitiesJSON: capabilities,
                        permissions: manifest.permissions
                    )
                }
                guard let storageIndex = tabIndex() else { return }
                let preparedStorage = try await RendererProcessPool.shared.prepareLocalStorage(
                    tabId: replacingTabId, reloadKey: tabs[storageIndex].reloadNonce,
                    origin: manifest.storageOrigin(devDirectory: nil), publisherIdentity: publisher.storageIdentity,
                    manifest: manifest.capabilityManifestJSON, permissions: manifest.permissions
                )
                if approved != nil, let candidateBinding {
                    try await candidateBinding.prepareForSwitch()
                }
                try Task.checkCancellation()
                guard try profileIsCurrent() else { return }
                try await RealmProfileStore.withPublisherKeys([
                    .init(publicKey: verified.trustedKey, domain: publisherDomain)
                ], database: .shared) {
                    try Task.checkCancellation()
                    guard try profileIsCurrent() else { return }
                    guard let publisherIdentity = publisher.storageIdentity else {
                        throw GuestLocalStorageError.InvalidPublisher(message: "Verified Publisher identity is missing.")
                    }
                    guard importACTRPackage(
                        from: cachedArchiveURL,
                        knownDigest: digest,
                        presentInstanceCreation: false,
                        requireSignature: true,
                        additionalTrustedKeys: publisherKeys,
                        publisherStorageIdentity: publisherIdentity,
                        preparedLocalStorage: preparedStorage,
                        replacingTabId: replacingTabId,
                        realmAddress: address
                    ) != nil else {
                        throw CoreError.RealmProfileInvalid(message: "Could not load the verified Front")
                    }
                    CapabilityPermissionStore.shared.clearSessionGrants(tabId: replacingTabId)
                    if let binding = candidateBinding {
                        RealmServiceBindings.shared.attach(binding, to: replacingTabId)
                        candidateBinding = nil
                    } else {
                        RealmServiceBindings.shared.remove(replacingTabId)
                    }
                    localFrontApprovals.removeValue(forKey: replacingTabId)
                    loadedRealmFronts[replacingTabId] = next
                    if let index = tabs.firstIndex(where: { $0.id == replacingTabId }) {
                        tabs[index].realmProfileSigner = (realmName, next.signingKeyId)
                    }
                    if action == .loadAvailable {
                        realmUpdateOffers.removeValue(forKey: replacingTabId)
                        deferredRealmUpdates.removeValue(forKey: replacingTabId)
                        do {
                            try saveRealmTrust(profileBody, .installedProfile, name: installedProfileName)
                        } catch {
                            logger.warning("Realm \(realmName): could not save installed Profile: \(error)")
                        }
                    } else if let offer = realmUpdateOffers[replacingTabId], let tab = tabs.first(where: { $0.id == replacingTabId }) {
                        // Import increments the tab's navigation nonce.
                        realmUpdateOffers[replacingTabId] = RealmFrontUpdateOffer(
                            tabId: offer.tabId, address: offer.address, navigationNonce: tab.reloadNonce,
                            previous: offer.previous, selection: offer.selection
                        )
                    }
                    logger.notice("Realm \(realmName): activated Front \(profile.packageType), Profile sequence \(profile.sequence), digest \(digest)")
                    objectWillChange.send()
                    persistBrowserPageSession()
                    schedulePackageCacheEviction()
                }
            } catch {
                guard !Task.isCancelled, let index = tabIndex() else { return }
                logger.warning("Realm \(realmName): Front refresh failed: \(error)")
                if case CoreError.RealmRetired = error, tabs[index].url == address {
                    unloadRealmFront(at: index)
                }
                if !background { presentRealmLinkError(error) }
            }
        }
        realmRefreshes[replacingTabId] = (refreshId, task, invite?.code)
    }

    private func realmPublisher(for actorType: String) async throws -> PublisherProfileInfo {
        guard let domain = actorType.split(separator: "/").first.map(String.init),
              let url = URL(string: "https://\(domain)/.well-known/actr") else {
            throw RealmServiceError(message: "The package type does not name a Publisher domain.")
        }
        let data = try await fetchRealmDocument(
            url,
            NSLocalizedString(
                "The publisher information was served over an insecure connection.",
                comment: "Actr publisher insecure transport error")
        )
        guard let body = String(data: data, encoding: .utf8) else {
            throw RealmServiceError(message: "The Publisher information is not valid text.")
        }
        return try await RealmProfileStore.acceptPublisher(
            body: body, domain: domain, preferences: realmPreferences, database: .shared,
            fallbackPins: { try self.readRealmTrust(.publisher, name: domain) }
        )
    }

    private func loadLocalRealmFront(
        _ front: RealmPackageInfo,
        policy: LocalRealmFront,
        accepted: RealmProfileStore.AcceptedRealm,
        profileData: Data,
        publisher: PublisherProfileInfo,
        address: String,
        tabId: UUID,
        isCurrent: @MainActor () -> Bool
    ) async throws -> UUID? {
        let profile = accepted.info
        guard let trustedKey = publisher.verificationKey(for: policy) else {
            throw RealmServiceError(message: "The local Front signing key is no longer trusted for this package.")
        }
        guard let service = profile.packages.first(where: { $0.reference == profile.primaryPackage }),
              let sourceURL = URL(string: address) else {
            throw RealmServiceError(message: "The local Front needs a selected Service.")
        }
        let existing = localFrontApprovals[tabId]
        let scope = LocalFrontScope(
            realm: profile.realmName, provider: profile.providerId, issuer: profile.issuerUrl,
            front: front, service: service, publicKey: trustedKey
        )
        let file: LocalRealmFrontFile
        let archive: Data
        do {
            if let existing, existing.scope == scope {
                file = existing.file
            } else {
                guard isCurrent(), currentTabId == tabId else { throw CancellationError() }
                let rebuild = policy.allowRebuild
                    ? "Refresh can load new builds signed by this key from the approved file."
                    : "The archive must match the digest in the Realm Profile."
                let message = """
                    \(profile.realmName) requests the local Front \(front.href).
                    \(profile.packageType) · \(policy.signingKeyId)
                    \(rebuild)
                    """
                if front.href.hasPrefix("/") {
                    let alert = NSAlert()
                    alert.alertStyle = .warning
                    alert.messageText = "Open this local Realm Front?"
                    alert.informativeText = message
                    alert.addButton(withTitle: "Allow and Open")
                    alert.addButton(withTitle: "Cancel")
                    guard alert.runModal() == .alertFirstButtonReturn, isCurrent() else {
                        throw CancellationError()
                    }
                    let approvedPath = front.href
                    file = try await Task.detached {
                        try LocalRealmFrontFile(approvedAbsolutePath: approvedPath)
                    }.value
                } else {
                    guard let packageType = UTType(filenameExtension: "actr") else {
                        throw RealmServiceError(message: "The local Front file type is unavailable.")
                    }
                    let panel = NSOpenPanel()
                    panel.title = "Choose the local Realm Front"
                    panel.message = message
                    panel.prompt = "Allow and Open"
                    panel.canChooseDirectories = false
                    panel.allowedContentTypes = [packageType]
                    guard let selectedURL = chooseLocalFrontFile(panel), isCurrent() else {
                        throw CancellationError()
                    }
                    file = try LocalRealmFrontFile(selectedURL: selectedURL)
                }
            }
            guard isCurrent() else { return nil }
            archive = try await Task.detached { try file.readArchive() }.value
        } catch let error as CocoaError where error.code == .fileReadNoSuchFile {
            let message = front.href.hasPrefix("/")
                ? "The local Front file was not found at \(front.href)."
                : "The selected local Front file was not found."
            throw RealmServiceError(message: message)
        } catch let error as CocoaError where error.code == .fileReadUnsupportedScheme {
            throw RealmServiceError(message: "Choose a regular local .actr file on a local volume without symbolic links.")
        } catch let error as CocoaError where error.code == .fileReadCorruptFile {
            throw RealmServiceError(message: "The local Front must be a nonempty regular .actr file of at most 64 MiB.")
        }
        guard isCurrent() else { return nil }

        let servicePublisher = profile.serviceType.hasPrefix(publisher.domain + "/")
            ? publisher : try await realmPublisher(for: profile.serviceType)
        let serviceKeys = servicePublisher.fixedArchiveVerificationKeys
        let staging = FileManager.default.temporaryDirectory
            .appendingPathComponent("realm-local-front-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: staging, withIntermediateDirectories: false)
        defer { try? FileManager.default.removeItem(at: staging) }
        let serviceCache = ActrPackageCache.archiveURL(forDigest: service.sha256)
        let cachedService = try? LocalRealmFrontFile(selectedURL: serviceCache).readArchive()
        let download: URL?
        let serviceBytes: Data
        if let cachedService, Self.realmArchiveDigest(cachedService) == service.sha256 {
            download = nil
            serviceBytes = cachedService
        } else {
            guard let href = URL(string: service.href) else {
                throw RealmServiceError(message: "The Service package location is invalid.")
            }
            let url = try await ActrPackageDownloadService.download(from: href, in: staging)
            download = url
            serviceBytes = try Data(contentsOf: url)
        }
        guard isCurrent() else { return nil }
        let snapshot = try await Task.detached {
            let info = try ActriumCore.verifyLocalRealmFrontPackage(
                archive: archive, trustedPublicKey: trustedKey, profile: profile,
                serviceArchive: serviceBytes, servicePublicKeys: serviceKeys
            )
            return (info, Self.realmArchiveDigest(archive))
        }.value
        guard isCurrent() else { return nil }
        if let download {
            _ = try ActrPackageCache.installArchive(at: download, digest: service.sha256, replacingRejectedArchive: true)
        }

        let (verified, digest) = snapshot
        let guest = verified.front.workloadBytes
        let stage = staging.appendingPathComponent("front.actr")
        try archive.write(to: stage, options: .atomic)
        let cached = try ActrPackageCache.installArchive(at: stage, digest: digest)
        let root = try ActrPackageCache.extractArchiveIfNeeded(cached, digest: digest) {
            try self.validateExtractedPackage(at: $0, requireSignature: true, additionalTrustedKeys: [trustedKey])
        }
        let manifest = try readPackageManifest(fromRoot: root, requireSignature: true, additionalTrustedKeys: [trustedKey])
        guard let entry = manifest.unifiedScript, let script = String(data: guest, encoding: .utf8),
              let capabilities = manifest.capabilityManifestJSON else {
            throw RealmServiceError(message: "The local Front needs a Keel Guest and capability manifest.")
        }
        let serviceType = try ActrBindings.verifyRealmFront(
            frontArchive: archive, trustedPublicKey: trustedKey,
            frontType: profile.packageType, serviceType: profile.serviceType
        )
        let binding = try RealmServiceBinding(
            profile: profile, verifiedProfileBody: profileData, front: archive,
            serviceType: serviceType, trustedFrontKey: trustedKey, capabilitiesJSON: capabilities,
            permissions: manifest.permissions
        )
        var installed = false
        defer { if !installed { Task { await binding.stop() } } }
        func requireCurrentPublisherKeys() async throws {
            try await RealmProfileStore.requirePublisherKeys([
                .init(publicKey: trustedKey, domain: publisher.domain, localFront: policy),
                .init(publicKey: verified.servicePublicKey, domain: servicePublisher.domain)
            ], database: .shared)
        }
        guard let storageIndex = tabs.firstIndex(where: { $0.id == tabId }) else { return nil }
        let preparedStorage = try await RendererProcessPool.shared.prepareLocalStorage(
            tabId: tabId, reloadKey: tabs[storageIndex].reloadNonce,
            origin: manifest.storageOrigin(devDirectory: nil), publisherIdentity: publisher.storageIdentity,
            manifest: capabilities, permissions: manifest.permissions
        )
        try await requireCurrentPublisherKeys()
        guard isCurrent() else { return nil }
        try await binding.prepareLocalFront()
        return try await RealmProfileStore.withPublisherKeys([
            .init(publicKey: trustedKey, domain: publisher.domain, localFront: policy),
            .init(publicKey: verified.servicePublicKey, domain: servicePublisher.domain)
        ], database: .shared) {
            guard isCurrent(), let index = tabs.firstIndex(where: { $0.id == tabId }) else { return nil }

            // New renderer identity prevents late messages from the old Guest
            // from using the new binding or inheriting its approvals.
            var tab = Tab(
                title: manifest.name ?? front.href, url: sourceURL,
                appId: "actrium.local-front/\(profile.realmName)/\(digest)",
                groupId: tabs[index].groupId, unifiedScript: script, guestExtractedPath: root.path,
                guestEntry: entry, capabilityManifestJSON: capabilities,
                localStorageOrigin: manifest.storageOrigin(devDirectory: nil),
                localStoragePublisherIdentity: publisher.storageIdentity, preparedLocalStorage: preparedStorage,
                contractName: manifest.contractName,
                contractVersion: manifest.contractVersion, permissions: manifest.permissions,
                leftPanelDefaultVisible: manifest.leftPanelDefaultVisible,
                rightPanelDefaultVisible: manifest.rightPanelDefaultVisible, backgroundColor: manifest.themeColor
            )
            tab.realmProfileSigner = (profile.realmName, accepted.signingKeyId)
            let position = tabSlotPositions.removeValue(forKey: tabId) ?? index
            let offer = realmUpdateOffers[tabId]
            let deferred = deferredRealmUpdates[tabId]
            releaseTabRuntime(tabId, preserveRealmLoad: true)
            if let offer {
                realmUpdateOffers[tab.id] = RealmFrontUpdateOffer(
                    tabId: tab.id, address: offer.address, navigationNonce: tab.reloadNonce,
                    previous: offer.previous, selection: offer.selection
                )
            }
            deferredRealmUpdates[tab.id] = deferred
            leftPanelVisibility[tabId] = nil
            RealmServiceBindings.shared.attach(binding, to: tab.id)
            localFrontApprovals[tab.id] = LocalFrontApproval(scope: scope, file: file)
            tabSlotPositions[tab.id] = position
            leftPanelVisibility[tab.id] = manifest.leftPanelDefaultVisible
            tabs[index] = tab
            if currentTabId == tabId {
                currentTabId = tab.id
                if actriumMode.isInFocusMode { actriumMode = .focus(tabId: tab.id) }
            }
            installed = true
            persistBrowserPageSession()
            schedulePackageCacheEviction()
            return tab.id
        }
    }

    nonisolated private static func realmArchiveDigest(_ bytes: Data) -> String {
        SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
    }

    /// Transport checks shared by every discovery fetch.
    private static func requireSuccess(_ response: URLResponse, insecureMessage: String) throws {
        guard let http = response as? HTTPURLResponse,
              (200 ..< 300).contains(http.statusCode) else {
            let status = (response as? HTTPURLResponse)?.statusCode ?? -1
            throw NSError(
                domain: "ActrDiscovery", code: 20,
                userInfo: [NSLocalizedDescriptionKey: String(
                    format: NSLocalizedString(
                        "The request failed (HTTP %ld).",
                        comment: "Actr fetch failure"),
                    status)])
        }
        guard response.url?.scheme?.lowercased() == "https" else {
            throw NSError(
                domain: "ActrDiscovery", code: 21,
                userInfo: [NSLocalizedDescriptionKey: insecureMessage])
        }
    }

    /// Apply the same signed digest, publisher and Service checks to cached and downloaded Fronts.
    private static func verifyRealmFront(at url: URL, profile: RealmProfileInfo, publisherKeys: [Data]) throws -> VerifiedRealmFront {
        guard try url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile == true else {
            throw CoreError.RealmProfileInvalid(message: "The Front archive is unavailable.")
        }
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        let limit = ActrPackageDownloadLimits.maxBytes
        let bytes = try handle.read(upToCount: Int(limit) + 1) ?? Data()
        guard bytes.count <= Int(limit) else { throw ActrPackageDownloadError.sizeExceeded(maxBytes: limit) }
        guard let key = publisherKeys.first(where: {
            (try? ActriumCore.verifyRealmFrontPackage(archive: bytes, trustedPublicKey: $0, profile: profile)) != nil
        }) else {
            throw NSError(
                domain: "ActrDiscovery", code: 26,
                userInfo: [NSLocalizedDescriptionKey: NSLocalizedString(
                    "The Front package could not be verified.", comment: "Actr Front verification error")])
        }
        let serviceType = try ActrBindings.verifyRealmFront(
            frontArchive: bytes, trustedPublicKey: key, frontType: profile.packageType, serviceType: profile.serviceType
        )
        return VerifiedRealmFront(bytes: bytes, trustedKey: key, serviceType: serviceType)
    }

    private func readRealmTrust(_ kind: RealmFrontTrustStore.Kind, name: String) throws -> String? {
        try RealmFrontTrustStore.read(kind, name: name, directory: realmTrustDirectory)
    }

    private func saveRealmTrust(_ value: String, _ kind: RealmFrontTrustStore.Kind, name: String) throws {
        try RealmFrontTrustStore.save(value, kind, name: name, directory: realmTrustDirectory)
    }

    /// Coalesce background fetches without sharing a tab's trust or approval state.
    private func fetchRealmUpdateDocument(from url: URL, background: Bool, insecureMessage: String) async throws -> Data {
        guard background else {
            return try await fetchRealmDocument(url, insecureMessage)
        }
        if let request = Self.backgroundRealmDocuments[url] { return try await request.value }
        let fetch = fetchRealmDocument
        let request = Task { try await fetch(url, insecureMessage) }
        Self.backgroundRealmDocuments[url] = request
        defer { Self.backgroundRealmDocuments.removeValue(forKey: url) }
        return try await request.value
    }

    /// Small bounded fetch for discovery documents (authority descriptor,
    /// Realm Profile, Publisher Profile): 2xx + HTTPS via `requireSuccess`,
    /// and a size cap so a hostile endpoint cannot exhaust memory.
    private static func fetchBoundedDocument(
        from url: URL,
        insecureMessage: String
    ) async throws -> Data {
        var request = URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData, timeoutInterval: 30)
        request.setValue("no-cache", forHTTPHeaderField: "Cache-Control")
        let (data, response) = try await URLSession.shared.data(for: request)
        try requireSuccess(response, insecureMessage: insecureMessage)
        let maxBytes = 4 * 1024 * 1024
        guard data.count <= maxBytes else {
            throw NSError(
                domain: "ActrDiscovery", code: 26,
                userInfo: [NSLocalizedDescriptionKey: NSLocalizedString(
                    "The discovery document exceeds the size limit.",
                    comment: "Actr discovery document size error")])
        }
        return data
    }

    /// Surface a rejected actr:// link with variant-specific copy.
    private func presentRealmLinkError(_ error: Error) {
        let message: String
        switch error {
        case GuestLocalStorageError.IdentityChanged:
            message = NSLocalizedString(
                "The Publisher's identity changed without a trusted connection to its previous identity. Saved app data has not been shared.",
                comment: "Publisher identity change error")
        case GuestLocalStorageError.InvalidPublisher:
            message = NSLocalizedString(
                "The Publisher information could not be verified. The application was not opened.",
                comment: "Publisher verification error")
        case let error as GuestStorageKeyError:
            message = error.localizedDescription
        case GuestLocalStorageError.DatabaseFailure,
             GuestLocalStorageError.DatabaseUnlockFailed,
             GuestLocalStorageError.EncryptionUnavailable:
            message = NSLocalizedString(
                "Saved Publisher verification information could not be read or updated. Existing app data has been preserved.",
                comment: "Publisher trust storage error")
        case GuestStorageDatabaseError.busy:
            message = NSLocalizedString(
                "Storage is busy. Try opening the application again shortly.",
                comment: "Publisher storage busy error")
        case CoreError.AuthorityDescriptorInvalid:
            message = NSLocalizedString(
                "The authority information for this domain is invalid.",
                comment: "Actr authority descriptor error")
        case CoreError.RealmProfileInvalid:
            message = NSLocalizedString(
                "The Realm Profile for this instance is invalid.",
                comment: "Actr realm profile error")
        case CoreError.RealmRetired:
            message = NSLocalizedString(
                "This Realm instance has been retired.",
                comment: "Actr realm retired error")
        case CoreError.UriSchemeInvalid:
            message = NSLocalizedString(
                "Only actr:// links are supported here.",
                comment: "Actr URI scheme error")
        case CoreError.UriDomainInvalid:
            message = NSLocalizedString(
                "The domain in this link is not a valid Realm domain.",
                comment: "Actr URI domain error")
        case CoreError.UriLabelInvalid:
            message = NSLocalizedString(
                "The RealmId or ActorId in this link is missing or invalid.",
                comment: "Actr URI label error")
        case CoreError.UriLegacyForm:
            message = NSLocalizedString(
                "This link uses an outdated address format that is no longer supported.",
                comment: "Actr URI legacy form error")
        case CoreError.UriLocalRealmForeign:
            message = NSLocalizedString(
                "This local Realm belongs to a different installation.",
                comment: "Actr URI foreign local Realm error")
        case let error as RealmServiceError:
            message = error.localizedDescription
        default:
            message = "\(error)"
        }
        presentRealmLinkMessage(message)
    }

    private func presentRealmLinkMessage(_ message: String) {
        let alert = NSAlert()
        alert.messageText = NSLocalizedString("Invalid Actr Link", comment: "Actr URI error title")
        alert.informativeText = message
        alert.alertStyle = .warning
        alert.runModal()
    }

    /// Checks whether input is a local file path
    func isLocalFilePath(_ input: String) -> Bool {
        // actro://... may itself point at an .actr archive. It must be parsed as
        // an application URL instead of being treated as a literal filesystem path.
        if ActriumAppURLScheme.hasSupportedPrefix(input) {
            return false
        }
        let scheme = URLComponents(string: input)?.scheme?.lowercased()
        if scheme == "actr" || scheme == "https" || scheme == "http" {
            return false
        }
        // Here. / The first is the absolute path.
        if input.hasPrefix("/") {
            return true
        }
        // Here. ~ Start with a user directory
        if input.hasPrefix("~") {
            return true
        }
        // Here. file:// Start
        if input.hasPrefix("file://") {
            return true
        }
        // Here. .actr End
        if input.hasSuffix(".actr") {
            return true
        }
        return false
    }

    /// Parsing URL String
    /// - If about:/http/https/actrium, Directly decipher
    /// - If it looks like a domain name (includes) . and does not contain spaces) add https:// Prefix
    func parseURL(_ input: String) -> URL? {
        // It's complete. URL (http/https/about/actrium)
        let scheme = URLComponents(string: input)?.scheme?.lowercased()
        if scheme == "http" || scheme == "https" ||
           input.hasPrefix("about:") ||
           ActriumAppURLScheme.hasSupportedPrefix(input) {
            return URL(string: input)
        }

        // Looks like a domain name (includes) . and does not contain spaces)
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
        guard let tabId = currentTabId,
              let holder = WebViewHolderManager.shared.existingHolder(for: tabId), holder.canGoBack else { return }
        webTabDidActivateLink(tabId)
        holder.goBack()
    }

    /// Navigate forward in web history
    func goForward() {
        guard let tabId = currentTabId,
              let holder = WebViewHolderManager.shared.existingHolder(for: tabId), holder.canGoForward else { return }
        webTabDidActivateLink(tabId)
        holder.goForward()
    }

    /// Reload the current tab (web page or .actr package)
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

        // For web content (http/https), use WebView's reload
        if sourceURL.scheme == "http" || sourceURL.scheme == "https" {
            logger.notice("🔄 [reloadCurrentTab] Web Content, Call WebView reload")
            WebViewHolderManager.shared.existingHolder(for: id)?.reload()
            return
        }

        if sourceURL.scheme?.lowercased() == "actr",
           let info = try? ActriumCore.parseActrUri(
               uri: sourceURL.absoluteString, installationId: InstallationIdProvider.getOrCreate()
           ), !info.isLocal {
            if localFrontApprovals[id] != nil {
                handleResolvedRealmLink(info, input: sourceURL.absoluteString, tabIndex: index)
            } else {
                navigateCurrentTab(to: sourceURL.absoluteString)
            }
            return
        }

        logger.debug("🔄 [reloadCurrentTab] Local package, read again")

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
            // Package archive mode — digest-addressed cache (15-B B2, 05 §15).
            do {
                let digest = try ActrPackageDownloadService.sha256FileSync(at: resolvedURL)
                let archiveURL = try ActrPackageCache.installArchive(at: resolvedURL, digest: digest)
                rootDir = try ActrPackageCache.extractArchiveIfNeeded(archiveURL, digest: digest) {
                    try self.validateExtractedPackage(at: $0)
                }
            } catch {
                logger.error(" [reloadCurrentTab] Depression failed:\(error.localizedDescription)")
                return
            }
        }

        do {
            let manifestInfo = try readPackageManifest(fromRoot: rootDir, requireSignature: !isDirectory.boolValue)

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
            tabs[index].guestExtractedPath = isDirectory.boolValue ? nil : rootDir.path
            tabs[index].guestEntry = !isDirectory.boolValue && unifiedScript != nil
                ? manifestInfo.unifiedScript
                : nil
            tabs[index].appId = manifestInfo.appId
            tabs[index].contractName = manifestInfo.contractName
            tabs[index].contractVersion = manifestInfo.contractVersion
            tabs[index].permissions = manifestInfo.permissions
            tabs[index].capabilityManifestJSON = manifestInfo.capabilityManifestJSON
            tabs[index].localStorageOrigin = manifestInfo.storageOrigin(
                devDirectory: isDirectory.boolValue ? resolvedURL : nil
            )
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
            logger.error(" [reloadCurrentTab] Synchronising: \(error.localizedDescription)")
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
        // Focus Use in Mode focus tabId, or use currentTabId
        let id: UUID
        if case .focus(let focusTabId) = actriumMode {
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

    private func schedulePackageCacheEviction() {
        guard let appDelegate = NSApplication.shared.delegate as? AppDelegate else { return }
        let protectedDigests = appDelegate.protectedPackageDigests()
        let cacheRootOverride = ActrPackageCache.rootOverrideForTesting
        Task.detached(priority: .utility) {
            ActrPackageCache.$rootOverrideForTesting.withValue(cacheRootOverride) {
                _ = ActrPackageCache.evictIfNeeded(protectedDigests: protectedDigests)
            }
        }
    }

    /// From Local .actr Package to create a new tab and load basic information.
    /// - Parameter presentInstanceCreation: archive imports present the instance
    ///   creation confirm sheet after rendering (15-B B6); restart restore passes
    ///   false — the instance already exists.
    /// - Parameter requireSignature: verify manifest.sig before importing an
    ///   archive (15-B B3). Pass false only for the download funnel while the
    ///   publisher-key verification step is still unwired; payload integrity is
    ///   still checked by the manifest reader.
    /// - Returns: the newly created runtime tab, or nil when the import failed.
    @discardableResult
    func importACTRPackage(
        from fileURL: URL,
        knownDigest: String? = nil,
        presentInstanceCreation: Bool = true,
        requireSignature: Bool = true,
        additionalTrustedKeys: [Data] = [],
        publisherStorageIdentity: String? = nil,
        preparedLocalStorage: PreparedLocalStorage? = nil,
        replacingTabId: UUID? = nil,
        realmAddress: URL? = nil
    ) -> Tab? {
        let resolvedURL = fileURL.standardizedFileURL
        guard FileManager.default.fileExists(atPath: resolvedURL.path) else {
            logger.error(" Not found .actr Documents:\(resolvedURL.path)")
            return nil
        }

        // Check if it's a directory (dev mode) or archive
        var isDirectory: ObjCBool = false
        FileManager.default.fileExists(atPath: resolvedURL.path, isDirectory: &isDirectory)

        let rootDir: URL
        var packageDigest: String?
        var cachedArchiveURL: URL?

        if isDirectory.boolValue {
            // Directory mode - use directly
            rootDir = resolvedURL
        } else {
            // Package archive mode — digest-addressed cache (15-B B2, 05 §15):
            // archive kept at cache/packages/sha256/<digest>/package.actr,
            // extraction at cache/extracted/sha256/<digest>/ (reused on reopen).
            do {
                let digest = try knownDigest ?? ActrPackageDownloadService.sha256FileSync(at: resolvedURL)
                let archiveURL = try ActrPackageCache.installArchive(at: resolvedURL, digest: digest)
                rootDir = try ActrPackageCache.extractArchiveIfNeeded(archiveURL, digest: digest) {
                    try self.validateExtractedPackage(
                        at: $0,
                        requireSignature: requireSignature,
                        additionalTrustedKeys: additionalTrustedKeys
                    )
                }
                packageDigest = digest
                cachedArchiveURL = archiveURL
            } catch {
                logger.error(" .actr Loading failed:\(error.localizedDescription)")
                return nil
            }
        }

        do {
            let manifestInfo = try readPackageManifest(
                fromRoot: rootDir,
                requireSignature: !isDirectory.boolValue && requireSignature,
                additionalTrustedKeys: additionalTrustedKeys
            )
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
                let newTab = appendRuntimeTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    appId: manifestInfo.appId,
                    unifiedScript: unified,
                    // 15-B B4: for package archives, hand the renderer the
                    // digest-addressed extraction dir + entry so it can read the
                    // verified bytes directly.
                    guestExtractedPath: isDirectory.boolValue ? nil : rootDir.path,
                    guestEntry: isDirectory.boolValue ? nil : manifestInfo.unifiedScript,
                    contractName: manifestInfo.contractName,
                    contractVersion: manifestInfo.contractVersion,
                    permissions: manifestInfo.permissions,
                    capabilityManifestJSON: manifestInfo.capabilityManifestJSON,
                    localStorageOrigin: manifestInfo.storageOrigin(
                        devDirectory: isDirectory.boolValue ? resolvedURL : nil
                    ),
                    localStoragePublisherIdentity: publisherStorageIdentity,
                    preparedLocalStorage: preparedLocalStorage,
                    leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                    rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible,
                    themeColor: manifestInfo.themeColor,
                    // Archive imports: history records only the final instance
                    // address after creation (04 §4, 15-B B6).
                    recordHistoryVisit: isDirectory.boolValue,
                    replacingTabId: replacingTabId,
                    realmAddress: realmAddress
                )
                // 15-B B6: after rendering a package archive, offer instance
                // creation (confirm page → serviceCreate → URI replace).
                if presentInstanceCreation, !isDirectory.boolValue,
                   let packageDigest, let cachedArchiveURL {
                    offerInstanceCreation(
                        for: newTab,
                        packageTitle: tabTitle,
                        packagePath: cachedArchiveURL.path,
                        packageDigest: packageDigest,
                        displayName: manifestInfo.name,
                        permissions: manifestInfo.permissions
                    )
                }
                schedulePackageCacheEviction()
                return newTab
            }
            // Legacy: Double Engine Structure
            else if leftPanelScript != nil || rightPanelScript != nil {
                logger.debug("🔄 [BrowserViewModel] Use Legacy Double Engine Structure")
                let newTab = appendRuntimeTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    appId: manifestInfo.appId,
                    leftPanelScript: leftPanelScript,
                    rightPanelScript: rightPanelScript,
                    guestExtractedPath: isDirectory.boolValue ? nil : rootDir.path,
                    contractName: manifestInfo.contractName,
                    contractVersion: manifestInfo.contractVersion,
                    permissions: manifestInfo.permissions,
                    capabilityManifestJSON: manifestInfo.capabilityManifestJSON,
                    leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                    rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible,
                    themeColor: manifestInfo.themeColor,
                    replacingTabId: replacingTabId,
                    realmAddress: realmAddress
                )
                schedulePackageCacheEviction()
                return newTab
            } else if let script = guestScript {
                // Old architecture: one compatible guest.js
                logger.debug("🔄 [BrowserViewModel] Use compatible mode (guestScript)")
                let newTab = appendRuntimeTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    appId: manifestInfo.appId,
                    guestScript: script,
                    guestExtractedPath: isDirectory.boolValue ? nil : rootDir.path,
                    contractName: manifestInfo.contractName,
                    contractVersion: manifestInfo.contractVersion,
                    permissions: manifestInfo.permissions,
                    capabilityManifestJSON: manifestInfo.capabilityManifestJSON,
                    themeColor: manifestInfo.themeColor,
                    replacingTabId: replacingTabId,
                    realmAddress: realmAddress
                )
                schedulePackageCacheEviction()
                return newTab
            } else {
                if realmAddress != nil { return nil }
                // No script, display application information page
                let html = renderACTRInfoHTML(
                    name: manifestInfo.name ?? "Unknown App",
                    version: manifestInfo.version ?? "0.0.0",
                    description: manifestInfo.description ?? "No description provided.",
                    sourcePath: resolvedURL.path
                )
                let newTab = appendRuntimeTab(
                    title: tabTitle,
                    sourceURL: resolvedURL,
                    appId: manifestInfo.appId,
                    htmlContent: html,
                    guestExtractedPath: isDirectory.boolValue ? nil : rootDir.path,
                    replacingTabId: replacingTabId
                )
                schedulePackageCacheEviction()
                return newTab
            }
        } catch {
            if realmAddress != nil {
                logger.error("Verified Front could not be loaded: \(error.localizedDescription)")
                return nil
            }
            // 15-B B8: signature failures get a dedicated, actionable error page
            // (04 §7 PACKAGE_SIGNATURE_INVALID) — never a generic "cannot connect".
            if let signatureError = error as? AppleActrPackageSignatureError {
                logger.error("🔒 [importACTRPackage] \(signatureError.stableCode): \(signatureError.localizedDescription)")
                let errorHTML = renderSignatureErrorHTML(signatureError, sourcePath: resolvedURL.path)
                return appendRuntimeTab(
                    title: resolvedURL.deletingPathExtension().lastPathComponent,
                    sourceURL: resolvedURL,
                    htmlContent: errorHTML,
                    guestExtractedPath: isDirectory.boolValue ? nil : rootDir.path,
                    replacingTabId: replacingTabId
                )
            }
            logger.error(" .actr Loading failed:\(error.localizedDescription)")
            let fallbackHTML = renderACTRInfoHTML(
                name: resolvedURL.deletingPathExtension().lastPathComponent,
                version: "unknown",
                description: "Loading failed:\(error.localizedDescription)",
                sourcePath: resolvedURL.path
            )
            return appendRuntimeTab(
                title: resolvedURL.deletingPathExtension().lastPathComponent,
                sourceURL: resolvedURL,
                htmlContent: fallbackHTML,
                guestExtractedPath: isDirectory.boolValue ? nil : rootDir.path,
                replacingTabId: replacingTabId
            )
        }
    }

    /// Dedicated signature-verification error page (15-B B8, 04 §7): shows the
    /// stable code and an actionable suggestion per failure mode.
    private func renderSignatureErrorHTML(_ error: AppleActrPackageSignatureError, sourcePath: String) -> String {
        let suggestion: String
        switch error {
        case .signatureMissing:
            suggestion = "The package is missing its manifest.sig signature. Re-download it; for a local package, re-pack it with actr pkg pack."
        case .noTrustedKeys:
            suggestion = "No trusted public keys are available. Run actr pkg keygen to generate a key, or configure trusted public keys in Actrium/trusted-keys.json and retry."
        case .signatureInvalid:
            suggestion = "Signature verification failed: the package may be tampered with or signed by another key. Re-download it or contact the publisher."
        }
        return """
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="utf-8">
            <style>
                body {
                    font-family: -apple-system, BlinkMacSystemFont, sans-serif;
                    padding: 48px; margin: 0; min-height: 100vh;
                    color: #e6e8ee;
                    background: linear-gradient(135deg, #1b0f0f 0%, #111827 60%, #0b1220 100%);
                }
                .card {
                    max-width: 640px; margin: 0 auto; padding: 32px;
                    background: rgba(255, 255, 255, 0.04);
                    border: 1px solid rgba(239, 108, 96, 0.35);
                    border-radius: 16px;
                }
                .code {
                    display: inline-block; padding: 6px 10px; margin-bottom: 14px;
                    border-radius: 10px; background: rgba(239, 108, 96, 0.14);
                    color: #f8b4ae; font-family: monospace; font-size: 12px;
                }
                h1 { margin: 0 0 10px 0; font-size: 24px; }
                .message { color: #d1d5db; line-height: 1.6; margin-bottom: 18px; }
                .suggestion {
                    line-height: 1.6; color: #e5e7eb;
                    background: rgba(255,255,255,0.05); padding: 14px 16px;
                    border-radius: 10px; margin-bottom: 16px;
                }
                .path {
                    font-family: 'Menlo', ui-monospace, monospace; font-size: 11px;
                    color: #9ca3af; word-break: break-all;
                }
            </style>
        </head>
        <body>
            <div class="card">
                <div class="code">PACKAGE_SIGNATURE_INVALID</div>
                <h1>Package signature verification failed</h1>
                <div class="message">\(error.localizedDescription)</div>
                <div class="suggestion">\(suggestion)</div>
                <div class="path">\(escapeHTML(sourcePath))</div>
            </div>
        </body>
        </html>
        """
    }

    // MARK: - Restart Restore (15-B B7)
    // Session tabs (http/https + actr:// instance + actro:// package) are
    // persisted by BrowserDataRepository.saveSession; on launch the session is
    // rebuilt, actr:// addresses rebind via serviceRecover with the same ActorId.

    private var didRestoreSession = false

    /// One-shot launch restore; called from AppDelegate after startup.
    func restoreSessionIfNeeded(
        loadSession: @MainActor () -> BrowserDataRepository.RestoredSession? = {
            BrowserDataRepository.shared.restoreSession()
        }
    ) {
        guard !didRestoreSession else { return }
        didRestoreSession = true
        guard let session = loadSession(), !session.tabs.isEmpty else { return }
        logger.notice("🔄 [RestoreSession] restoring \(session.tabs.count) session tab(s)")

        // The persisted session is the source of truth on launch: drop the
        // placeholder Start Page created in init() so restoring does not leave
        // an extra blank tab next to the restored ones.
        if tabs.count == 1, let only = tabs.first,
           only.systemTabType == .startPage, only.url == nil {
            tabs.removeAll()
            tabSlotPositions.removeAll()
            currentTabId = nil
        }

        var restoredTabIds: [UUID] = []
        var activeTabId: UUID?

        for (index, sessionTab) in session.tabs.enumerated() {
            guard let url = URL(string: sessionTab.address) else { continue }
            let tabId: UUID?
            switch url.scheme?.lowercased() {
            case "http", "https":
                // Web tab: plain URL restore.
                let tab = Tab(title: sessionTab.title, url: url)
                tabs.append(tab)
                tabSlotPositions[tab.id] = tabs.count - 1
                restoredTabIds.append(tab.id)
                tabId = tab.id
            case "actr":
                guard let address = ActrLinkInvite.strippingInviteArgs(from: url.absoluteString),
                      let realmURL = URL(string: address) else { continue }
                if let info = try? ActriumCore.parseActrUri(
                    uri: address, installationId: InstallationIdProvider.getOrCreate()
                ), !info.isLocal, let realm = info.realmId,
                   redeemedRealms.contains("\(realm)@\(info.domain)") {
                    let tab = Tab(title: sessionTab.title, id: sessionTab.id, url: realmURL)
                    tabs.append(tab)
                    tabSlotPositions[tab.id] = tabs.count - 1
                    restoredTabIds.append(tab.id)
                    tabId = tab.id
                    deferredRealmLinks[tab.id] = info
                } else {
                    tabId = nil
                    restoreInstanceTab(address: realmURL, title: sessionTab.title, isActiveTab: index == session.activeTabIndex)
                }
            case "actro":
                // Package address: reuse the existing package load chain.
                let path = actroSourcePath(for: url)
                if let path, FileManager.default.fileExists(atPath: path) {
                    let tab = importACTRPackage(
                        from: URL(fileURLWithPath: path),
                        presentInstanceCreation: false
                    )
                    if let tab {
                        restoredTabIds.append(tab.id)
                        tabId = tab.id
                    } else {
                        tabId = nil
                    }
                } else {
                    tabId = nil
                }
            default:
                tabId = nil
            }
            if index == session.activeTabIndex {
                activeTabId = tabId
            }
        }

        // Focus the persisted active tab (fall back to the last restored tab).
        if activeTabId == nil {
            activeTabId = restoredTabIds.last
        }
        if let activeTabId {
            currentTabId = activeTabId
            actriumMode = .focus(tabId: activeTabId)
        }
        if !restoredTabIds.isEmpty {
            persistBrowserPageSession()
        }
        objectWillChange.send()
    }

    /// actro://<bundleId> resolves via AppRepository; actro:///path is a direct path.
    private func actroSourcePath(for url: URL) -> String? {
        if let bundleId = ActriumAppURLScheme.bundleId(from: url) {
            return appRepository.getByBundleId(bundleId)?.sourcePath
        }
        return url.path
    }

    /// Recovers one actr:// instance tab: serviceRecover (same ActorId) →
    /// re-render from the digest cache → restore the instance address.
    private func restoreInstanceTab(address: URL, title: String, isActiveTab: Bool) {
        // `URL.pathComponents` starts with "/" for a hierarchical URL, so the
        // address label is the last path component (04 §2).
        let label = address.lastPathComponent
        guard !label.isEmpty,
              let actorId = serviceInstanceStore.resolveActorId(byAddressLabel: label),
              let expectedDigest = serviceInstanceStore.knownDigest(actorId: actorId)
        else {
            logger.error("❌ [RestoreSession] instance record missing for \(address.absoluteString) (SERVICE_NOT_FOUND)")
            return
        }
        Task { [weak self] in
            guard let self else { return }
            do {
                let handle = try await self.serviceInstanceStore.serviceRecover(
                    actorId: actorId,
                    expectedPackageDigest: expectedDigest
                )
                let archiveURL = ActrPackageCache.archiveURL(forDigest: handle.packageDigest)
                guard FileManager.default.fileExists(atPath: archiveURL.path) else {
                    logger.error("❌ [RestoreSession] package cache miss (digest \(handle.packageDigest.prefix(12))…)")
                    return
                }
                guard let tab = self.importACTRPackage(
                    from: archiveURL,
                    knownDigest: handle.packageDigest,
                    presentInstanceCreation: false
                ) else { return }
                // Restore the instance address (not the actro:// package address).
                if let index = self.tabs.firstIndex(where: { $0.id == tab.id }) {
                    self.tabs[index].url = address
                    self.tabs[index].title = title
                }
                if isActiveTab {
                    self.currentTabId = tab.id
                    self.actriumMode = .focus(tabId: tab.id)
                }
                logger.notice("✅ [RestoreSession] recovered instance \(actorId) (sha256=\(handle.packageDigest.prefix(12))…)")
                self.persistBrowserPageSession()
                self.objectWillChange.send()
            } catch {
                logger.error("❌ [RestoreSession] recover failed: \(error.localizedDescription)")
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
                // System app type takes priority (no scripts needed)
                if let systemType = manifestInfo.systemAppType {
                    logger.notice(" [loadFromDevDirectory] System Applications: \(tabTitle) (type: \(systemType.rawValue))")
                    self?.appendSystemTab(
                        title: tabTitle,
                        systemType: systemType,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible
                    )
                } else if let unified = unifiedScript {
                    self?.appendRuntimeTab(
                        title: tabTitle,
                        sourceURL: directoryURL,
                        appId: manifestInfo.appId,
                        unifiedScript: unified,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        capabilityManifestJSON: manifestInfo.capabilityManifestJSON,
                        localStorageOrigin: manifestInfo.storageOrigin(devDirectory: directoryURL),
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible,
                        themeColor: manifestInfo.themeColor
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
                        capabilityManifestJSON: manifestInfo.capabilityManifestJSON,
                        leftPanelDefaultVisible: manifestInfo.leftPanelDefaultVisible,
                        rightPanelDefaultVisible: manifestInfo.rightPanelDefaultVisible,
                        themeColor: manifestInfo.themeColor
                    )
                } else if let script = guestScript {
                    self?.appendRuntimeTab(
                        title: tabTitle,
                        sourceURL: directoryURL,
                        appId: manifestInfo.appId,
                        guestScript: script,
                        contractName: manifestInfo.contractName,
                        contractVersion: manifestInfo.contractVersion,
                        permissions: manifestInfo.permissions,
                        capabilityManifestJSON: manifestInfo.capabilityManifestJSON,
                        themeColor: manifestInfo.themeColor
                    )
                } else {
                    logger.warning(" [loadFromDevDirectory] No script file found")
                }
            }
        } catch {
            logger.error(" Development directory load failed:\(error.localizedDescription)")
        }
    }

    /// Register an Actrium app in the database
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
        // Check if already registered
        if let existing = appRepository.getByBundleId(bundleId) {
            // Update existing entry
            var updated = existing
            updated.sourcePath = sourcePath
            if let v = version { updated.version = v }
            if let d = description { updated.description = d }
            if let i = icon { updated.icon = i }
            appRepository.update(updated)
            logger.notice("📱 [registerApp] Update application: \(bundleId)")
        } else {
            // Create new entry
            let app = InstalledActriumApp(
                name: name,
                bundleId: bundleId,
                version: version,
                description: description,
                sourceType: sourceType,
                sourcePath: sourcePath,
                icon: icon
            )
            appRepository.add(app)
            logger.notice("📱 [registerApp] Register new applications: \(bundleId) -> \(sourcePath)")
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
        logger.notice(" Loaded system applications:\(title)")
        persistBrowserPageSession()
    }

    @discardableResult
    private func appendRuntimeTab(
        title: String,
        sourceURL: URL,
        appId: String? = nil,
        htmlContent: String? = nil,
        guestScript: String? = nil,
        leftPanelScript: String? = nil,
        rightPanelScript: String? = nil,
        unifiedScript: String? = nil,
        guestExtractedPath: String? = nil,
        guestEntry: String? = nil,
        contractName: String? = nil,
        contractVersion: Int? = nil,
        permissions: [String] = [],
        capabilityManifestJSON: String? = nil,
        localStorageOrigin: String? = nil,
        localStoragePublisherIdentity: String? = nil,
        preparedLocalStorage: PreparedLocalStorage? = nil,
        leftPanelDefaultVisible: Bool = true,
        rightPanelDefaultVisible: Bool = false,
        themeColor: String? = nil,
        recordHistoryVisit: Bool = true,
        replacingTabId: UUID? = nil,
        realmAddress: URL? = nil
    ) -> Tab {
        // Use actro:// URL for History and address bar
        let appURL = realmAddress ?? ActriumAppURLScheme.appURL(filePath: sourceURL.path) ?? sourceURL

        // Same-tab replace (actr:// discovery funnel, 15-B B6): update the
        // originating tab in place so the address bar replaces instead of a
        // second tab appearing. Falls through to append when the tab is gone.
        if let replacingTabId,
           let index = tabs.firstIndex(where: { $0.id == replacingTabId }) {
            tabs[index].title = title
            tabs[index].url = appURL
            tabs[index].appId = appId
            tabs[index].legacyActorBridge = ActriumApp.isLegacyActorBridgePackage(sourceURL)
            tabs[index].htmlContent = htmlContent
            tabs[index].systemTabType = nil
            tabs[index].guestScript = guestScript
            tabs[index].leftPanelScript = leftPanelScript
            tabs[index].rightPanelScript = rightPanelScript
            tabs[index].unifiedScript = unifiedScript
            tabs[index].guestExtractedPath = guestExtractedPath
            tabs[index].guestEntry = guestEntry
            tabs[index].contractName = contractName
            tabs[index].contractVersion = contractVersion
            tabs[index].permissions = permissions
            tabs[index].capabilityManifestJSON = capabilityManifestJSON
            tabs[index].localStorageOrigin = localStorageOrigin
            tabs[index].localStoragePublisherIdentity = localStoragePublisherIdentity
            tabs[index].preparedLocalStorage = preparedLocalStorage
            tabs[index].leftPanelDefaultVisible = leftPanelDefaultVisible
            tabs[index].rightPanelDefaultVisible = rightPanelDefaultVisible
            if let themeColor { tabs[index].backgroundColor = themeColor }
            leftPanelVisibility[tabs[index].id] = leftPanelDefaultVisible
            // Force the renderer to pick up the replaced content.
            tabs[index].reloadNonce &+= 1
            if realmAddress == nil { currentTabId = tabs[index].id }

            logger.notice(" Loaded .actr (replaced tab): \(title)")
            if recordHistoryVisit {
                historyVM?.recordVisit(tab: tabs[index])
            }
            objectWillChange.send()
            persistBrowserPageSession()
            return tabs[index]
        }

        let newTab = Tab(
            title: title,
            url: appURL,
            appId: appId,
            legacyActorBridge: ActriumApp.isLegacyActorBridgePackage(sourceURL),
            htmlContent: htmlContent,
            guestScript: guestScript,
            leftPanelScript: leftPanelScript,
            rightPanelScript: rightPanelScript,
            unifiedScript: unifiedScript,
            guestExtractedPath: guestExtractedPath,
            guestEntry: guestEntry,
            capabilityManifestJSON: capabilityManifestJSON,
            localStorageOrigin: localStorageOrigin,
            localStoragePublisherIdentity: localStoragePublisherIdentity,
            preparedLocalStorage: preparedLocalStorage,
            contractName: contractName,
            contractVersion: contractVersion,
            permissions: permissions,
            leftPanelDefaultVisible: leftPanelDefaultVisible,
            rightPanelDefaultVisible: rightPanelDefaultVisible,
            backgroundColor: themeColor
        )
        logger.debug("📝 [appendRuntimeTab] Create New Tab: \(title), ID=\(newTab.id.uuidString.prefix(8)), bg=\(newTab.backgroundColor)")
        tabs.append(newTab)
        currentTabId = newTab.id
        tabSlotPositions[newTab.id] = tabs.count - 1
        leftPanelVisibility[newTab.id] = leftPanelDefaultVisible

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

        // Record History visit for ACTR app. Package-archive imports pass
        // false here: after instance creation the URI is replaced with the
        // instance address and only that final address is recorded (04 §4,
        // 15-B B6) — history keeps only the final address.
        if recordHistoryVisit {
            historyVM?.recordVisit(tab: newTab)
        }
        persistBrowserPageSession()
        return newTab
    }

    private func readPackageManifest(
        fromRoot root: URL,
        requireSignature: Bool = false,
        additionalTrustedKeys: [Data] = []
    ) throws -> ActrPackageManifestInfo {
        try ActrPackageManifestReader.read(
            fromRoot: root,
            requireSignature: requireSignature,
            additionalTrustedKeys: additionalTrustedKeys
        )
    }

    /// Escapes text interpolated into the static HTML error/info pages.
    private func escapeHTML(_ text: String) -> String {
        text
            .replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
            .replacingOccurrences(of: "\"", with: "&quot;")
            .replacingOccurrences(of: "'", with: "&#39;")
    }

    /// Validates a staging extraction before it enters the digest cache
    /// (05 §15): manifest signature (actr-package/2) plus per-file SHA-256
    /// integrity. Throws on failure so the cache never keeps unverified bytes.
    private func validateExtractedPackage(
        at root: URL,
        requireSignature: Bool = true,
        additionalTrustedKeys: [Data] = []
    ) throws {
        _ = try readPackageManifest(
            fromRoot: root,
            requireSignature: requireSignature,
            additionalTrustedKeys: additionalTrustedKeys
        )
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
                <h1>🧩 \(escapeHTML(name))</h1>
                <div class="meta">
                    <span class="pill">version \(escapeHTML(version))</span>
                    <span class="pill">ACTR package</span>
                </div>
                <div class="desc">\(escapeHTML(description))</div>
                <div class="path">\(escapeHTML(sourcePath))</div>
            </div>
        </body>
        </html>
        """
    }
}
extension BrowserViewModel: TabWindowTransferOwner {
    func insertTabFromCrossWindow(_ tab: Tab, at index: Int, groupId: UUID?) {
        insertTabFromCrossWindow(tab, at: index, groupId: groupId, select: true)
    }
}
