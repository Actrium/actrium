package com.keel.demo

import android.os.Bundle
import com.facebook.react.ReactActivity
import com.facebook.react.ReactActivityDelegate
import com.facebook.react.defaults.DefaultNewArchitectureEntryPoint.fabricEnabled
import com.facebook.react.defaults.DefaultReactActivityDelegate

class MainActivity : ReactActivity() {

    override fun getMainComponentName(): String = "KeelDemo"

    override fun createReactActivityDelegate(): ReactActivityDelegate =
        object : DefaultReactActivityDelegate(this, mainComponentName, fabricEnabled) {
            override fun getLaunchOptions(): Bundle? {
                val extras = intent?.extras ?: return null
                val launchOptions = Bundle()

                if (extras.getBoolean("keelE2E", false)) {
                    launchOptions.putBoolean("keelE2E", true)
                }

                extras.getString("keelSandbox")?.let { sandbox ->
                    launchOptions.putString("keelSandbox", sandbox)
                }

                return if (launchOptions.isEmpty) null else launchOptions
            }
        }
}
