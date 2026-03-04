import Foundation
import Combine

/// ViewModel for managing bookmarks
@MainActor
final class BookmarkViewModel: ObservableObject {
    @Published var bookmarks: [Bookmark] = []
    @Published var tags: [Tag] = []
    @Published var selectedTagIds: Set<UUID> = []
    @Published var searchQuery: String = ""
    @Published var filteredBookmarks: [Bookmark] = []

    private let bookmarkRepo = BookmarkRepository()
    private let tagRepo = TagRepository()
    private var cancellables = Set<AnyCancellable>()

    init() {
        // One-time cleanup of duplicate bookmarks (same app_url)
        bookmarkRepo.deduplicateByURL()

        setupSearch()
        loadAll()
    }

    private func setupSearch() {
        Publishers.CombineLatest($searchQuery, $selectedTagIds)
            .debounce(for: .milliseconds(300), scheduler: RunLoop.main)
            .sink { [weak self] query, tagIds in
                self?.performFilter(query: query, tagIds: tagIds)
            }
            .store(in: &cancellables)
    }

    // MARK: - Public Methods

    /// Load all bookmarks and tags
    func loadAll() {
        bookmarks = bookmarkRepo.getAll()
        tags = tagRepo.getAllTags()
        performFilter(query: searchQuery, tagIds: selectedTagIds)
    }

    /// Add a new bookmark from a Tab
    func addBookmark(from tab: Tab, tags: [Tag] = [], notes: String? = nil, manufacturer: String? = nil) {
        var bookmark = Bookmark.from(tab: tab, manufacturer: manufacturer)
        bookmark.notes = notes
        bookmark.tags = tags

        bookmarkRepo.add(bookmark)
        loadAll()
    }

    /// Add a bookmark directly
    func addBookmark(_ bookmark: Bookmark) {
        bookmarkRepo.add(bookmark)
        loadAll()
    }

    /// Update an existing bookmark
    func updateBookmark(_ bookmark: Bookmark) {
        bookmarkRepo.update(bookmark)
        loadAll()
    }

    /// Remove a bookmark
    func removeBookmark(id: UUID) {
        bookmarkRepo.delete(id: id)
        loadAll()
    }

    /// Check if a tab is bookmarked
    func isBookmarked(tab: Tab) -> Bool {
        guard let url = tab.url else { return false }
        return bookmarkRepo.isBookmarked(appURL: url)
    }

    /// Get bookmark for a tab
    func getBookmark(for tab: Tab) -> Bookmark? {
        guard let url = tab.url else { return nil }
        return bookmarkRepo.getBookmark(for: url)
    }

    /// Update last used time when opening a bookmarked app
    func markAsUsed(_ bookmark: Bookmark) {
        bookmarkRepo.updateLastUsed(id: bookmark.id)
        loadAll()
    }

    /// Toggle bookmark for a tab
    func toggleBookmark(for tab: Tab) -> Bool {
        if let existing = getBookmark(for: tab) {
            removeBookmark(id: existing.id)
            return false
        } else {
            addBookmark(from: tab)
            return true
        }
    }

    // MARK: - Tag Management

    /// Create a new tag
    func createTag(name: String, color: String? = nil) {
        if let _ = tagRepo.createTag(name: name, color: color) {
            tags = tagRepo.getAllTags()
        }
    }

    /// Delete a tag
    func deleteTag(_ tag: Tag) {
        guard !tag.isSystem else { return }
        tagRepo.deleteTag(id: tag.id)
        tags = tagRepo.getAllTags()
        loadAll() // Refresh bookmarks as they might have lost this tag
    }

    /// Get user-created tags only
    var userTags: [Tag] {
        tags.filter { !$0.isSystem }
    }

    /// Toggle tag filter
    func toggleTagFilter(_ tagId: UUID) {
        if selectedTagIds.contains(tagId) {
            selectedTagIds.remove(tagId)
        } else {
            selectedTagIds.insert(tagId)
        }
    }

    /// Clear all tag filters
    func clearTagFilters() {
        selectedTagIds.removeAll()
    }

    /// Clear all bookmarks (including tag associations)
    func clearAllBookmarks() {
        bookmarkRepo.clearAll()
        loadAll()
    }

    // MARK: - Private Methods

    private func performFilter(query: String, tagIds: Set<UUID>) {
        var result = bookmarks

        // Filter by search query
        if !query.isEmpty {
            result = result.filter { bookmark in
                bookmark.title.localizedCaseInsensitiveContains(query) ||
                (bookmark.notes?.localizedCaseInsensitiveContains(query) ?? false) ||
                (bookmark.manufacturer?.localizedCaseInsensitiveContains(query) ?? false)
            }
        }

        // Filter by selected tags
        if !tagIds.isEmpty {
            result = result.filter { bookmark in
                let bookmarkTagIds = Set(bookmark.tags.map { $0.id })
                return !tagIds.isDisjoint(with: bookmarkTagIds)
            }
        }

        filteredBookmarks = result
    }
}
