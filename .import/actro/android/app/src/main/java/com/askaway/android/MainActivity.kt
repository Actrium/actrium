package com.askaway.android

import android.animation.ObjectAnimator
import android.content.Intent
import android.graphics.Outline
import android.graphics.Path
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.View
import android.view.ViewOutlineProvider
import android.view.ViewTreeObserver
import android.view.animation.DecelerateInterpolator
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
import com.askaway.android.global.Constants
import kotlinx.coroutines.delay
import com.askaway.android.core.ReactNativeBundle

class MainActivity : AppCompatActivity(), DefaultHardwareBackBtnHandler {
    private lateinit var binding: ActivityMainBinding
    private val viewModel: MainViewModel by viewModels()
    private var reactRootView: ReactRootView? = null
    private val appViews = mutableMapOf<String, ReactRootView>()
    private var isReactNativeVisible = false
    private var isLayoutComplete = false

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)

        // 初始化 flReactNative 为隐藏状态
        binding.flReactNative.visibility = View.INVISIBLE // 使用 INVISIBLE 而不是 GONE，以便能获取高度

        // 使用 ViewTreeObserver 确保视图完全布局后再设置初始位置
        binding.flRNContent.viewTreeObserver.addOnGlobalLayoutListener(object : ViewTreeObserver.OnGlobalLayoutListener {
            override fun onGlobalLayout() {
                if (!isLayoutComplete && binding.flReactNative.height > 0) {
                    binding.flReactNative.translationY = binding.flReactNative.height.toFloat()
                    binding.flReactNative.visibility = View.GONE
                    isLayoutComplete = true
                    binding.flReactNative.viewTreeObserver.removeOnGlobalLayoutListener(this)

                    // 设置自定义 OutlineProvider 以支持圆角裁剪（仅上方圆角）
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
                        // 38dp 转换为 px
                        val cornerRadiusDp = 38f
                        val cornerRadiusPx = cornerRadiusDp * resources.displayMetrics.density

                        binding.flRNContent.outlineProvider = object : ViewOutlineProvider() {
                            override fun getOutline(view: View, outline: Outline) {
                                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
                                    // Android 11+ 使用 setPath
                                    val path = Path()
                                    val width = view.width.toFloat()
                                    val height = view.height.toFloat()
                                    val radius = cornerRadiusPx

                                    // 从左上角开始
                                    path.moveTo(radius, 0f)
                                    // 顶部直线到右上角
                                    path.lineTo(width - radius, 0f)
                                    // 右上角圆角 (90度圆弧)
                                    path.arcTo(width - radius * 2, 0f, width, radius * 2, 270f, 90f, false)
                                    // 右侧直线到右下角
                                    path.lineTo(width, height)
                                    // 底部直线到左下角
                                    path.lineTo(0f, height)
                                    // 左侧直线到左上角
                                    path.lineTo(0f, radius)
                                    // 左上角圆角 (90度圆弧)
                                    path.arcTo(0f, 0f, radius * 2, radius * 2, 180f, 90f, false)
                                    path.close()

                                    outline.setPath(path)
                                } else {
                                    // Android 5.0-10 使用 setRoundRect，所有角都会有圆角
                                    outline.setRoundRect(0, 0, view.width, view.height, cornerRadiusPx)
                                }
                            }
                        }
                        binding.flRNContent.elevation = 0.0f
                        binding.flRNContent.clipToOutline = true
                    }

                    Log.d("MainActivity", "flReactNative initial height: ${binding.flReactNative.height}")
                }
            }
        })

        binding.flContent.post {
            viewModel.containerWidth = binding.flContent.width.pxToDp().toInt()
            viewModel.containerHeight = binding.flContent.height.pxToDp().toInt()
        }

        binding.ivAdd.setOnClickListener {
            viewModel.addApp()
            showReactNativeView()
        }

        // 设置导航栏选择监听器
        setupNavigationListener()

        // 初始化导航指示器位置和内容显隐
        binding.rgNavigation.post {
            // 先设置 navActivity 为选中状态
            binding.navActivity.isChecked = true
            // 不使用动画，直接对齐到 navActivity 位置
            alignIndicatorToSelectedButton(animate = false)
        }

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                launch {
                    viewModel.currentReactNativeStatus.collect {
//                        reactRootView?.unmountReactApplication()
//                        binding.flContent.removeAllViews()
                        val targetView = createReactRootView("canvas-kotlin", it)
                        Log.e("MainActivity", "Received new React Native status: ${it.getString("currentTabId")}")
                        targetView.tag = it.getString("currentTabId")
                        binding.flRNContent.addView(targetView)
                    }
                }
                launch {
                    Params.appList.collect {
                        Log.e("MainActivity", "appList changed: ${it.size}")
//                        binding.llTabs.removeAllViews()
//                        it.forEach {
//                            val tabView = HeadTabView(this@MainActivity)
//                            tabView.setOnClickListener { tabView ->
//                                val tabId = it.id
//                                binding.flContent.forEach { child ->
//                                    if(child.tag == tabId) {
//                                        binding.flContent.removeView(child)
//                                        binding.flContent.addView(child)
//                                        return@setOnClickListener
//                                    }
//                                }
//                            }
//                            tabView.setAppName("123")
//                            binding.llTabs.addView(tabView)
//                        }
                    }
                }
            }
        }
    }

    /**
     * 设置导航栏监听器
     */
    private fun setupNavigationListener() {
        binding.rgNavigation.setOnCheckedChangeListener { _, checkedId ->
            alignIndicatorToSelectedButton(animate = true)
        }
    }

    /**
     * 将指示器对齐到选中的 RadioButton
     */
    private fun alignIndicatorToSelectedButton(animate: Boolean = false) {
        val checkedId = binding.rgNavigation.checkedRadioButtonId
        if (checkedId == -1) return

        // 获取选中的 RadioButton
        val selectedButton = binding.rgNavigation.findViewById<View>(checkedId) ?: return

        // 计算选中按钮的中心位置
        val buttonCenterX = selectedButton.left + selectedButton.width / 2f

        // 计算指示器应该移动到的位置（使指示器中心与按钮中心对齐）
        val indicatorCenterOffset = binding.flNavIndicator.width / 2f
        val targetX = buttonCenterX - indicatorCenterOffset

        Log.d("MainActivity", "Align indicator: checkedId=$checkedId, buttonCenterX=$buttonCenterX, targetX=$targetX")

        if (animate) {
            // 创建滑动动画
            ObjectAnimator.ofFloat(binding.flNavIndicator, "translationX", targetX).apply {
                duration = 200
                interpolator = DecelerateInterpolator()
                start()
            }
        } else {
            // 直接设置位置（初始化时不需要动画）
            binding.flNavIndicator.translationX = targetX
        }

        // 根据选中的标签切换对应的内容显隐
        updateContentVisibility(checkedId)
    }

    /**
     * 更新内容组件的显隐状态
     */
    private fun updateContentVisibility(checkedId: Int) {
        when (checkedId) {
            R.id.navTag -> {
                // 显示书签页
                binding.flBookmark.visibility = View.VISIBLE
                binding.llActive.visibility = View.GONE
                binding.flSet.visibility = View.GONE
                Log.d("MainActivity", "Show Bookmark")
            }
            R.id.navActivity -> {
                // 显示活动页
                binding.flBookmark.visibility = View.GONE
                binding.llActive.visibility = View.VISIBLE
                binding.flSet.visibility = View.GONE
                Log.d("MainActivity", "Show Active")
            }
            R.id.navSettings -> {
                // 显示设置页
             showSet()
            }
        }
    }

    private fun showSet(){
        binding.flBookmark.visibility = View.GONE
        binding.llActive.visibility = View.GONE
        binding.flSet.visibility = View.VISIBLE
        if(binding.flSet.childCount == 0) {
            val bundle = ReactNativeBundle.getSetBundle()
            val targetView = createReactRootView("canvas-kotlin", bundle)
            binding.flSet.addView(targetView)
        }
        Log.d("MainActivity", "Show Settings")
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

    /**
     * 从底部显示 ReactNative 视图
     */
    private fun showReactNativeView() {
        if (isReactNativeVisible) return

        isReactNativeVisible = true
        val startY = binding.flReactNative.height.toFloat()

        Log.d("MainActivity", "showReactNativeView - height: ${binding.flReactNative.height}, startY: $startY")

        // 确保起始位置在底部
        binding.flReactNative.translationY = startY
        binding.flReactNative.visibility = View.VISIBLE

        // 创建从底部滑入的动画
        ObjectAnimator.ofFloat(binding.flReactNative, "translationY", startY, 0f).apply {
            duration = 300
            interpolator = DecelerateInterpolator()
            start()
        }
    }

    /**
     * 隐藏 ReactNative 视图到底部
     */
    private fun hideReactNativeView() {
        if (!isReactNativeVisible) return

        // 创建滑出到底部的动画
        ObjectAnimator.ofFloat(binding.flReactNative, "translationY", 0f, binding.flReactNative.height.toFloat()).apply {
            duration = 300
            interpolator = DecelerateInterpolator()
            start()
            addUpdateListener { animation ->
                if (animation.animatedFraction == 1f) {
                    binding.flReactNative.visibility = View.GONE
                    isReactNativeVisible = false
                }
            }
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
        // 如果 flReactNative 正在显示，则隐藏它
        if (isReactNativeVisible) {
            hideReactNativeView()
            return
        }

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
