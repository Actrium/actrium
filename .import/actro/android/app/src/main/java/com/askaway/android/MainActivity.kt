package com.askaway.android

import android.animation.ObjectAnimator
import android.animation.ValueAnimator
import android.animation.AnimatorListenerAdapter
import android.animation.Animator
import android.graphics.Color
import android.graphics.Bitmap
import android.graphics.Canvas
import android.content.Intent
import android.graphics.Outline
import android.graphics.Path
import android.os.Build
import android.os.Bundle
import android.util.Log
import android.view.View
import android.view.ViewGroup
import android.view.ViewOutlineProvider
import android.view.ViewTreeObserver
import android.view.WindowInsets
import android.view.animation.DecelerateInterpolator
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updateLayoutParams
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.recyclerview.widget.LinearLayoutManager
import chat.askaway.android.data.database.AppDatabase
import chat.askaway.android.data.repository.ActiveRepository
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
    private val viewModel: MainViewModel by viewModels {
        val database = AppDatabase.getDatabase(applicationContext)
        val repository = ActiveRepository(database.activeDao())
        MainViewModelFactory(repository)
    }
    private var reactRootView: ReactRootView? = null
    private val appViews = mutableMapOf<String, ReactRootView>()
    private var isReactNativeVisible = false
    private var isSearchVisible = false
    private var isLayoutComplete = false
    private var permissionListener: PermissionListener? = null
    private lateinit var activeRecyclerAdapter: com.askaway.android.adapter.ActiveRecyclerAdapter
    private var currentSourceView: View? = null
    private var isSearchMode = false
    private var originalSearchWidth = 0
    private var originalNavWidth = 0

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        // 设置沉浸式，使内容能够延伸到状态栏区域，同时处理状态栏字体颜色
        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.statusBarColor = Color.TRANSPARENT
        // 假设背景是浅色，设置状态栏图标为深色
        WindowCompat.getInsetsController(window, window.decorView).isAppearanceLightStatusBars = false

        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)

        ViewCompat.setOnApplyWindowInsetsListener(binding.root) { v, insets ->
            val systemBars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            val navBars = insets.getInsets(WindowInsetsCompat.Type.navigationBars())
            val imeBars = insets.getInsets(WindowInsetsCompat.Type.ime())
            // 仅应用底部和左右的内边距，保留顶部不做处理，以实现内容延伸到状态栏
            v.setPadding(systemBars.left, 0, systemBars.right, 0)
            // 底部沉浸式判断：当 decorFitsSystemWindows=false 且导航栏 inset 为 0，说明内容已延伸到底部
            val isBottomImmersive = navBars.bottom == 0
            binding.llNav.updateLayoutParams<ViewGroup.MarginLayoutParams>  {
                bottomMargin = navBars.bottom
            }
            insets
        }


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
                    Log.d("MainActivity", "flReactNative initial height: ${binding.flReactNative.height}")
                }
            }
        })

        binding.flContent.post {
            viewModel.containerWidth = binding.flContent.width.pxToDp().toInt()
            viewModel.containerHeight = binding.flContent.height.pxToDp().toInt()
            val statusBarHeight = getStatusBarHeight()
            binding.llActive.updateLayoutParams<ViewGroup.MarginLayoutParams> {
                topMargin = statusBarHeight
            }
            binding.flReactNative.updateLayoutParams<ViewGroup.MarginLayoutParams> {
                topMargin = statusBarHeight
            }

        }
        binding.flReactNative.adjustKeyboard()

        binding.ivAdd.setOnClickListener {
            viewModel.addApp()
            showReactNativeView()
        }

        // 搜索框内容变化时，控制删除按钮显示与隐藏
        binding.etSearch.addTextChangedListener(object : android.text.TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                binding.ivDel.visibility = if (s.isNullOrEmpty()) View.GONE else View.VISIBLE
            }
            override fun afterTextChanged(s: android.text.Editable?) {
                viewModel.changeInputText(s.toString())
            }
        })
        // 删除按钮点击清空搜索框内容
        binding.ivDel.setOnClickListener {
            binding.etSearch.text.clear()
        }

        binding.ivSearch.setOnClickListener {
            enterSearchMode()
            showSearch()
        }

        binding.flNavButton.setOnClickListener {
            exitSearchMode()
            hideSearch()
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
            var hasCreate = false
            Params.currentTabId = activeModel.id
            for (i in 0 until binding.flRNContent.childCount) {
                val child = binding.flRNContent.getChildAt(i)
                if (child.tag == activeModel.id) {
                    hasCreate = true
                    child.bringToFront()
                    binding.flRNContent.invalidate()
                    break
                }
            }
            if(!hasCreate){
                val bundle = viewModel.genBundle(activeModel)
                val targetView = createReactRootView("canvas-kotlin", bundle)
                Log.e("MainActivity", "Received new React Native status: $bundle")
                targetView.tag = activeModel.id
                binding.flRNContent.addView(targetView)
            }
            viewModel.enterFullScreen(activeModel.id)
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

        binding.ivAdd.postDelayed({
            if (viewModel.isActiveListEmpty()) {
                binding.ivAdd.performClick()
            }
        }, 500)

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                launch {
                    viewModel.currentReactNativeStatus.collect {
                        val currentTabId = it.getString("currentTabId")
                        Log.e("MainActivity", "currentTabId:${currentTabId}")
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
                        viewModel.enterFullScreen(currentTabId)
                        binding.navActivity.isChecked = true
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
     * 进入搜索模式
     */
    private fun enterSearchMode() {
        if (isSearchMode) return
        isSearchMode = true

        // 保存原始宽度
        originalSearchWidth = binding.ivSearch.width
        originalNavWidth = (binding.rgNavigation.parent as View).width

        // 根据当前选中的 RadioButton 设置对应的图标和文字
        val checkedId = binding.rgNavigation.checkedRadioButtonId

        // 设置图标
        val iconRes = when (checkedId) {
            R.id.navTag -> R.drawable.nav_tag_selector
            R.id.navActivity -> R.drawable.nav_activity_selector
            R.id.navSettings -> R.drawable.nav_settings_selector
            else -> R.drawable.nav_activity_selector
        }

        // 设置文字
        val textRes = when (checkedId) {
            R.id.navTag -> R.string.tab_bookmark
            R.id.navActivity -> R.string.tab_active
            R.id.navSettings -> R.string.tab_set
            else -> R.string.tab_active
        }

        // 使用 setCompoundDrawablesRelativeWithIntrinsicBounds 并保持 padding
        binding.ivNav.setCompoundDrawablesRelativeWithIntrinsicBounds(0, iconRes, 0, 0)
        binding.ivNav.compoundDrawablePadding = (4 * resources.displayMetrics.density).toInt()
        binding.ivNav.setText(textRes)
        binding.ivNav.isChecked = false

        // 显示 llSearchMode
        binding.llSearchMode.visibility = View.VISIBLE
        binding.llSearchMode.alpha = 0f

        // 创建动画
        val duration = 300L

        // ivSearch 放大到 llEdit 的宽度
        val searchStartWidth = originalSearchWidth
        val searchEndWidth = binding.llEdit.width

        // rgNavigation 缩小到 flNavButton 的宽度
        val navStartWidth = originalNavWidth
        val navEndWidth = binding.flNavButton.width

        ValueAnimator.ofFloat(0f, 1f).apply {
            this.duration = duration
            interpolator = DecelerateInterpolator()

            addUpdateListener { animator ->
                val fraction = animator.animatedValue as Float

                // ivSearch 宽度变化
                val currentSearchWidth = (searchStartWidth + (searchEndWidth - searchStartWidth) * fraction).toInt()
                binding.ivSearch.layoutParams = binding.ivSearch.layoutParams.apply {
                    width = currentSearchWidth
                }

                // rgNavigation 父容器宽度变化
                (binding.rgNavigation.parent as? View)?.let { navParent ->
                    navParent.layoutParams = navParent.layoutParams.apply {
                        width = (navStartWidth + (navEndWidth - navStartWidth) * fraction).toInt()
                    }
                }

                // llSearchMode 透明度变化
                binding.llSearchMode.alpha = fraction

                // llNav 透明度变化
                binding.llNav.alpha = 1f - fraction
            }

            addListener(object : AnimatorListenerAdapter() {
                override fun onAnimationEnd(animation: Animator) {
                    binding.llNav.visibility = View.GONE
                }
            })

            start()
        }
    }

    /**
     * 退出搜索模式
     */
    private fun exitSearchMode() {
        if (!isSearchMode) return
        isSearchMode = false

        // 显示 llNav
        binding.llNav.visibility = View.VISIBLE
        binding.llNav.alpha = 0f

        // 创建动画
        val duration = 300L

        // ivSearch 从当前宽度缩小回原始大小
        val searchStartWidth = binding.ivSearch.width
        val searchEndWidth = originalSearchWidth

        // rgNavigation 从当前宽度放大回原始大小
        val navStartWidth = (binding.rgNavigation.parent as View).width
        val navEndWidth = originalNavWidth

        ValueAnimator.ofFloat(0f, 1f).apply {
            this.duration = duration
            interpolator = DecelerateInterpolator()

            addUpdateListener { animator ->
                val fraction = animator.animatedValue as Float

                // ivSearch 宽度变化
                val currentSearchWidth = (searchStartWidth + (searchEndWidth - searchStartWidth) * fraction).toInt()
                binding.ivSearch.layoutParams = binding.ivSearch.layoutParams.apply {
                    width = currentSearchWidth
                }

                // rgNavigation 父容器宽度变化
                (binding.rgNavigation.parent as? View)?.let { navParent ->
                    navParent.layoutParams = navParent.layoutParams.apply {
                        width = (navStartWidth + (navEndWidth - navStartWidth) * fraction).toInt()
                    }
                }

                // llNav 透明度变化
                binding.llNav.alpha = fraction

                // llSearchMode 透明度变化
                binding.llSearchMode.alpha = 1f - fraction
            }

            addListener(object : AnimatorListenerAdapter() {
                override fun onAnimationEnd(animation: Animator) {
                    binding.llSearchMode.visibility = View.GONE
                }
            })

            start()
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
            viewModel.exitFullScreen(tabId)
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

        // 保存 ActiveList 到数据库
        viewModel.saveActiveListToDatabase()
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
