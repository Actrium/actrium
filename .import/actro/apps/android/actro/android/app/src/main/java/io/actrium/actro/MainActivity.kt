package io.actrium.actro

import android.animation.ObjectAnimator
import android.animation.ValueAnimator
import android.animation.AnimatorListenerAdapter
import android.animation.Animator
import android.graphics.Color
import android.content.Intent
import android.content.res.Configuration
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
import androidx.appcompat.app.AppCompatDelegate
import androidx.core.content.ContextCompat
import androidx.core.widget.TextViewCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.updateLayoutParams
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.lifecycleScope
import androidx.lifecycle.repeatOnLifecycle
import androidx.recyclerview.widget.LinearLayoutManager
import io.actrium.actro.data.database.AppDatabase
import io.actrium.actro.data.repository.ActiveRepository
import io.actrium.actro.data.sync.BrowserSyncStack
import io.actrium.actro.core.ReactNativeBundle
import io.actrium.actro.databinding.ActivityMainBinding
import io.actrium.actro.global.Params
import io.actrium.actro.model.ActiveModel
import io.actrium.actro.utils.RuntimeBridgeParams
import com.facebook.react.ReactRootView
import com.facebook.react.modules.core.DefaultHardwareBackBtnHandler
import com.facebook.react.modules.core.PermissionAwareActivity
import com.facebook.react.modules.core.PermissionListener
import com.facebook.react.uimanager.PixelUtil.pxToDp
import java.io.File
import kotlinx.coroutines.launch
import io.actrium.actro.utils.adjustKeyboard
import io.actrium.actro.utils.getStatusBarHeight
import io.actrium.actro.utils.screenshot
import io.actrium.actro.utils.ViewAnimationHelper
import android.content.Context
import android.view.LayoutInflater
import android.view.inputmethod.InputMethodManager
import android.widget.ImageView
import android.widget.PopupWindow

class MainActivity : AppCompatActivity(), DefaultHardwareBackBtnHandler, PermissionAwareActivity {
    private lateinit var binding: ActivityMainBinding
    private val viewModel: MainViewModel by viewModels {
        val database = AppDatabase.getDatabase(applicationContext)
        val repository = ActiveRepository(database.activeDao())
        // Browser sync stack is an app-level singleton; do not build a local one here,
        // otherwise multiple schedulers would race on the same SQLite file.
        val browserDataRepository = BrowserSyncStack.getInstance(applicationContext).repository
        MainViewModelFactory(repository, browserDataRepository)
    }
    private var reactRootView: ReactRootView? = null
    private val appViews = mutableMapOf<String, ReactRootView>()
    private var isReactNativeVisible = false
    private var isSearchVisible = false
    private var isLayoutComplete = false
    private var permissionListener: PermissionListener? = null
    private lateinit var activeRecyclerAdapter: io.actrium.actro.adapter.ActiveRecyclerAdapter
    private var isSearchMode = false
    private var isFromDeepLink = false
    private var originalSearchWidth = 0
    private var originalNavWidth = 0
    private var currentNightMode = 0

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        currentNightMode = resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK

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
            // 应用左、右、底部的内边距，保留顶部不做处理，以实现内容延伸到状态栏
            // 底部 padding 确保整个布局位于虚拟按键上方
            v.setPadding(systemBars.left, 0, systemBars.right, navBars.bottom)
            // 根布局的 paddingBottom 已处理虚拟按键高度，flNavBar 不再需要额外 bottomMargin，
            // 否则间距会重复计算（padding + margin = 两倍虚拟按键高度）
            // 键盘弹出时 IME inset 已包含虚拟按键高度（adjustKeyboard 会将其作为 padding）
            val isImeVisible = imeBars.bottom > 0
            binding.flNavBar.updateLayoutParams<ViewGroup.MarginLayoutParams> {
                bottomMargin = 0
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
        binding.flContent.adjustKeyboard()

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
            showSearch()
        }

