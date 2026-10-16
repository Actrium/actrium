package io.actrium.app.core

import android.os.Bundle
import io.actrium.app.model.RuntimeSystemRootProps
import io.actrium.app.utils.RuntimeBridgeParams

class ReactNativeBundle {
    companion object {
        const val REACT_COMPONENT_NAME = "actrium"

        val REACT_NATIVE_VIEW_TYPE: String
            get() = RuntimeBridgeParams.openTabSystemAppTypeKey
        val REACT_NATIVE_VIEW_TAB_TYPE: String
            get() = RuntimeBridgeParams.openTabTabSystemAppTypeKey
        val REACT_NATIVE_VIEW_TYPE_BOOKMARK: String
            get() = RuntimeBridgeParams.systemAppTypeBookmarks
        val REACT_NATIVE_VIEW_TYPE_SETTINGS: String
            get() = RuntimeBridgeParams.systemAppTypeSettings
        val REACT_NATIVE_VIEW_TYPE_SEARCH: String
            get() = RuntimeBridgeParams.systemAppTypeGlobalSearch

        fun getSetBundle(): Bundle {
            return addViewType(RuntimeBridgeParams.systemAppTypeSettings)
        }

        fun getBookmarkBundle(): Bundle {
            return addViewType(RuntimeBridgeParams.systemAppTypeBookmarks)
        }

        fun getSearchBundle(): Bundle {
            return addViewType(RuntimeBridgeParams.systemAppTypeGlobalSearch)
        }

        private fun addViewType(type: String): Bundle {
            return genBundle(RuntimeBridgeParams.systemRootProps(type))
        }

        private fun genBundle(props: RuntimeSystemRootProps): Bundle {
            val subBundle = Bundle().apply {
                putString(REACT_NATIVE_VIEW_TYPE, props.tab.systemAppType)
            }
            val bundle = Bundle().apply {
                putBundle(RuntimeBridgeParams.rootTabKey, subBundle)
                putBoolean(RuntimeBridgeParams.rootSingleTabModeKey, props.singleTabMode)
            }
            return bundle
        }
    }
}
