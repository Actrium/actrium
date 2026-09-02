package io.actrium.app.utils

import io.actrium.app.model.RuntimeOpenTabPlan
import io.actrium.app.model.RuntimeSystemRootProps
import io.actrium.app.model.RuntimeSystemTabProps
import io.actrium.app.model.RuntimeTabBookmarkStatus
import io.actrium.app.model.RuntimeTabUnread
import io.actrium.app.model.RuntimeTabUpdate
import io.actrium.app.rust.ActriumCore

object RuntimeBridgeParams {
    val tabIdKey = ActriumCore.runtimeBridgeTabIdKey()
    val titleKey = ActriumCore.runtimeBridgeTitleKey()
    val iconKey = ActriumCore.runtimeBridgeIconKey()
    val appUrlKey = ActriumCore.runtimeBridgeAppUrlKey()
    val statusKey = ActriumCore.runtimeBridgeStatusKey()
    val unreadKey = ActriumCore.runtimeBridgeUnreadKey()
    val markedKey = ActriumCore.runtimeBridgeMarkedKey()
    val isBookmarkedKey = ActriumCore.runtimeBridgeIsBookmarkedKey()
    val visibleKey = ActriumCore.runtimeBridgeVisibleKey()
    val containerWidthKey = ActriumCore.runtimeBridgeContainerWidthKey()
    val containerHeightKey = ActriumCore.runtimeBridgeContainerHeightKey()
    val rootTabKey = ActriumCore.runtimeRootPropsTabKey()
    val rootTabsKey = ActriumCore.runtimeRootPropsTabsKey()
    val rootSingleTabModeKey = ActriumCore.runtimeRootPropsSingleTabModeKey()
    val rootCurrentTabIdKey = ActriumCore.runtimeRootPropsCurrentTabIdKey()
    val rootIsLeftPanelVisibleKey = ActriumCore.runtimeRootPropsIsLeftPanelVisibleKey()
    val rootIsRightPanelVisibleKey = ActriumCore.runtimeRootPropsIsRightPanelVisibleKey()
    val tabPropsUnifiedScriptKey = ActriumCore.runtimeTabPropsUnifiedScriptKey()
    val tabPropsRightPanelDefaultVisibleKey = ActriumCore.runtimeTabPropsRightPanelDefaultVisibleKey()
    val tabPropsIdKey = ActriumCore.runtimeTabPropsIdKey()
    val tabPropsBackgroundColorKey = ActriumCore.runtimeTabPropsBackgroundColorKey()
    val tabPropsReloadKeyKey = ActriumCore.runtimeTabPropsReloadKeyKey()
    val tabPropsHtmlContentKey = ActriumCore.runtimeTabPropsHtmlContentKey()
    const val openTabKindApp = "app"
    const val openTabKindSystemPage = "systemPage"
    const val openTabKindSystemTab = "systemTab"
    val openTabTabIdKey = tabIdKey
    const val openTabSystemAppTypeKey = "systemAppType"
    const val openTabTabSystemAppTypeKey = "tabSystemAppType"
    val systemAppTypeBookmarks = ActriumCore.systemAppTypeBookmarks()
    val systemAppTypeSettings = ActriumCore.systemAppTypeSettings()
    val systemAppTypeGlobalSearch = ActriumCore.systemAppTypeGlobalSearch()
    val tabBookmarkChangedEventName = ActriumCore.runtimeTabBookmarkChangedEventName()
    val extensionPanelVisibleChangeEventName = ActriumCore.runtimeExtensionPanelVisibleChangeEventName()
    val tabEnteredFullscreenEventName = ActriumCore.runtimeTabEnteredFullscreenEventName()
    val tabExitedFullscreenEventName = ActriumCore.runtimeTabExitedFullscreenEventName()
    val containerSizeChangeEventName = ActriumCore.runtimeContainerSizeChangeEventName()
    val searchInputChangedEventName = ActriumCore.runtimeSearchInputChangedEventName()

    fun tabUpdateFrom(params: Map<String, Any?>): RuntimeTabUpdate? =
        ActriumCore.runtimeTabUpdateFromFields(
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
        ActriumCore.runtimeTabUnreadFromFields(
            tabId = params[tabIdKey] as? String,
            unread = params[unreadKey] as? Boolean,
        )?.let { unread ->
            RuntimeTabUnread(
                tabId = unread.tabId,
                unread = unread.unread,
            )
        }

    fun tabBookmarkStatusFrom(params: Map<String, Any?>): RuntimeTabBookmarkStatus? =
        ActriumCore.runtimeTabBookmarkStatusFromFields(
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
        val payload = ActriumCore.runtimeTabBookmarkEvent(tabId, isBookmarked)
        return mapOf(
            tabIdKey to payload.tabId,
            isBookmarkedKey to payload.isBookmarked,
        )
    }

    fun tabFullscreenEventParams(tabId: String): Map<String, Any?> {
        val payload = ActriumCore.runtimeTabFullscreenEvent(tabId)
        return mapOf(tabIdKey to payload.tabId)
    }

    fun extensionPanelVisibleEventParams(tabId: String, visible: Boolean): Map<String, Any?> {
        val payload = ActriumCore.runtimeExtensionPanelVisibleEvent(tabId, visible)
        return mapOf(
            tabIdKey to payload.tabId,
            visibleKey to payload.visible,
        )
    }

    fun containerSizeChangeEventParams(width: Int, height: Int): Map<String, Any?> {
        val payload = ActriumCore.runtimeContainerSizeChangeEvent(width, height)
        return mapOf(
            containerWidthKey to payload.containerWidth,
            containerHeightKey to payload.containerHeight,
        )
    }

    fun systemRootProps(type: String): RuntimeSystemRootProps {
        val props = ActriumCore.runtimeSystemRootProps(type)
        return RuntimeSystemRootProps(
            tab = RuntimeSystemTabProps(
                systemAppType = props.tab.systemAppType,
            ),
            singleTabMode = props.singleTabMode,
        )
    }
}
