import Foundation
import Combine

private let logger = AppLogger.viewModel

/// ViewModel for managing history
@MainActor
final class HistoryViewModel: ObservableObject {
    @Published var recentItems: [HistoryItem] = []
    @Published var allItems: [HistoryItem] = []
    @Published var searchQuery: String = ""
    @Published var groupedItems: [(key: String, items: [HistoryItem])] = []

    // Tab Groups history
    @Published var closedGroups: [HistoryGroup] = []

    private let repository = HistoryRepository()
    private let browserDataRepository = BrowserDataRepository.shared
    private var cancellables = Set<AnyCancellable>()
    private let groupsKey = "closedTabGroups"

    init() {
        setupSearch()
        backfillSharedBrowserHistory()
        loadRecent()
        loadClosedGroups()
    }

    private func setupSearch() {
        $searchQuery
            .debounce(for: .milliseconds(300), scheduler: RunLoop.main)
            .removeDuplicates()
            .sink { [weak self] query in
                self?.performSearch(query)
            }
            .store(in: &cancellables)
    }

    private func backfillSharedBrowserHistory() {
        browserDataRepository.backfillHistoryIfNeeded(repository.getAll())
    }

    // MARK: - Public Methods

    /// Record a new visit from a Tab
    func recordVisit(tab: Tab) {
        guard let url = tab.url else { return }

        repository.addVisit(
            id: UUID(),
            title: tab.title.isEmpty ? "Untitled" : tab.title,
            appURL: url,
            appId: tab.resolvedAppId
        )
        browserDataRepository.recordVisit(tab: tab)

        loadRecent()
    }

    /// Record a visit from history item (when reopening)
    func recordVisit(item: HistoryItem) {
        repository.addVisit(
            id: UUID(),
            title: item.title,
            appURL: item.appURL,
            appId: item.appId
        )
        browserDataRepository.recordVisit(item: item)

        loadRecent()
    }

    /// Load recent history items
    func loadRecent(limit: Int = 10) {
        recentItems = mergedHistoryItems(
            localItems: repository.getRecent(limit: limit),
            sharedItems: browserDataRepository.loadRecentHistory(limit: limit),
            limit: limit
        )
    }

    /// Load all history items
    func loadAll() {
        if searchQuery.isEmpty {
            allItems = mergedHistoryItems(
                localItems: repository.getAll(),
                sharedItems: browserDataRepository.loadRecentHistory()
            )
        } else {
            allItems = filteredHistoryItems(query: searchQuery)
        }
        groupedItems = allItems.groupedByDate()
    }

    /// Delete a single history item
    func deleteItem(_ item: HistoryItem) {
        repository.deleteItem(id: item.id)
        browserDataRepository.removeHistory(item: item)
        loadRecent()
        loadAll()
    }

    /// Clear all history
    func clearHistory() {
        repository.clearAll()
        browserDataRepository.clearHistory()
        recentItems = []
        allItems = []
        groupedItems = []
    }

    // MARK: - Private Methods

    private func performSearch(_ query: String) {
        if query.isEmpty {
            allItems = mergedHistoryItems(
                localItems: repository.getAll(),
                sharedItems: browserDataRepository.loadRecentHistory()
            )
        } else {
            allItems = filteredHistoryItems(query: query)
        }
        groupedItems = allItems.groupedByDate()
    }

    private func filteredHistoryItems(query: String) -> [HistoryItem] {
        let localItems = repository.search(query: query)
        let sharedItems = browserDataRepository.loadRecentHistory().filter { item in
            item.title.localizedCaseInsensitiveContains(query) ||
            (item.appURL?.absoluteString.localizedCaseInsensitiveContains(query) ?? false)
        }
        return mergedHistoryItems(localItems: localItems, sharedItems: sharedItems)
    }

    private func mergedHistoryItems(
        localItems: [HistoryItem],
        sharedItems: [HistoryItem],
        limit: Int? = nil
    ) -> [HistoryItem] {
        var merged = localItems
        var knownBrowserAddresses = Set(
            localItems.compactMap { item -> String? in
                guard let url = item.appURL, BrowserDataRepository.isBrowserPageURL(url) else {
                    return nil
                }
                return url.absoluteString.lowercased()
            }
        )

        for item in sharedItems {
            guard let url = item.appURL else { continue }
            let key = url.absoluteString.lowercased()
            guard !knownBrowserAddresses.contains(key) else { continue }
            knownBrowserAddresses.insert(key)
            merged.append(item)
        }

        let sorted = merged.sorted { $0.visitTime > $1.visitTime }
        if let limit {
            return Array(sorted.prefix(max(limit, 0)))
        }
        return sorted
    }

    // MARK: - Tab Groups History

    /// Record a closed tab group for potential restore
    func recordClosedGroup(_ group: TabGroup, tabs: [Tab]) {
        let historyGroup = HistoryGroup.from(group: group, tabs: tabs)
        closedGroups.insert(historyGroup, at: 0)

        // Keep only the last 50 groups
        if closedGroups.count > 50 {
            closedGroups = Array(closedGroups.prefix(50))
        }

        saveClosedGroups()
        logger.notice("Recorded closed group: \(group.name.isEmpty ? "Unnamed" : group.name) with \(tabs.count) tabs")
    }

    /// Remove a group from history
    func removeClosedGroup(_ group: HistoryGroup) {
        closedGroups.removeAll { $0.id == group.id }
        saveClosedGroups()
    }

    /// Clear all closed groups
    func clearClosedGroups() {
        closedGroups.removeAll()
        saveClosedGroups()
    }

    /// Load closed groups from storage
    private func loadClosedGroups() {
        guard let data = UserDefaults.standard.data(forKey: groupsKey) else { return }
        do {
            closedGroups = try JSONDecoder().decode([HistoryGroup].self, from: data)
            logger.info("Loaded \(self.closedGroups.count) closed groups")
        } catch {
            logger.error("Failed to load closed groups: \(error)")
        }
    }

    /// Save closed groups to storage
    private func saveClosedGroups() {
        do {
            let data = try JSONEncoder().encode(closedGroups)
            UserDefaults.standard.set(data, forKey: groupsKey)
        } catch {
            logger.error("Failed to save closed groups: \(error)")
        }
    }
}
