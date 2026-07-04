package io.actrium.actro.event

import com.facebook.react.bridge.Promise
data class ReactModuleEvent(
    val methodName: String,
    val params: Map<String, Any?> = emptyMap(),
    val promise:Promise? =null
) {
    companion object {
        const val EVENT_EXIT_FULLSCREEN = "exitFullscreen"
        const val EVENT_EXIT_SEARCH = "exitSearch"
        const val EVENT_SET_TAB_UNREAD_STATUS = "setTabUnreadStatus"
        const val EVENT_UPDATE_TAB_INFO = "updateTabInfo"
        const val EVENT_TOGGLE_MARK_STATUS = "toggleMarkStatus"
        const val EVENT_OPEN_TAB = "openTab"
        const val EVENT_GET_ALL_TABS = "getAllTabs"
    }
}
