package com.askaway.android

import android.animation.ObjectAnimator
import android.graphics.Bitmap
import android.graphics.Canvas
import android.content.Intent
import android.graphics.Outline
import android.graphics.Path
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.View
import android.view.ViewOutlineProvider
import android.view.ViewTreeObserver
import android.view.WindowInsets
import android.view.animation.DecelerateInterpolator
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.recyclerview.widget.LinearLayoutManager
import com.askaway.android.core.ReactNativeBundle
import com.askaway.android.databinding.ActivityMainBinding
import com.askaway.android.global.Params
import com.askaway.android.model.ActiveModel
import com.facebook.react.ReactRootView
import com.facebook.react.modules.core.DefaultHardwareBackBtnHandler
import com.facebook.react.modules.core.PermissionAwareActivity
import com.facebook.react.modules.core.PermissionListener
import com.facebook.react.uimanager.PixelUtil.pxToDp
import java.io.File
import java.io.FileOutputStream
import java.util.UUID
import kotlinx.coroutines.launch
import com.askaway.android.utils.adjustKeyboard
import com.askaway.android.utils.getStatusBarHeight
import com.askaway.android.utils.screenshot
import com.askaway.android.utils.ViewAnimationHelper

class MainActivity : AppCompatActivity(), DefaultHardwareBackBtnHandler, PermissionAwareActivity {
    private lateinit var binding: ActivityMainBinding
    private val viewModel: MainViewModel by viewModels()
    private var reactRootView: ReactRootView? = null
    private val appViews = mutableMapOf<String, ReactRootView>()
    private var isReactNativeVisible = false
    private var isSearchVisible = false
    private var isLayoutComplete = false
    private var permissionListener: PermissionListener? = null
    private lateinit var activeRecyclerAdapter: com.askaway.android.adapter.ActiveRecyclerAdapter
    private var currentSourceView: View? = null

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

