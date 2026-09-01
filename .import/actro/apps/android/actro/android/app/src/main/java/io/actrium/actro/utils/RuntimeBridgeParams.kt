package io.actrium.actro.utils

import io.actrium.actro.model.RuntimeOpenTabPlan
import io.actrium.actro.model.RuntimeSystemRootProps
import io.actrium.actro.model.RuntimeSystemTabProps
import io.actrium.actro.model.RuntimeTabBookmarkStatus
import io.actrium.actro.model.RuntimeTabUnread
import io.actrium.actro.model.RuntimeTabUpdate
import io.actrium.actro.rust.ActroCore

object RuntimeBridgeParams {
    val tabIdKey = ActroCore.runtimeBridgeTabIdKey()
    val titleKey = ActroCore.runtimeBridgeTitleKey()
    val iconKey = ActroCore.runtimeBridgeIconKey()
    val appUrlKey = ActroCore.runtimeBridgeAppUrlKey()
    val statusKey = ActroCore.runtimeBridgeStatusKey()
    val unreadKey = ActroCore.runtimeBridgeUnreadKey()
    val markedKey = ActroCore.runtimeBridgeMarkedKey()
    val isBookmarkedKey = ActroCore.runtimeBridgeIsBookmarkedKey()
    val visibleKey = ActroCore.runtimeBridgeVisibleKey()
    val containerWidthKey = ActroCore.runtimeBridgeContainerWidthKey()
    val containerHeightKey = ActroCore.runtimeBridgeContainerHeightKey()
    val rootTabKey = ActroCore.runtimeRootPropsTabKey()
    val rootTabsKey = ActroCore.runtimeRootPropsTabsKey()
    val rootSingleTabModeKey = ActroCore.runtimeRootPropsSingleTabModeKey()
    val rootCurrentTabIdKey = ActroCore.runtimeRootPropsCurrentTabIdKey()
    val rootIsLeftPanelVisibleKey = ActroCore.runtimeRootPropsIsLeftPanelVisibleKey()
    val rootIsRightPanelVisibleKey = ActroCore.runtimeRootPropsIsRightPanelVisibleKey()
    val tabPropsUnifiedScriptKey = ActroCore.runtimeTabPropsUnifiedScriptKey()
    val tabPropsRightPanelDefaultVisibleKey = ActroCore.runtimeTabPropsRightPanelDefaultVisibleKey()
    val tabPropsIdKey = ActroCore.runtimeTabPropsIdKey()
    val tabPropsBackgroundColorKey = ActroCore.runtimeTabPropsBackgroundColorKey()
    val tabPropsReloadKeyKey = ActroCore.runtimeTabPropsReloadKeyKey()
    val tabPropsHtmlContentKey = ActroCore.runtimeTabPropsHtmlContentKey()
    const val openTabKindApp = "app"
    const val openTabKindSystemPage = "systemPage"
    const val openTabKindSystemTab = "systemTab"
    val openTabTabIdKey = tabIdKey
    const val openTabSystemAppTypeKey = "systemAppType"
    const val openTabTabSystemAppTypeKey = "tabSystemAppType"
    val systemAppTypeBookmarks = ActroCore.systemAppTypeBookmarks()
    val systemAppTypeSettings = ActroCore.systemAppTypeSettings()
    val systemAppTypeGlobalSearch = ActroCore.systemAppTypeGlobalSearch()
    val tabBookmarkChangedEventName = ActroCore.runtimeTabBookmarkChangedEventName()
    val extensionPanelVisibleChangeEventName = ActroCore.runtimeExtensionPanelVisibleChangeEventName()
    val tabEnteredFullscreenEventName = ActroCore.runtimeTabEnteredFullscreenEventName()
    val tabExitedFullscreenEventName = ActroCore.runtimeTabExitedFullscreenEventName()
    val containerSizeChangeEventName = ActroCore.runtimeContainerSizeChangeEventName()
    val searchInputChangedEventName = ActroCore.runtimeSearchInputChangedEventName()

