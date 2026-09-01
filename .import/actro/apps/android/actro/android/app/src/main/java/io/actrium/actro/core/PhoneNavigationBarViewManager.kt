package io.actrium.actro.core

import io.actrium.actro.view.PhoneNavigationBarView
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.module.annotations.ReactModule
import com.facebook.react.uimanager.SimpleViewManager
import com.facebook.react.uimanager.ThemedReactContext
import com.facebook.react.uimanager.annotations.ReactProp

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
}