        binding.flNavButton.setOnClickListener {
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
        activeRecyclerAdapter = io.actrium.actro.adapter.ActiveRecyclerAdapter()
        binding.recyApps.apply {
            layoutManager = LinearLayoutManager(this@MainActivity)
            adapter = activeRecyclerAdapter
        }

        activeRecyclerAdapter.onMarkClick = { activeModel ->
            viewModel.toggleMarkStatus(activeModel)
        }
        activeRecyclerAdapter.onItemLongClick = { activeModel, rawX, rawY ->
            showLongPressPopup(activeModel, rawX, rawY)
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
                val targetView = createReactRootView(ReactNativeBundle.REACT_COMPONENT_NAME, bundle)
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
            // 播放删除动画，动画结束后再更新 ViewModel
            activeRecyclerAdapter.removeItemWithAnimation(activeModel.id) {
                viewModel.removeActiveModel(activeModel.id)
            }
            // 不再在这里 finish，由 updateData 回调判断
        }

        binding.ivAdd.postDelayed({
            if (isFromDeepLink) {
                // deeplink 进入时不自动创建或打开，由 handleDeepLink 处理
                isFromDeepLink = false
            } else if (viewModel.isActiveListEmpty()) {
                binding.ivAdd.performClick()
            } else {
                // 列表不为空时自动点击第一个 item
                activeRecyclerAdapter.getItemAt(0)?.let { activeModel ->
                    activeRecyclerAdapter.onItemClick?.invoke(activeModel, binding.recyApps)
                }
            }
        }, 500)

        lifecycleScope.launch {
            repeatOnLifecycle(Lifecycle.State.STARTED) {
                launch {
                    viewModel.currentReactNativeStatus.collect {
                        val currentTabId = it.getString(RuntimeBridgeParams.rootCurrentTabIdKey)
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
                            val targetView = createReactRootView(ReactNativeBundle.REACT_COMPONENT_NAME, it)
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
                                if (isSearchVisible) {
                                    hideSearch {
                                        binding.navTag.isChecked = true
                                        showBookmark(viewModel.pageBundle)
                                    }
                                } else {
                                    binding.navTag.isChecked = true
                                    showBookmark(viewModel.pageBundle)
                                }
                            }

                            ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_SETTINGS -> {
                                if (isSearchVisible) {
                                    hideSearch {
                                        binding.navSettings.isChecked = true
                                        showSet(viewModel.pageBundle)
                                    }
                                } else {
                                    binding.navSettings.isChecked = true
                                    showSet(viewModel.pageBundle)
                                }
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
                        binding.llEmptyView.visibility = if (activeModels.isEmpty()) View.VISIBLE else View.GONE
                    }
                }
            }
        }

        // 处理首次启动时携带的 deeplink
        handleDeepLink(intent)
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
                // 根据 activeList 是否为空决定空白视图显隐
                binding.llEmptyView.visibility = if (viewModel.isActiveListEmpty()) View.VISIBLE else View.GONE
            }

