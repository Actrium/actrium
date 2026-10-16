package io.actrium.app.model

enum class AppStatus {
    ALIVE,
    DEAD,
}

data class ActiveModel(
    val id: String,
    val name: String = "",
    val icon: String = "",
    var screenshot: String = "",
    val hasUnread: Boolean = false,
    val url: String = "",
    val isMarked: Boolean = false,
    var panelVisible: Boolean = false,
    val status: AppStatus = AppStatus.ALIVE
) {
    fun applyRuntimeUpdate(update: RuntimeTabUpdate): ActiveModel {
        if (id != update.tabId) return this
        return copy(
            name = update.title ?: name,
            icon = update.icon ?: icon,
            url = update.appUrl ?: url,
            status = update.status ?: status,
        )
    }

    fun withUnreadStatus(payload: RuntimeTabUnread): ActiveModel {
        return copy(hasUnread = payload.unread)
    }

    fun withBookmarkStatus(payload: RuntimeTabBookmarkStatus): ActiveModel {
        return copy(isMarked = payload.marked)
    }
}
