package com.askaway.android

import android.os.Bundle
import android.view.ViewGroup
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
import androidx.fragment.app.Fragment
import com.askaway.android.databinding.ActivityMainBinding
import com.google.android.material.tabs.TabLayout
import com.facebook.react.ReactRootView
import com.facebook.react.ReactInstanceManager
import com.facebook.react.modules.core.DefaultHardwareBackBtnHandler

class MainActivity : AppCompatActivity(), DefaultHardwareBackBtnHandler {
    private lateinit var binding: ActivityMainBinding
    private val viewModel: MainViewModel by viewModels()
    private var reactRootView: ReactRootView? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)

        val tabLayout = binding.tabLayout
        val reactInstanceManager = (application as MainApplication).reactNativeHost.reactInstanceManager

        binding.ivAdd.setOnClickListener {
            binding.flContent.removeAllViews()

            // 停止上一个 ReactRootView
            reactRootView?.unmountReactApplication()

            val bundle = Bundle()
            val tabModel = viewModel.genReactNativeTabModel()
            bundle.putBundle("tab", tabModel.toBundle())
            bundle.putBoolean("singleTabMode", true)

            // 创建新的 ReactRootView
            reactRootView = ReactRootView(this)
            reactRootView?.startReactApplication(
                reactInstanceManager,
                "canvas-kotlin",
                bundle
            )

            // 添加 ReactRootView 到容器
            reactRootView?.let { view ->
                binding.flContent.addView(view)
            }
        }

        // ...existing code...
    }

    private fun showFragment(fragment: Fragment) {
        // ...existing code...
    }

    override fun onDestroy() {
        super.onDestroy()
        reactRootView?.unmountReactApplication()
    }

    override fun onPause() {
        super.onPause()
        val reactInstanceManager = (application as MainApplication).reactNativeHost.reactInstanceManager
        reactInstanceManager.onHostPause(this)
    }

    override fun onResume() {
        super.onResume()
        val reactInstanceManager = (application as MainApplication).reactNativeHost.reactInstanceManager
        reactInstanceManager.onHostResume(this, this)
    }

    @Deprecated("Deprecated in Java")
    override fun onBackPressed() {
        val reactInstanceManager = (application as MainApplication).reactNativeHost.reactInstanceManager
        if (reactInstanceManager != null && reactInstanceManager.hasStartedCreatingInitialContext()) {
            reactInstanceManager.onBackPressed()
            return
        }
        @Suppress("DEPRECATION")
        super.onBackPressed()
    }

    // 实现 DefaultHardwareBackBtnHandler 接口
    override fun invokeDefaultOnBackPressed() {
        @Suppress("DEPRECATION")
        super.onBackPressed()
    }
}
