package io.actrium.app.utils

import android.os.Bundle
import io.actrium.app.model.ActiveModel
import io.actrium.app.model.AppModel
import io.actrium.app.rust.ActriumCore
import io.actrium.app.rust.RuntimeRootPropsInfo
import io.actrium.app.rust.RuntimeTabInfo

class TabBundleUtils {
    companion object {
        fun genBundle(tab: AppModel): Bundle {
            return genBundle(tab.toCoreRuntimeTabInfo())
        }

        fun genBundle(tab: ActiveModel): Bundle {
            return genBundle(ActriumCore.runtimeTabFromActiveTab(ActiveTabCoreAdapter.toCoreInfo(tab)))
        }

        private fun genBundle(tab: RuntimeTabInfo): Bundle {
            return Bundle().apply {
                putString(RuntimeBridgeParams.tabPropsUnifiedScriptKey, tab.unifiedScript)
                putBoolean(
                    RuntimeBridgeParams.tabPropsRightPanelDefaultVisibleKey,
                    tab.rightPanelDefaultVisible,
                )
                putString(RuntimeBridgeParams.tabPropsIdKey, tab.id)
                putString(RuntimeBridgeParams.tabPropsBackgroundColorKey, tab.backgroundColor)
                putInt(RuntimeBridgeParams.tabPropsReloadKeyKey, tab.reloadKey)
                putString(RuntimeBridgeParams.tabPropsHtmlContentKey, tab.htmlContent)
                putString(RuntimeBridgeParams.titleKey, tab.title)
            }
        }

        fun genBundle(tab: AppModel, tabs: List<AppModel>, height: Int, width: Int): Bundle {
            return genBundle(
                ActriumCore.runtimeRootPropsFromRuntimeTabs(
                    tab.toCoreRuntimeTabInfo(),
                    tabs.map { it.toCoreRuntimeTabInfo() },
                    tab.isLeftPanelVisible,
                    tab.isRightPanelVisible,
                    height,
                    width,
                    null
                )
            )
        }

        fun genBundle(tab: ActiveModel, tabs: List<ActiveModel>, height: Int, width: Int): Bundle {
            return genBundle(
                ActriumCore.runtimeRootPropsFromActiveTabs(
                    ActiveTabCoreAdapter.toCoreInfo(tab),
                    tabs.map { ActiveTabCoreAdapter.toCoreInfo(it) },
                    height,
                    width
                )
            )
        }

        private fun genBundle(props: RuntimeRootPropsInfo): Bundle {
            val tabBundles: Array<Bundle> = props.tabs.map { genBundle(it) }.toTypedArray()
            return Bundle().apply {
                putBundle(RuntimeBridgeParams.rootTabKey, genBundle(props.tab))
                putBoolean(
                    RuntimeBridgeParams.rootIsLeftPanelVisibleKey,
                    props.isLeftPanelVisible,
                )
                putParcelableArray(RuntimeBridgeParams.rootTabsKey, tabBundles)
                putBoolean(RuntimeBridgeParams.rootSingleTabModeKey, props.singleTabMode)
                putBoolean(
                    RuntimeBridgeParams.rootIsRightPanelVisibleKey,
                    props.isRightPanelVisible,
                )
                putString(RuntimeBridgeParams.rootCurrentTabIdKey, props.currentTabId)
                putInt(RuntimeBridgeParams.containerHeightKey, props.containerHeight)
                putInt(RuntimeBridgeParams.containerWidthKey, props.containerWidth)
                props.appUrl?.let { putString(RuntimeBridgeParams.appUrlKey, it) }
            }
        }

        private fun AppModel.toCoreRuntimeTabInfo(): RuntimeTabInfo {
            return RuntimeTabInfo(
                unifiedScript = unifiedScript,
                rightPanelDefaultVisible = rightPanelDefaultVisible,
                id = id,
                backgroundColor = backgroundColor,
                reloadKey = reloadKey,
                htmlContent = htmlContent,
                title = title
            )
        }
    }
}
