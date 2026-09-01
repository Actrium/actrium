package io.actrium.actro.core

import com.facebook.react.ReactPackage
import com.facebook.react.bridge.NativeModule
import com.facebook.react.bridge.ReactApplicationContext
import com.facebook.react.uimanager.ViewManager

class ActroPackage : ReactPackage {
    override fun createNativeModules(reactContext: ReactApplicationContext): List<NativeModule> {
        return listOf(
            ActrBridge(reactContext),
            TabModule(reactContext),
            BrowserDataModule(reactContext),
            UserInfoModule(reactContext),
            EmailModule(reactContext),
            AsrModule(reactContext),
        )
    }

    override fun createViewManagers(reactContext: ReactApplicationContext): List<ViewManager<*, *>> {
        return listOf(
            PhoneNavigationBarViewManager(reactContext)
        )
    }
}