            R.id.navSettings -> if (showPage) showSet()
        }
        // 切换到非活动页时隐藏空白视图
        if (checkedId != R.id.navActivity) {
            binding.llEmptyView.visibility = View.GONE
        }
    }

    private fun showBookmark(sourceBundle: Bundle? = null) {
        binding.flBookmark.visibility = View.VISIBLE
        binding.llActive.visibility = View.GONE
        binding.llEmptyView.visibility = View.GONE
        binding.flSet.visibility = View.GONE
        val bundle = sourceBundle ?: ReactNativeBundle.getBookmarkBundle()
        if (binding.flBookmark.childCount == 0) {
            val targetView = createReactRootView(ReactNativeBundle.REACT_COMPONENT_NAME, bundle)
            binding.flBookmark.addView(targetView)
        } else {
            viewModel.updateBookmark(bundle)
        }
        Log.d("MainActivity", "Show Bookmark")
    }

    private fun showSet(sourceBundle: Bundle? = null) {
        binding.flBookmark.visibility = View.GONE
        binding.llActive.visibility = View.GONE
        binding.llEmptyView.visibility = View.GONE
        binding.flSet.visibility = View.VISIBLE
        val bundle = sourceBundle ?: ReactNativeBundle.getSetBundle()
        if (binding.flSet.childCount == 0) {
            val targetView = createReactRootView(ReactNativeBundle.REACT_COMPONENT_NAME, bundle)
            binding.flSet.addView(targetView)
        } else  {
            viewModel.updateSet(bundle)
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
//        binding.ivNav.setCompoundDrawablesRelativeWithIntrinsicBounds(0, iconRes, 0, 0)
        binding.ivNav.compoundDrawablePadding = (4 * resources.displayMetrics.density).toInt()
        binding.ivNav.setText(textRes)
        binding.ivNav.isChecked = false

        // 设置 ivNav 的 drawable tint 颜色为 labels_vibrant_controls_primary（RadioButton用setButtonDrawable和setCompoundDrawables）
        val topDrawable = ContextCompat.getDrawable(this, iconRes)
        topDrawable?.setTint(ContextCompat.getColor(this, R.color.labels_vibrant_controls_primary))
        binding.ivNav.setCompoundDrawablesWithIntrinsicBounds(null, topDrawable, null, null)
        binding.ivNav.setTextColor(ContextCompat.getColor(this, R.color.labels_vibrant_controls_primary))

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
    private fun exitSearchMode(onEnd: (() -> Unit)? = null) {
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

        //rgNavigation 从当前宽度放大回原始大小
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
                    onEnd?.invoke()
                }
            })

            start()
        }
    }

    /**
     * 从底部显示搜索视图
     */
    private fun showSearch(sourceBundle: Bundle? = null) {
        if (isSearchVisible) return

        isSearchVisible = true
        enterSearchMode()
        // 清空搜索输入框
        binding.etSearch.text.clear()
        // 移除 flSearch 中的所有子组件
        binding.flSearch.removeAllViews()

        // 加载搜索 React Native 视图
        val bundle = sourceBundle ?: ReactNativeBundle.getSearchBundle()
        Log.e("MainActivity", "bundle:${bundle}")
        val targetView = createReactRootView(ReactNativeBundle.REACT_COMPONENT_NAME, bundle)
        binding.flSearch.addView(targetView)

        // 初始位置在屏幕底部
        binding.flSearch.translationY = binding.flSearch.height.toFloat()
        binding.flSearch.visibility = View.VISIBLE
        // 为 flSearch 添加 WindowInsets 监听，处理键盘问题
        binding.flSearch.adjustKeyboard()
        // 请求应用 window insets
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.KITKAT) {
            binding.flSearch.requestApplyInsets()
        }

        // Ensure the search EditText gets focus and the keyboard is shown.
        // Post to the view queue so that focus/keyboard request happens after layout/visibility changes.
        binding.etSearch.post {
            try {
                binding.etSearch.isFocusableInTouchMode = true
                binding.etSearch.requestFocus()
                val imm = getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
                imm.showSoftInput(binding.etSearch, InputMethodManager.SHOW_IMPLICIT)
            } catch (e: Exception) {
                Log.e("MainActivity", "Failed to show keyboard for etSearch", e)
            }
        }

        // 创建从底部滑入的动画
        ObjectAnimator.ofFloat(binding.flSearch, "translationY", binding.flSearch.height.toFloat(), 0f).apply {
            duration = 300
            interpolator = DecelerateInterpolator()
            start()
        }
    }

    /**
     * 隐藏搜索视图到底部
     */
    private fun hideSearch(onSearchHidden: (() -> Unit)? = null) {
        if (!isSearchVisible) return
        exitSearchMode {
            alignIndicatorToSelectedButton(animate = false)
            onSearchHidden?.invoke()
        }

        // 收起键盘
        val imm = getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        binding.etSearch.clearFocus()
        imm.hideSoftInputFromWindow(binding.etSearch.windowToken, 0)

        // 创建滑出到底部的动画
        ObjectAnimator.ofFloat(binding.flSearch, "translationY", 0f, binding.flSearch.height.toFloat()).apply {
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
                    val position = activeRecyclerAdapter.findPositionById(tabId)
                    if (position != -1) {
                        activeRecyclerAdapter.notifyItemChanged(position)
                    }
                }
            }
        }

        val targetRect = calculateItemRectByTabId(tabId)
        if (targetRect != null) {
            ViewAnimationHelper.animateHide(binding.flReactNative, targetRect, 26f) {
                binding.flReactNative.visibility = View.GONE
                isReactNativeVisible = false
            }
        } else {
            ViewAnimationHelper.animateHide(binding.flReactNative, null as View?) {
                binding.flReactNative.visibility = View.GONE
                isReactNativeVisible = false
            }
        }
    }

    /**
     * 通过 tabId 计算 recyApps 中对应 item 在 window 中的位置和尺寸
     */
    private fun calculateItemRectByTabId(tabId: String?): android.graphics.Rect? {
        if (tabId.isNullOrEmpty()) return null
        val position = activeRecyclerAdapter.findPositionById(tabId)
        if (position == -1) return null

        val recyApps = binding.recyApps
        val density = resources.displayMetrics.density

        // item 固定尺寸：高度 200dp，上下 margin 8dp，左右 margin 16dp
        val itemHeightPx = (200 * density).toInt()
        val itemMarginVPx = (8 * density).toInt()
        val itemMarginHPx = (16 * density).toInt()
        val itemWidthPx = recyApps.width - itemMarginHPx * 2

        // 计算 item 在 recyApps 内部的 top 位置
        // recyApps 有 paddingTop=80dp，item 间距为 itemMarginVPx*2
        val paddingTopPx = recyApps.paddingTop
        val itemTopInRecycler = paddingTopPx + position * (itemHeightPx + itemMarginVPx * 2) + itemMarginVPx
        val itemLeftInRecycler = itemMarginHPx

        // 考虑 RecyclerView 的滚动偏移
        val scrollOffset = recyApps.computeVerticalScrollOffset()

        // 将 recyApps 坐标转换为 window 坐标
        val recyclerLoc = IntArray(2)
        recyApps.getLocationInWindow(recyclerLoc)

        val left = recyclerLoc[0] + itemLeftInRecycler
        val top = recyclerLoc[1] + itemTopInRecycler - scrollOffset
        val right = left + itemWidthPx
        val bottom = top + itemHeightPx

        return android.graphics.Rect(left, top, right, bottom)
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
        // 同步当前配置到 RN AppearanceModule，确保深色模式切换后被感知
        application.reactNativeHost.reactInstanceManager.onConfigurationChanged(this, resources.configuration)
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        val newNightMode = newConfig.uiMode and Configuration.UI_MODE_NIGHT_MASK
        if (currentNightMode != newNightMode) {
            currentNightMode = newNightMode
            applyThemeToViews()
        }
        val application = application as MainApplication
        application.reactNativeHost.reactInstanceManager.onConfigurationChanged(this, newConfig)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleDeepLink(intent)
    }

    /**
     * 根据 uiMode 变化刷新所有原生视图的颜色和背景
     * 当 configChanges 包含 uiMode 时，Activity 不会 recreate，
     * 已渲染的 View 保留旧的颜色值，需要手动从更新后的 resources 重新读取
     */
    private fun applyThemeToViews() {
        val isDark = (currentNightMode == Configuration.UI_MODE_NIGHT_YES)

        // 更新状态栏图标颜色：浅色模式用深色图标，深色模式用浅色图标
        WindowCompat.getInsetsController(window, window.decorView).isAppearanceLightStatusBars = !isDark

        // 重新应用导航栏背景（引用 @color/nav_bg，浅色=白色，深色=灰色）
        (binding.rgNavigation.parent as? View)?.background =
            ContextCompat.getDrawable(this, R.drawable.bottom_nav_bg)
        binding.flNavIndicator.background =
            ContextCompat.getDrawable(this, R.drawable.bottom_nav_indicator)

        // 重新应用 RadioButton 的文字颜色和图标 tint（引用 @color/nav_text_selector）
        val navTextSelector = ContextCompat.getColorStateList(this, R.color.nav_text_selector)
        listOf(binding.navTag, binding.navActivity, binding.navSettings).forEach { rb ->
            rb.setTextColor(navTextSelector)
            TextViewCompat.setCompoundDrawableTintList(rb, navTextSelector)
        }

        // 重新应用搜索图标背景和 tint（引用 @drawable/bottom_nav_bg, @color/labels_vibrant_controls_primary）
        binding.ivSearch.background = ContextCompat.getDrawable(this, R.drawable.bottom_nav_bg)
        binding.ivSearch.imageTintList =
            ContextCompat.getColorStateList(this, R.color.labels_vibrant_controls_primary)

        // 重新应用搜索模式相关视图
        binding.flNavButton.background = ContextCompat.getDrawable(this, R.drawable.bottom_nav_bg)
        binding.llEdit.background = ContextCompat.getDrawable(this, R.drawable.bottom_nav_bg)
        binding.ivSearchIcon.imageTintList =
            ContextCompat.getColorStateList(this, R.color.labels_vibrant_controls_primary)

        // 如果当前在搜索模式，更新 ivNav 颜色
        if (isSearchMode) {
            val labelColor = ContextCompat.getColor(this, R.color.labels_vibrant_controls_primary)
            val checkedId = binding.rgNavigation.checkedRadioButtonId
            val iconRes = when (checkedId) {
                R.id.navTag -> R.drawable.nav_tag_selector
                R.id.navSettings -> R.drawable.nav_settings_selector
                else -> R.drawable.nav_activity_selector
            }
            val topDrawable = ContextCompat.getDrawable(this, iconRes)
            topDrawable?.setTint(labelColor)
            binding.ivNav.setCompoundDrawablesWithIntrinsicBounds(null, topDrawable, null, null)
            binding.ivNav.setTextColor(labelColor)
        }

        // 刷新 RecyclerView 列表项
        activeRecyclerAdapter.notifyDataSetChanged()
    }

    companion object {
        /**
         * 切换深色/浅色模式
         * @param mode AppCompatDelegate.MODE_NIGHT_FOLLOW_SYSTEM / MODE_NIGHT_NO / MODE_NIGHT_YES
         */
        fun switchTheme(mode: Int) {
            AppCompatDelegate.setDefaultNightMode(mode)
        }
    }

    /**
     * 解析 deeplink 并将数据发送给 React Native
     * 链接格式示例:
     *   actro://chat/123?token=abc
     *   https://app.actro.chat/chat/123?token=abc
     */
    private fun handleDeepLink(intent: Intent?) {
        val uri = intent?.data ?: return
        Log.d("MainActivity", "DeepLink received: $uri")
        val appId = uri.getQueryParameter("appid")
        isFromDeepLink = true
        viewModel.addApp(appId = appId)
    }

    private var longPressPopup: PopupWindow? = null

    /**
     * 长按 item 弹出操作菜单
     * 以触点为中心定位，与屏幕边界保持 20px 间距
     */
    private fun showLongPressPopup(activeModel: ActiveModel, rawX: Int, rawY: Int) {
        longPressPopup?.dismiss()

        val popupView = LayoutInflater.from(this).inflate(R.layout.popup_long_press, null)
        val llFav = popupView.findViewById<View>(R.id.llFav)
        val ivFav = popupView.findViewById<ImageView>(R.id.ivFav)
        val tvFav = popupView.findViewById<android.widget.TextView>(R.id.tvFav)
        val llCloseOther = popupView.findViewById<View>(R.id.llCloseOther)
        val vDivider = popupView.findViewById<View>(R.id.vDivider)

        // name 为空时不显示收藏行（与 item 中 ivMark 的显隐逻辑一致）
        val showFav = activeModel.name.isNotEmpty()
        llFav.visibility = if (showFav) View.VISIBLE else View.GONE

        // 只有当前 item 时不显示"关闭其他页签"
        val hasOtherItems = viewModel.activeList.value.size > 1
        llCloseOther.visibility = if (hasOtherItems) View.VISIBLE else View.GONE
        vDivider.visibility = if (hasOtherItems) View.VISIBLE else View.GONE

        // 根据收藏状态切换图标和文字
        if (activeModel.isMarked) {
            ivFav.setImageResource(R.drawable.menu_cancel_fav)
            tvFav.text = getString(R.string.menu_cancel_bookmark)
        } else {
            ivFav.setImageResource(R.drawable.menu_fav)
            tvFav.text = getString(R.string.menu_bookmark)
        }

        val popup = PopupWindow(
            popupView,
            ViewGroup.LayoutParams.WRAP_CONTENT,
            ViewGroup.LayoutParams.WRAP_CONTENT,
            true
        )
        popup.isOutsideTouchable = true
        longPressPopup = popup

        llFav.setOnClickListener {
            popup.dismiss()
            // 与 item 上的收藏操作一致
            activeRecyclerAdapter.onMarkClick?.invoke(activeModel)
        }
        popupView.findViewById<View>(R.id.llClose).setOnClickListener {
            popup.dismiss()
            // 与 item 上的删除操作一致
            activeRecyclerAdapter.onDeleteClick?.invoke(activeModel)
        }
        llCloseOther.setOnClickListener {
            closeOtherTabs(activeModel.id)
            popup.dismiss()
        }

        // 测量弹窗尺寸
        popupView.measure(View.MeasureSpec.UNSPECIFIED, View.MeasureSpec.UNSPECIFIED)
        val popupWidth = popupView.measuredWidth
        val popupHeight = popupView.measuredHeight

        val margin = 20
        val screenWidth = resources.displayMetrics.widthPixels
        val screenHeight = resources.displayMetrics.heightPixels

        // 以触点为中心计算位置，再约束到边界内
        val x = (rawX - popupWidth / 2)
            .coerceAtLeast(margin)
            .coerceAtMost(screenWidth - popupWidth - margin)

        val y = (rawY - popupHeight / 2)
            .coerceAtLeast(margin)
            .coerceAtMost(screenHeight - popupHeight - margin)

        popup.showAtLocation(window.decorView, android.view.Gravity.NO_GRAVITY, x, y)
    }

    /**
     * 关闭除指定 tab 以外的所有 tab
     */
    private fun closeOtherTabs(keepId: String) {
        val allItems = viewModel.activeList.value
        val others = allItems.filter { it.id != keepId }
        for (model in others) {
            // 移除 flRNContent 中对应的 child
            for (i in binding.flRNContent.childCount - 1 downTo 0) {
                val child = binding.flRNContent.getChildAt(i)
                if (child.tag == model.id) {
                    binding.flRNContent.removeViewAt(i)
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
            viewModel.removeActiveModel(model.id)
        }
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