    fun tabUpdateFrom(params: Map<String, Any?>): RuntimeTabUpdate? =
        ActroCore.runtimeTabUpdateFromFields(
            tabId = params[tabIdKey] as? String,
            title = params[titleKey] as? String,
            icon = params[iconKey] as? String,
            appUrl = params[appUrlKey] as? String,
            status = params[statusKey] as? String,
        )?.let { update ->
            RuntimeTabUpdate(
                tabId = update.tabId,
                title = update.title,
                icon = update.icon,
                appUrl = update.appUrl,
                status = update.status?.let { ActiveTabCoreAdapter.appStatusFromCoreValue(it) },
            )
        }

    fun tabUnreadFrom(params: Map<String, Any?>): RuntimeTabUnread? =
        ActroCore.runtimeTabUnreadFromFields(
            tabId = params[tabIdKey] as? String,
            unread = params[unreadKey] as? Boolean,
        )?.let { unread ->
            RuntimeTabUnread(
                tabId = unread.tabId,
                unread = unread.unread,
            )
        }

    fun tabBookmarkStatusFrom(params: Map<String, Any?>): RuntimeTabBookmarkStatus? =
        ActroCore.runtimeTabBookmarkStatusFromFields(
            tabId = params[tabIdKey] as? String,
            marked = params[markedKey] as? Boolean,
        )?.let { bookmark ->
            RuntimeTabBookmarkStatus(
                tabId = bookmark.tabId,
                marked = bookmark.marked,
            )
        }

    fun openTabPlanFrom(
        params: Map<String, Any?>,
        preferTabletRoutes: Boolean,
        systemKeyPresenceMatters: Boolean,
    ): RuntimeOpenTabPlan {
        val tabId = params[openTabTabIdKey] as? String
        val systemAppType = params[openTabSystemAppTypeKey] as? String
        val hasSystemAppType = if (systemKeyPresenceMatters) {
            params.containsKey(openTabSystemAppTypeKey)
        } else {
            systemAppType != null
        }
        val hasTabSystemAppType = params.containsKey(openTabTabSystemAppTypeKey)
        val kind = when {
            hasSystemAppType -> openTabKindSystemPage
            preferTabletRoutes && hasTabSystemAppType -> openTabKindSystemTab
            else -> openTabKindApp
        }
        val paramsToRemove = when (kind) {
            openTabKindSystemPage -> setOf(openTabSystemAppTypeKey)
            openTabKindSystemTab -> emptySet()
            else -> setOf(openTabTabIdKey)
        }
        return RuntimeOpenTabPlan(
            kind = kind,
            tabId = tabId,
            systemAppType = systemAppType,
            paramsToRemove = paramsToRemove,
        )
    }

    fun filteredOpenTabParams(
        params: Map<String, Any?>,
        plan: RuntimeOpenTabPlan,
    ): Map<String, Any?> {
        return params.filterKeys { key -> key !in plan.paramsToRemove }
    }

    fun tabBookmarkEventParams(tabId: String, isBookmarked: Boolean): Map<String, Any?> {
        val payload = ActroCore.runtimeTabBookmarkEvent(tabId, isBookmarked)
        return mapOf(
            tabIdKey to payload.tabId,
            isBookmarkedKey to payload.isBookmarked,
        )
    }

    fun tabFullscreenEventParams(tabId: String): Map<String, Any?> {
        val payload = ActroCore.runtimeTabFullscreenEvent(tabId)
        return mapOf(tabIdKey to payload.tabId)
    }

    fun extensionPanelVisibleEventParams(tabId: String, visible: Boolean): Map<String, Any?> {
        val payload = ActroCore.runtimeExtensionPanelVisibleEvent(tabId, visible)
        return mapOf(
            tabIdKey to payload.tabId,
            visibleKey to payload.visible,
        )
    }

    fun containerSizeChangeEventParams(width: Int, height: Int): Map<String, Any?> {
        val payload = ActroCore.runtimeContainerSizeChangeEvent(width, height)
        return mapOf(
            containerWidthKey to payload.containerWidth,
            containerHeightKey to payload.containerHeight,
        )
    }

    fun systemRootProps(type: String): RuntimeSystemRootProps {
        val props = ActroCore.runtimeSystemRootProps(type)
        return RuntimeSystemRootProps(
            tab = RuntimeSystemTabProps(
                systemAppType = props.tab.systemAppType,
            ),
            singleTabMode = props.singleTabMode,
        )
    }
}
