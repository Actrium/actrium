package com.askaway.android

import android.content.Intent
import android.os.Bundle
import android.util.Log
import android.view.View
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
import androidx.core.view.forEach
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import com.askaway.android.databinding.ActivityMainBinding
import com.askaway.android.global.Params
import com.askaway.android.view.HeadTabView
import com.facebook.react.ReactRootView
import com.facebook.react.modules.core.DefaultHardwareBackBtnHandler
import com.facebook.react.uimanager.PixelUtil.pxToDp
import kotlinx.coroutines.launch

class MainActivity : AppCompatActivity(), DefaultHardwareBackBtnHandler {
    private lateinit var binding: ActivityMainBinding
    private val viewModel: MainViewModel by viewModels()
    private var reactRootView: ReactRootView? = null
    private val appViews = mutableMapOf<String, ReactRootView>()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)
        binding.flContent.post {
            viewModel.containerWidth = binding.flContent.width.pxToDp().toInt()
            viewModel.containerHeight = binding.flContent.height.pxToDp().toInt()
        }
        binding.ivExpand.setOnClickListener {
            startActivity(Intent(this, HomeActivity::class.java))
        }
        binding.ivAdd.setOnClickListener {
            // 移除旧的 ReactRootView
            viewModel.addApp()
        }
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                launch {
                    viewModel.currentReactNativeStatus.collect {
                        reactRootView?.unmountReactApplication()
//                        binding.flContent.removeAllViews()
                        val targetView = createReactRootView("canvas-kotlin", it)
                        Log.e("MainActivity", "Received new React Native status: ${it.getString("currentTabId")}")
                        targetView.tag = it.getString("currentTabId")
                        binding.flContent.addView(targetView)
                    }
                }
                launch {
                    Params.appList.collect {
                        Log.e("MainActivity", "appList changed: ${it.size}")
                        binding.llTabs.removeAllViews()
                        it.forEach {
                            val tabView = HeadTabView(this@MainActivity)
                            tabView.setOnClickListener { tabView ->
                                val tabId = it.id
                                binding.flContent.forEach { child ->
                                    if(child.tag == tabId) {
                                        binding.flContent.removeView(child)
                                        binding.flContent.addView(child)
                                        return@setOnClickListener
                                    }
                                }
                            }
                            tabView.setAppName("123")
                            binding.llTabs.addView(tabView)
                        }
                    }
                }
            }
        }
    }

    /**
     * 创建支持新架构的 ReactRootView
     */
    private fun createReactRootView(componentName: String, launchOptions: Bundle?): ReactRootView {
        val application = application as MainApplication
        val reactInstanceManager = application.reactNativeHost.reactInstanceManager

        return ReactRootView(this).apply {
            // 为新架构启用 Fabric
            if (BuildConfig.IS_NEW_ARCHITECTURE_ENABLED) {
                setIsFabric(true)
            }

            // 启动 React 应用
            startReactApplication(reactInstanceManager, componentName, launchOptions)
        }
    }

    override fun onPause() {
        super.onPause()
        val application = application as MainApplication
        application.reactNativeHost.reactInstanceManager.onHostPause(this)
    }

    override fun onResume() {
        super.onResume()
        val application = application as MainApplication
        application.reactNativeHost.reactInstanceManager.onHostResume(this, this)
    }

    override fun onDestroy() {
        super.onDestroy()
        reactRootView?.unmountReactApplication()
    }

    @Deprecated("Deprecated in Java")
    override fun onBackPressed() {
        val application = application as MainApplication
        val reactInstanceManager = application.reactNativeHost.reactInstanceManager
        if (reactInstanceManager.hasStartedCreatingInitialContext()) {
            reactInstanceManager.onBackPressed()
        } else {
            @Suppress("DEPRECATION")
            super.onBackPressed()
        }
    }

    override fun invokeDefaultOnBackPressed() {
        @Suppress("DEPRECATION")
        super.onBackPressed()
    }
}
