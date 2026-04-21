package com.askaway.android.core

import chat.askaway.android.view.PhoneNavigationBarView
import com.facebook.react.bridge.Arguments
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.bridge.WritableMap
import com.facebook.react.module.annotations.ReactModule
import com.facebook.react.uimanager.SimpleViewManager
import com.facebook.react.uimanager.ThemedReactContext
import com.facebook.react.uimanager.annotations.ReactProp
import com.facebook.react.uimanager.events.RCTEventEmitter

/**
 * React Native ViewManager for PhoneNavigationBarView
 */
@ReactModule(name = PhoneNavigationBarViewManager.REACT_CLASS)
class PhoneNavigationBarViewManager(
    private val reactContext: ReactApplicationContext
) : SimpleViewManager<PhoneNavigationBarView>() {

    companion object {
        const val REACT_CLASS = "PhoneNavigationBarView"
    }

    override fun getName(): String = REACT_CLASS

    override fun createViewInstance(reactContext: ThemedReactContext): PhoneNavigationBarView {
        val view = PhoneNavigationBarView(reactContext)

        // 设置添加按钮点击事件
        view.onAddClickListener = {
            val event: WritableMap = Arguments.createMap()
            reactContext
                .getJSModule(RCTEventEmitter::class.java)
                .receiveEvent(view.id, "onAddClick", event)
        }

        return view
    }

    /**
     * 设置标题文字
     */
    @ReactProp(name = "title")
    fun setTitle(view: PhoneNavigationBarView, title: String?) {
        title?.let {
            view.setTitle(it)
        }
    }

    /**
     * 设置副标题文字
     */
    @ReactProp(name = "subtitle")
    fun setSubtitle(view: PhoneNavigationBarView, subtitle: String?) {
        view.setSubtitle(subtitle)
    }

    /**
     * 设置是否显示大标题
     */
    @ReactProp(name = "showLargeTitle", defaultBoolean = true)
    fun setShowLargeTitle(view: PhoneNavigationBarView, show: Boolean) {
        view.setShowLargeTitle(show)
    }

    /**
     * 设置添加按钮的可见性
     */
    @ReactProp(name = "showAddButton", defaultBoolean = true)
    fun setShowAddButton(view: PhoneNavigationBarView, show: Boolean) {
        view.setAddButtonVisible(show)
    }

    /**
     * 导出事件名称
     */
    override fun getExportedCustomDirectEventTypeConstants(): MutableMap<String, Any> {
        return mutableMapOf(
            "onAddClick" to mapOf("registrationName" to "onAddClick")
        )
    }
}