                    Log.d("MainActivity", "flReactNative initial height: ${binding.flReactNative.height}")
                }
            }
        })

        binding.flContent.post {
            viewModel.containerWidth = binding.flContent.width.pxToDp().toInt()
            viewModel.containerHeight = binding.flContent.height.pxToDp().toInt()
        }
        binding.flReactNative.adjustKeyboard()

        binding.ivAdd.setOnClickListener {
            viewModel.addApp()
            showReactNativeView()
        }

        binding.ivSearch.setOnClickListener {
            showSearch()
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

        // 初始化 RecyclerView 和 Adapter
        activeRecyclerAdapter = com.askaway.android.adapter.ActiveRecyclerAdapter()
        binding.recyApps.apply {
            layoutManager = LinearLayoutManager(this@MainActivity)
            adapter = activeRecyclerAdapter
        }

        activeRecyclerAdapter.onMarkClick = { activeModel ->
            viewModel.toggleMarkStatus(activeModel)
        }
        // 设置 item 点击事件
        activeRecyclerAdapter.onItemClick = { activeModel, view ->
            // 显示 flRNContent
            showReactNativeView(view)
            // 遍历 flRNContent 的子 view，将 tag == activeModel.id 的 view bringToFront
            for (i in 0 until binding.flRNContent.childCount) {
                val child = binding.flRNContent.getChildAt(i)
                if (child.tag == activeModel.id) {
                    Params.currentTabId = activeModel.id
                    child.bringToFront()
                    binding.flRNContent.invalidate()
                    break
                }
            }
        }

        // 设置删除点击事件
        activeRecyclerAdapter.onDeleteClick = { activeModel ->
            // 先移除 flRNContent 中对应的 child
            for (i in binding.flRNContent.childCount - 1 downTo 0) {
                val child = binding.flRNContent.getChildAt(i)
                if (child.tag == activeModel.id) {
                    binding.flRNContent.removeViewAt(i)
                    // 必须手动卸载 React 应用以释放 JS 资源并停止执行
                    if (child is ReactRootView) {
                        try {
                            child.unmountReactApplication()
                        } catch (e: Exception) {
                            Log.e("MainActivity", "Error unmounting React application", e)
                        }
                    }
                    break
                }
            }
            // 移除当前 item
            viewModel.removeActiveModel(activeModel.id)
            // 不再在这里 finish，由 updateData 回调判断
        }

        binding.ivAdd.post {
            binding.ivAdd.performClick()
        }
        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                launch {
                    viewModel.currentReactNativeStatus.collect {
                        val currentTabId = it.getString("currentTabId")
                        var found = false
                        for (i in 0 until binding.flRNContent.childCount) {
                            val child = binding.flRNContent.getChildAt(i)
                            if (child.tag == currentTabId) {
                                child.bringToFront()
                                found = true
                                break
                            }
                        }

                        if (!found) {
                            val targetView = createReactRootView("canvas-kotlin", it)
                            Log.e("MainActivity", "Received new React Native status: $currentTabId")
                            targetView.tag = currentTabId
                            binding.flRNContent.addView(targetView)
                        }
                        Params.currentTabId = currentTabId
                        showReactNativeView()
                    }
                }
                launch {
                    viewModel.showPageType.collect { pageType ->
                        Log.e("MainActivity", "Received showPageType: $pageType with bundle: ${viewModel.pageBundle}")
                        when (pageType) {
                            ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_BOOKMARK -> {
                                hideSearch()
                                binding.navTag.isChecked = true
                                showBookmark(viewModel.pageBundle)
                            }

                            ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_SETTINGS -> {
                                hideSearch()
                                binding.navSettings.isChecked = true
                                showSet(viewModel.pageBundle)
                            }

                            ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_SEARCH -> {
                                showSearch(viewModel.pageBundle)
                            }
                        }
                    }
                }
                launch {
                    viewModel.activeList.collect { activeModels ->
                        Log.d("MainActivity", "activeList changed: " + activeModels.size)
                        activeRecyclerAdapter.updateData(activeModels)
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
    private fun updateContentVisibility(checkedId: Int, showPage: Boolean = true) {
        when (checkedId) {
            R.id.navTag -> if (showPage) showBookmark()
            R.id.navActivity -> {
                // 显示活动页
                binding.flBookmark.visibility = View.GONE
                binding.llActive.visibility = View.VISIBLE
                binding.flSet.visibility = View.GONE
            }

            R.id.navSettings -> if (showPage) showSet()
        }
    }

    private fun showBookmark(sourceBundle: Bundle? = null) {
        binding.flBookmark.visibility = View.VISIBLE
        binding.llActive.visibility = View.GONE
        binding.flSet.visibility = View.GONE
        binding.flBookmark.removeAllViews()
        if (binding.flBookmark.childCount == 0) {
            val bundle = sourceBundle ?: ReactNativeBundle.getBookmarkBundle()
            val targetView = createReactRootView("canvas-kotlin", bundle)
            binding.flBookmark.addView(targetView)
        }
        Log.d("MainActivity", "Show Bookmark")
    }

    private fun showSet(sourceBundle: Bundle? = null) {
        binding.flBookmark.visibility = View.GONE
        binding.llActive.visibility = View.GONE
        binding.flSet.visibility = View.VISIBLE
        binding.flSet.removeAllViews()
        if (binding.flSet.childCount == 0) {
            val bundle = sourceBundle ?: ReactNativeBundle.getSetBundle()
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
    private fun showReactNativeView(sourceView: View? = null) {
        if (isReactNativeVisible) return
        isReactNativeVisible = true

        currentSourceView = sourceView

        ViewAnimationHelper.animateShow(binding.flReactNative, sourceView) {
            // 如果 View 需要重试（例如尺寸未就绪），重置状态并稍后重试
            isReactNativeVisible = false
            showReactNativeView(sourceView)
        }
    }

    /**
     * 从右侧显示搜索视图
     */
    private fun showSearch(sourceBundle: Bundle? = null) {
        if (isSearchVisible) return

        isSearchVisible = true

        // 移除 flSearch 中的所有子组件
        binding.flSearch.removeAllViews()

        // 加载搜索 React Native 视图
        val bundle = sourceBundle ?: ReactNativeBundle.getSearchBundle()
        val targetView = createReactRootView("canvas-kotlin", bundle)
        binding.flSearch.addView(targetView)

        // 初始位置在屏幕右侧
        binding.flSearch.translationX = binding.flSearch.width.toFloat()
        binding.flSearch.visibility = View.VISIBLE
        // 为 flSearch 添加 WindowInsets 监听，处理键盘问题
        binding.flSearch.adjustKeyboard()
        // 请求应用 window insets
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.KITKAT) {
            binding.flSearch.requestApplyInsets()
        }

        // 创建从右侧滑入的动画
        ObjectAnimator.ofFloat(binding.flSearch, "translationX", binding.flSearch.width.toFloat(), 0f).apply {
            duration = 300
            interpolator = DecelerateInterpolator()
            start()
        }
    }

    /**
     * 隐藏搜索视图到右侧
     */
    private fun hideSearch() {
        if (!isSearchVisible) return

        // 创建滑出到右侧的动画
        ObjectAnimator.ofFloat(binding.flSearch, "translationX", 0f, binding.flSearch.width.toFloat()).apply {
            duration = 300
            interpolator = DecelerateInterpolator()
            start()
            addUpdateListener { animation ->
                if (animation.animatedFraction == 1f) {
                    binding.flSearch.visibility = View.GONE
                    isSearchVisible = false
                }
            }
        }
    }

    /**
     * 隐藏 ReactNative 视图到底部
     */
    private fun hideReactNativeView() {
        if (!isReactNativeVisible) return

        val tabId = Params.currentTabId
        if (!tabId.isNullOrEmpty()) {
            if (viewModel.canCaptureScreenshot(tabId)) {
                val screenshotPath = binding.flRNContent.screenshot("$tabId.jpg")
                if (screenshotPath != null) {
                    viewModel.updateScreenshot(tabId, screenshotPath)
                }
            }
        }

        ViewAnimationHelper.animateHide(binding.flReactNative, currentSourceView) {
            binding.flReactNative.visibility = View.GONE
            isReactNativeVisible = false
        }
        currentSourceView = null
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
        // 如果搜索视图正在显示，则隐藏它
        if (isSearchVisible) {
            hideSearch()
            return
        }

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

    override fun checkPermission(permission: String, pid: Int, uid: Int): Int {
        return super.checkPermission(permission, pid, uid)
    }

    override fun checkSelfPermission(permission: String): Int {
        return ContextCompat.checkSelfPermission(this, permission)
    }

    override fun shouldShowRequestPermissionRationale(permission: String): Boolean {
        return super.shouldShowRequestPermissionRationale(permission)
    }

    override fun requestPermissions(
        permissions: Array<String>,
        requestCode: Int,
        listener: PermissionListener?,
    ) {
        permissionListener = listener
        super.requestPermissions(permissions, requestCode)
    }

    override fun onRequestPermissionsResult(requestCode: Int, permissions: Array<String>, grantResults: IntArray) {
        val handled =
            permissionListener?.onRequestPermissionsResult(
                requestCode,
                permissions,
                grantResults,
            ) ?: false
        if (handled) {
            permissionListener = null
            return
        }
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
    }

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        val application = application as MainApplication
        application.reactNativeHost.reactInstanceManager
            .onActivityResult(this, requestCode, resultCode, data)
    }
}
