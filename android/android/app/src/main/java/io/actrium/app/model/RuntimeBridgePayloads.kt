package io.actrium.app.model

data class RuntimeTabUpdate(
    val tabId: String,
    val title: String?,
    val icon: String?,
    val appUrl: String?,
    val status: AppStatus?,
)

data class RuntimeTabUnread(
    val tabId: String,
    val unread: Boolean,
)

data class RuntimeTabBookmarkStatus(
    val tabId: String,
    val marked: Boolean,
)

data class RuntimeOpenTabPlan(
    val kind: String,
    val tabId: String?,
    val systemAppType: String?,
    val paramsToRemove: Set<String>,
)

data class RuntimeSystemTabProps(
    val systemAppType: String,
)

data class RuntimeSystemRootProps(
    val tab: RuntimeSystemTabProps,
    val singleTabMode: Boolean,
)
