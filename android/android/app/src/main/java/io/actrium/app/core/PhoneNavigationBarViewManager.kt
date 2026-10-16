package io.actrium.app.core

import io.actrium.app.view.PhoneNavigationBarView
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
     * Set title text
     */
    @ReactProp(name = "title")
    fun setTitle(view: PhoneNavigationBarView, title: String?) {
        title?.let {
            view.setTitle(it)
        }
    }

    /**
     * Set Subtitle Text
     */
    @ReactProp(name = "subtitle")
    fun setSubtitle(view: PhoneNavigationBarView, subtitle: String?) {
        view.setSubtitle(subtitle)
    }

    /**
     * Sets whether to show the large title
     */
    @ReactProp(name = "showLargeTitle", defaultBoolean = true)
    fun setShowLargeTitle(view: PhoneNavigationBarView, show: Boolean) {
        view.setShowLargeTitle(show)
    }
}