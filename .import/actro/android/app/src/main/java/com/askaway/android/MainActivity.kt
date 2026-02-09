package com.askaway.android

import android.content.Intent
import android.os.Bundle
import android.util.Log
import android.view.View
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
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
//                        reactRootView?.unmountReactApplication()
//                        binding.flContent.removeAllViews()
//                        val currentAppId = Params.appList.value.lastOrNull()?.id ?: return@collect
//                        var targetView = appViews[currentAppId]
//                     直接创建 ReactRootView 作为普通 View 使用
//                        val reactRootView2 = createReactRootView("canvas-kotlin", it)
//                        appViews[Params.appList.value.last().id] = reactRootView2
//                        binding.flContent.addView(reactRootView2)

//                        if (targetView == null) {
//                            // A. 不存在：创建新的，并添加到容器
//                            targetView = createReactRootView("canvas-kotlin", it)
//                            appViews[currentAppId] = targetView
//                            binding.flContent.addView(targetView)
//                        } else {
//                            // B. 存在：更新属性 (可选，如果 props 变了的话)
//                            targetView.appProperties = it
//                        }
//                        appViews.forEach { (id, view) ->
//                            if (id == currentAppId) {
//                                view.visibility = View.VISIBLE
//                                // 仅仅是显示时，一般不需要重新 startReactApplication，因为 View 还在内存里
//                            } else {
//                                view.visibility = View.GONE
//                            }
//                        }
                        val targetView1 = createReactRootView("canvas-kotlin", it)
                        val targetView2 = createReactRootView("canvas-kotlin", it)
                        binding.flContent.addView(targetView1)
                        binding.flContent2.addView(targetView2)
                    }
                }
                launch {
                    Params.appList.collect {
                        Log.e("MainActivity", "appList changed: ${it.size}")
                        binding.llTabs.removeAllViews()
                        it.forEach {
                            val tabView = HeadTabView(this@MainActivity)
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
