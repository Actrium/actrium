package com.askaway.android.event

data class ReactModuleEvent(
    val methodName: String,
    val params: Map<String, Any?> = emptyMap()
) {
    companion object {
        const val EVENT_EXIT_FULLSCREEN = "exitFullscreen"
        const val EVENT_EXIT_SEARCH = "exitSearch"
        const val EVENT_SET_TAB_UNREAD_STATUS = "setTabUnreadStatus"
        const val EVENT_UPDATE_TAB_INFO = "updateTabInfo"
        const val EVENT_TOGGLE_MARK_STATUS = "toggleMarkStatus"
    }
}
