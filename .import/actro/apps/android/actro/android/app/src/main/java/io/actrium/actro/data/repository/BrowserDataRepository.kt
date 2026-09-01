package io.actrium.actro.data.repository

import android.net.Uri
import io.actrium.actro.data.database.BrowserBookmarkItem
import io.actrium.actro.data.database.BrowserHistoryItem
import io.actrium.actro.data.database.BrowserSessionSnapshot
import io.actrium.actro.data.database.BrowserSessionTab
import io.actrium.actro.data.database.BrowserStorageDatabase
import io.actrium.actro.model.ActiveModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

data class RestoredBrowserSession(
    val activeList: List<ActiveModel>,
    val currentTabId: String?
)

class BrowserDataRepository(
    private val database: BrowserStorageDatabase,
    private val onLocalMutation: () -> Unit = {}
) {
    suspend fun loadRecentHistory(limit: Int = 500) = withContext(Dispatchers.IO) {
        database.loadRecentHistory(limit.coerceIn(0, 500))
    }

    suspend fun loadBookmarks(limit: Int = 500) = withContext(Dispatchers.IO) {
        database.loadRecentBookmarks(limit.coerceIn(0, 500))
    }

    suspend fun deleteHistory(address: String) = withContext(Dispatchers.IO) {
        val browserAddress = browserAddress(address) ?: return@withContext false
        database.deleteHistory(browserAddress).also { succeeded ->
            if (succeeded) onLocalMutation()
        }
    }

    suspend fun clearHistory() = withContext(Dispatchers.IO) {
        database.clearHistory().also { succeeded ->
            if (succeeded) onLocalMutation()
        }
    }

    suspend fun deleteBookmark(address: String) = withContext(Dispatchers.IO) {
        val browserAddress = browserAddress(address) ?: return@withContext false
        database.deleteBookmark(browserAddress).also { succeeded ->
            if (succeeded) onLocalMutation()
        }
    }

    suspend fun clearBookmarks() = withContext(Dispatchers.IO) {
        database.clearBookmarks().also { succeeded ->
            if (succeeded) onLocalMutation()
        }
    }

    suspend fun loadSessionActiveList() = withContext(Dispatchers.IO) {
        val snapshot = database.loadSession() ?: return@withContext null
        val restored = snapshot.tabs.mapIndexedNotNull { index, tab ->
            val address = browserAddress(tab.address) ?: return@mapIndexedNotNull null
            index to ActiveModel(
                id = tab.id,
                name = normalizedTitle(tab.title, address),
                icon = "",
                screenshot = "",
                url = address
            )
        }
        if (restored.isEmpty()) return@withContext null

        val currentTabId = restored
            .firstOrNull { (index, _) -> index == snapshot.activeTabIndex }
            ?.second
            ?.id
            ?: restored.first().second.id

        RestoredBrowserSession(
            activeList = restored.map { (_, activeModel) -> activeModel },
            currentTabId = currentTabId
        )
    }

    suspend fun recordVisit(activeModel: ActiveModel) = withContext(Dispatchers.IO) {
        val address = browserAddress(activeModel.url) ?: return@withContext
        val succeeded = database.recordHistoryVisit(
            BrowserHistoryItem(
                title = normalizedTitle(activeModel.name, address),
                address = address,
                visitedAtUnixSeconds = currentUnixSeconds()
            )
        )
        if (succeeded) onLocalMutation()
    }

    suspend fun saveActiveSnapshot(
        activeList: List<ActiveModel>,
        currentTabId: String?
    ) = withContext(Dispatchers.IO) {
        val now = currentUnixSeconds()
        val browserTabs = activeList.mapNotNull { activeModel ->
            val address = browserAddress(activeModel.url) ?: return@mapNotNull null
            activeModel.id to BrowserSessionTab(
                id = activeModel.id,
                title = normalizedTitle(activeModel.name, address),
                address = address,
                createdAtUnixSeconds = now,
                updatedAtUnixSeconds = now
            )
        }

        val activeTabIndex = currentTabId
            ?.let { id -> browserTabs.indexOfFirst { it.first == id } }
            ?.takeIf { it >= 0 }
            ?: 0

        val sessionSaved = database.saveSession(
            snapshot = BrowserSessionSnapshot(
                activeTabIndex = activeTabIndex,
                tabs = browserTabs.map { it.second },
                surfaceKind = "android"
            )
        )

        val bookmarks = activeList.mapIndexedNotNull { index, activeModel ->
            if (!activeModel.isMarked) return@mapIndexedNotNull null
            val address = browserAddress(activeModel.url) ?: return@mapIndexedNotNull null
            BrowserBookmarkItem(
                id = activeModel.id,
                title = normalizedTitle(activeModel.name, address),
                address = address,
                createdAtUnixSeconds = now,
                updatedAtUnixSeconds = now,
                sortOrder = index,
                favicon = activeModel.icon.ifBlank { null },
                ownerKind = "tab",
                ownerId = activeModel.id
            )
        }
        val markedBookmarkIds = bookmarks.mapTo(mutableSetOf()) { it.id }
        val browserPageActiveIds = activeList.mapNotNullTo(mutableSetOf()) { activeModel ->
            browserAddress(activeModel.url) ?: return@mapNotNullTo null
            activeModel.id
        }
        val staleTabBookmarkIds = browserPageActiveIds - markedBookmarkIds

        var bookmarksSaved = false
        bookmarks.forEach { bookmark ->
            bookmarksSaved = database.upsertBookmarkItem(bookmark) || bookmarksSaved
        }
        staleTabBookmarkIds.forEach { bookmarkId ->
            bookmarksSaved = database.deleteBookmarkById(bookmarkId) || bookmarksSaved
        }
        if (sessionSaved || bookmarksSaved) onLocalMutation()
    }

    private fun browserAddress(rawUrl: String): String? {
        val address = rawUrl.trim()
        if (address.isEmpty()) return null
        val scheme = Uri.parse(address).scheme?.lowercase()
        return if (scheme == "http" || scheme == "https") address else null
    }

    private fun normalizedTitle(title: String, address: String): String {
        val trimmed = title.trim()
        if (trimmed.isNotEmpty()) return trimmed
        val host = Uri.parse(address).host
        return host?.takeIf { it.isNotEmpty() } ?: address
    }
}

private fun currentUnixSeconds(): Long = System.currentTimeMillis() / 1000
