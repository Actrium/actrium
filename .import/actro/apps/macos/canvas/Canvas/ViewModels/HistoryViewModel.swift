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
    private var cancellables = Set<AnyCancellable>()
    private let groupsKey = "closedTabGroups"

    init() {
        setupSearch()
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

    // MARK: - Public Methods

    /// Record a new visit from a Tab
    func recordVisit(tab: Tab) {
        guard let url = tab.url else { return }

        repository.addVisit(
            id: UUID(),
            title: tab.title.isEmpty ? "Untitled" : tab.title,
            appURL: url,
            appId: nil // TODO: Extract from ACTR package
        )

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

        loadRecent()
    }

    /// Load recent history items
    func loadRecent(limit: Int = 10) {
        recentItems = repository.getRecent(limit: limit)
    }

    /// Load all history items
    func loadAll() {
        if searchQuery.isEmpty {
            allItems = repository.getAll()
        } else {
            allItems = repository.search(query: searchQuery)
        }
        groupedItems = allItems.groupedByDate()
    }

    /// Delete a single history item
    func deleteItem(_ item: HistoryItem) {
        repository.deleteItem(id: item.id)
        loadRecent()
        loadAll()
    }

    /// Clear all history
    func clearHistory() {
        repository.clearAll()
        recentItems = []
        allItems = []
        groupedItems = []
    }

    // MARK: - Private Methods

    private func performSearch(_ query: String) {
        if query.isEmpty {
            allItems = repository.getAll()
        } else {
            allItems = repository.search(query: query)
        }
        groupedItems = allItems.groupedByDate()
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
