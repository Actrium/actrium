package io.actrium.app.event

import io.actrium.app.utils.RuntimeBridgeParams

data class NativeModuleEvent(
    val methodName: String,
    val params: Map<String, Any?> = emptyMap()
) {
    companion object {
        val EVENT_TAB_BOOKMARK_CHANGED = RuntimeBridgeParams.tabBookmarkChangedEventName
        val EVENT_EXTENSION_PANEL_VISIBLE_CHANGE =
            RuntimeBridgeParams.extensionPanelVisibleChangeEventName
        val EVENT_TAB_ENTERED_FULLSCREEN = RuntimeBridgeParams.tabEnteredFullscreenEventName
        val EVENT_TAB_EXITED_FULLSCREEN = RuntimeBridgeParams.tabExitedFullscreenEventName

        val EVENT_CONTAINER_SIZE_CHANGE = RuntimeBridgeParams.containerSizeChangeEventName
        val EVENT_SEARCH_INPUT_CHANGED = RuntimeBridgeParams.searchInputChangedEventName
        const val EVENT_UPDATE_BOOKMARK = "onUpdateBookmark"
        const val EVENT_UPDATE_SET = "onUpdateSet"
    }
}
