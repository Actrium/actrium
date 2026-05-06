package chat.askaway.android.tablet

import android.content.Intent
import android.os.Bundle
import android.util.Log
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.widget.LinearLayout
import android.view.animation.AccelerateDecelerateInterpolator
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.core.view.isVisible
import androidx.core.view.postDelayed
import androidx.lifecycle.lifecycleScope
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import android.content.res.Configuration
import com.askaway.android.BuildConfig
import com.askaway.android.MainApplication
import com.askaway.android.R
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.askaway.android.databinding.ActivityTabletBinding
import com.facebook.react.ReactRootView
import com.facebook.react.modules.core.DefaultHardwareBackBtnHandler
import com.facebook.react.modules.core.PermissionAwareActivity
import com.facebook.react.modules.core.PermissionListener
import com.facebook.react.uimanager.PixelUtil.pxToDp
import com.facebook.react.uimanager.PixelUtil.dpToPx
import com.askaway.android.core.ReactNativeBundle
import android.content.Context
import android.view.inputmethod.InputMethodManager
import com.askaway.android.utils.screenshot
import com.askaway.android.utils.adjustKeyboard
import chat.askaway.android.utils.TabBundleUtils

class TabletActivity : AppCompatActivity(), DefaultHardwareBackBtnHandler, PermissionAwareActivity {

    private lateinit var binding: ActivityTabletBinding
    private lateinit var adapter: ActiveAdapter
    private lateinit var tabContentAdapter: TabContentAdapter
    private lateinit var gridAdapter: GridContentAdapter
    private var permissionListener: PermissionListener? = null
    private lateinit var viewModel: TabletViewModel
    private val pagerLayoutManager = TabPagerLayoutManager(this)
    private val gridLayoutManager: TabGridLayoutManager by lazy {
        TabGridLayoutManager(this, 16f / 9f)
    }
    private var gridItemDecoration: GridItemDecoration? = null
    private val gridSpacingPx: Int
        get() = (16 * resources.displayMetrics.density).toInt()
    private val gridSpanCount: Int
        get() = if (resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE) 4 else 3
    private val reactViewMap = mutableMapOf<String, ReactRootView>()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityTabletBinding.inflate(layoutInflater)
        setContentView(binding.root)
        viewModel = TabletViewModel(applicationContext)
        // 初始化顶部应用列表 adapter
        adapter = ActiveAdapter()
        binding.rvApp.layoutManager = LinearLayoutManager(this, LinearLayoutManager.HORIZONTAL, false)
        binding.rvApp.adapter = adapter

        // 初始化内容区域 adapter
        tabContentAdapter = TabContentAdapter(reactViewMap)
        gridAdapter = GridContentAdapter()

        binding.rvTabContent.layoutManager = pagerLayoutManager
        binding.rvTabContent.adapter = tabContentAdapter

        binding.rvGridContent.layoutManager = gridLayoutManager
        binding.rvGridContent.adapter = gridAdapter

        val optionHeightPx = (32 * resources.displayMetrics.density).toInt()
        gridItemDecoration = GridItemDecoration(gridSpanCount, gridSpacingPx, 16f / 9f, optionHeightPx)
        binding.rvGridContent.addItemDecoration(gridItemDecoration!!)

        binding.flContent.addOnLayoutChangeListener { v, left, top, right, bottom, oldLeft, oldTop, oldRight, oldBottom ->
            val newWidth = right - left
            val newHeight = bottom - top
            val oldWidth = oldRight - oldLeft
            val oldHeight = oldBottom - oldTop
            if (newWidth == oldWidth && newHeight == oldHeight) return@addOnLayoutChangeListener
            viewModel.changeSize(newWidth.pxToDp().toInt(), newHeight.pxToDp().toInt())

            // 更新容器尺寸
            if (newWidth > 0 && newHeight > 0) {
                Log.e("TabletActivity", "newWidth:${newWidth}, newHeight:${newHeight}")

                val marginHorizontalPx = (viewModel.containerWidth * resources.displayMetrics.density * 0.1).toInt()

                gridAdapter.containerWidth = newWidth
                gridAdapter.containerHeight = newHeight
                gridAdapter.spanCount = gridSpanCount
                gridAdapter.spacingPx = gridSpacingPx
                gridAdapter.marginHorizontalPx = marginHorizontalPx

                viewModel.containerWidth = newWidth.pxToDp().toInt()
                viewModel.containerHeight = newHeight.pxToDp().toInt()

                // 更新 GridLayoutManager 的 aspectRatio
                val newAspectRatio = newWidth.toFloat() / newHeight.toFloat()
                gridLayoutManager.updateAspectRatio(newAspectRatio)

                // 更新 GridItemDecoration 的宽高比和 optionHeight
                gridItemDecoration?.let {
                    binding.rvGridContent.removeItemDecoration(it)
                }
                val optionHeightPxUpdate = (32 * resources.displayMetrics.density).toInt()
                gridItemDecoration = GridItemDecoration(gridSpanCount, gridSpacingPx, newAspectRatio, optionHeightPxUpdate)
                binding.rvGridContent.addItemDecoration(gridItemDecoration!!)

                // 刷新 grid adapter 布局
                gridAdapter.notifyDataSetChanged()

                // 更新 rvGridContent 的 margin
                val gridParams = binding.rvGridContent.layoutParams as? ViewGroup.MarginLayoutParams
                if (gridParams != null) {
                    val marginTop = (16 * resources.displayMetrics.density).toInt()
                    gridParams.topMargin = marginTop
                    gridParams.leftMargin = marginHorizontalPx
                    gridParams.rightMargin = marginHorizontalPx
                    gridParams.bottomMargin = marginTop
                    binding.rvGridContent.layoutParams = gridParams
                }
            }
        }

        // 获取容器尺寸
        binding.rvTabContent.post {
            val w = binding.rvTabContent.width
            val h = binding.rvTabContent.height
            val marginHorizontalPx = (viewModel.containerWidth * resources.displayMetrics.density * 0.1).toInt()

            gridAdapter.containerWidth = w
            gridAdapter.containerHeight = h
            gridAdapter.spanCount = gridSpanCount
            gridAdapter.spacingPx = gridSpacingPx
            gridAdapter.marginHorizontalPx = marginHorizontalPx

            viewModel.containerWidth = w.pxToDp().toInt()
            viewModel.containerHeight = h.pxToDp().toInt()
        }
        binding.rvTabContent.adjustKeyboard()

        // 设置初始模式为 TAB
        changeMode(PageMode.TAB)

        adapter.onItemClick = {
            viewModel.changeActiveModel(it.id)
        }
        adapter.onDeleteClick = {
            // 从 reactViewMap 中移除并卸载
            val reactRootView = reactViewMap.remove(it.id)
            reactRootView?.let { rootView ->
                try {
                    rootView.unmountReactApplication()
                } catch (e: Exception) {
                    Log.e("TabletActivity", "Error unmounting React application", e)
                }
            }
            // 移除当前 item
            viewModel.removeActiveModel(it.id)
            // 如果 reactViewMap 为空，则切换为 Grid 模式
            if (reactViewMap.isEmpty()) {
                viewModel.changeDisplayMode(PageMode.GRID)
            }
        }

        // Grid adapter 的点击事件
        gridAdapter.onItemClick = { position ->
            // 在网格模式下点击，切换到 TAB 模式并滚动到对应位置
            viewModel.changeDisplayMode(PageMode.TAB)
            binding.rvTabContent.scrollToPosition(position)
            // 更新当前选中的 app
            val appList = viewModel.tabletUseCase.value?.appList
            if (appList != null && position < appList.size) {
                viewModel.changeActiveModel(appList[position].id)
            }
        }

        gridAdapter.onAddClick = {
            viewModel.changeDisplayMode(PageMode.TAB)
            viewModel.addApp()
        }

        gridAdapter.onDeleteClick = { tabId ->
            // 从 reactViewMap 中移除并卸载
            val reactRootView = reactViewMap.remove(tabId)
            reactRootView?.let { rootView ->
                try {
                    rootView.unmountReactApplication()
                } catch (e: Exception) {
                    Log.e("TabletActivity", "Error unmounting React application", e)
                }
            }
            viewModel.removeActiveModel(tabId)
        }
        gridAdapter.onMarkClick = { activeModel ->
            viewModel.toggleMarkStatus(activeModel)
        }

        // 监听 RecyclerView 的布局变化，更新 ivAdd 位置
        binding.rvApp.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ ->
            updateAddButtonPosition()
        }

        // 监听 RecyclerView 的滚动，更新 ivAdd 位置
        binding.rvApp.addOnScrollListener(object : RecyclerView.OnScrollListener() {
            override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
                updateAddButtonPosition()
            }
        })
        binding.ivHome.setOnClickListener {
            binding.navActivity.isChecked = true
            viewModel.changeDisplayMode(PageMode.GRID)
        }
        binding.ivAppMenu.setOnClickListener { viewModel.toggleMenu() }
        binding.tvBack.setOnClickListener {
            Log.e("TableActivity", "tvBackClick:${reactViewMap.size}")
            if (reactViewMap.isNotEmpty()) {
                viewModel.hideBookmark()
                viewModel.changeDisplayMode(PageMode.TAB)
            }
        }

        // 监听 StateFlow 并更新 Adapter
        lifecycleScope.launchWhenStarted {
            viewModel.tabletUseCase.collect { useCase ->
                if (useCase != null) {
                    for (item in useCase.appList) {
                        if (reactViewMap[item.id] == null) {
                            // 创建新的 ReactRootView
                            val bundle = TabBundleUtils.genBundle(
                                item,
                                useCase.appList,
                                viewModel.containerHeight,
                                viewModel.containerWidth
                            )
                            val targetView = createReactRootView("canvas-kotlin", bundle)
                            reactViewMap[item.id] = targetView
                        }
                    }
                    // 为当前 app 创建 ReactRootView（如果还没有）
                    useCase.currentApp?.let { bundle ->
                        Log.e("TabletActivity", "bundle:${bundle}")
                        val currentTabId = bundle.getString("currentTabId")
                        if (currentTabId != null) {
                            if (reactViewMap[currentTabId] == null) {
                                // 创建新的 ReactRootView
                                val targetView = createReactRootView("canvas-kotlin", bundle)
                                reactViewMap[currentTabId] = targetView
                                Log.d("TabletActivity", "Created new ReactRootView for $currentTabId ${bundle}")
                            }
                        }
                    }

                    // 更新顶部应用列表
                    adapter.updateData(useCase.appList)
                    binding.rvApp.post {
                        updateAddButtonPosition()
                    }

                    // 更新内容区域
                    tabContentAdapter.updateItems(useCase.appList)
                    gridAdapter.updateItems(useCase.appList)

                    // 更新当前选中的 app
                    useCase.currentApp?.let {
                        val currentTabId = it.getString("currentTabId")
                        adapter.currentId = currentTabId
                        Log.e("TabletActivity", "adapter.currentId:${adapter.currentId}")

                        // 在 TAB 模式下，滚动到当前选中的 app
                        if (useCase.pageMode == PageMode.TAB && currentTabId != null) {
                            val currentIndex = useCase.appList.indexOfFirst { model -> model.id == currentTabId }
                            Log.e("TabletActivity", "currentIndex:${currentIndex}")
                            if (currentIndex >= 0) {
                                binding.rvTabContent.scrollToPosition(currentIndex)
                            }
                        }
                    }
                    if (useCase.autoCreate) {
                        binding.ivAdd.performClick()
                        viewModel.cancelCreate()
                    }
                    Log.e(
                        "TabletActivity",
                        "pageMode:${useCase.pageMode} currentDisplayModel:${viewModel.currentDisplayModel} "
                    )
                    // 切换模式
                    if (useCase.pageMode != viewModel.currentDisplayModel) {
                        changeMode(useCase.pageMode)
                        viewModel.currentDisplayModel = useCase.pageMode
                    }
                    if (useCase.pageMode == PageMode.TAB && adapter.currentId != null) {
                        val currentIndex = useCase.appList.indexOfFirst { model -> model.id == adapter.currentId }
                        Log.e("TabletActivity", "currentIndex:${currentIndex}")
                        if (currentIndex >= 0) {
                            binding.rvTabContent.scrollToPosition(currentIndex)
                        }
                    }

                    if (useCase.showPageType == ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_SETTINGS) {
                        showSettingsPopup(viewModel.pageBundle.deepCopy())
                    } else {
                        hideSettingsPopup()
                    }
                    if (useCase.showPageType == ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_SEARCH) {
                        Log.e("TabletActivity", "showSearch:${viewModel.pageBundle}")
                        showSearch(viewModel.pageBundle.deepCopy())
                    } else {
                        Log.e("TabletActivity", "hideSearch")
                        hideSearch()
                    }
                    if (useCase.showPageType == ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_BOOKMARK) {
                        showBookmark(viewModel.pageBundle.deepCopy())
                    } else {
                        hideBookmark()
                    }
                    viewModel.pageBundle.clear()
                }
            }
        }
        binding.ivAdd.setOnClickListener {
            viewModel.addApp()
        }
        // 设置按钮点击事件
        binding.ivSet.setOnClickListener {
            viewModel.showSet()
        }
        binding.llSearch.setOnClickListener {
            Log.e("TabletActivity", "llSearch click")
            viewModel.showSearch()
        }
        binding.navTag.setOnCheckedChangeListener { _, isChecked ->
            if (isChecked) {
                // 设置 navTag 背景为 bottom_nav_indicator_dark
                binding.navTag.background = ContextCompat.getDrawable(this, R.drawable.bottom_nav_indicator_dark)
                // 设置 navActivity 背景为空
                binding.navActivity.background = null
                viewModel.showBookmark()
            } else {
                // 设置 navTag 背景为空
                binding.navTag.background = null
                // 设置 navActivity 背景为 bottom_nav_indicator_dark
                binding.navActivity.background = ContextCompat.getDrawable(this, R.drawable.bottom_nav_indicator_dark)
                viewModel.hideBookmark()
            }
        }
//        binding.flRNContent.adjustKeyboard()
    }

    override fun onPause() {
        super.onPause()
        val application = application as MainApplication
        application.reactNativeHost.reactInstanceManager.onHostPause(this)
        viewModel.saveActiveListToDatabase()
    }

    override fun onResume() {
        super.onResume()
        val application = application as MainApplication
        application.reactNativeHost.reactInstanceManager.onHostResume(this, this)
    }

    private fun updateAddButtonPosition() {
        val layoutManager = binding.rvApp.layoutManager as? LinearLayoutManager ?: return
        val itemCount = adapter.itemCount

        if (itemCount == 0) {
            // 如果没有 item，将 ivAdd 放在起始位置
            binding.ivAdd.translationX = 0f
            return
        }

        // 获取最后一个可见的 item
        val lastVisiblePosition = layoutManager.findLastVisibleItemPosition()
        val lastItemView = layoutManager.findViewByPosition(lastVisiblePosition)

        if (lastItemView != null) {
            // 计算最后一个 item 的右边缘位置
            val lastItemRight = lastItemView.right.toFloat()

            // 获取父容器的宽度
            val parentWidth = binding.rvApp.parent?.let { (it as android.view.View).width } ?: 0

            // 计算 ivAdd 应该的位置（最后一个 item 的右边）
            val targetX = lastItemRight

            // 确保不超出父容器的边界
            val maxX = parentWidth - binding.ivAdd.width
            val finalX = targetX.coerceAtMost(maxX.toFloat())

            binding.ivAdd.translationX = finalX
        }
    }

    private fun changeMode(mode: PageMode) {
        // 切换到 Grid 模式前，对所有 item 的 ReactRootView 截图并存储到 screenshot 字段
        if (mode == PageMode.GRID) {
            captureScreenshots()
        }

        if (mode == PageMode.TAB) {
            viewModel.enterFullScreen()
            // TAB 模式
            binding.llTitleTab.isVisible = true
            binding.rvTabContent.isVisible = true
            binding.rlTitleGrid.isVisible = false
            binding.llSearch.isVisible = false
            binding.rvGridContent.isVisible = false
        } else {
            viewModel.exitFullScreen()
            // GRID 模式
            binding.rlTitleGrid.isVisible = true
            binding.llSearch.isVisible = true
            binding.llTitleTab.isVisible = false
            binding.rvTabContent.isVisible = false
            binding.rvGridContent.isVisible = true

            // 刷新 grid adapter 以应用截图
            gridAdapter.notifyDataSetChanged()
        }
    }

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
     * 对所有 ReactRootView 截图，并将本地存储路径存储到对应 ActiveModel 的 screenshot 字段。
     */
    private fun captureScreenshots() {
        val appList = viewModel.tabletUseCase.value?.appList ?: return
        for (item in appList) {
            val reactRootView = reactViewMap[item.id]
            if (reactRootView != null && reactRootView.width > 0 && reactRootView.height > 0) {
                try {
                    val screenshotPath = reactRootView.screenshot("${item.id}.jpg")
                    if (screenshotPath != null) {
                        item.screenshot = screenshotPath
                    }
                } catch (e: Exception) {
                    Log.e("TabletActivity", "Failed to capture screenshot for ${item.id}", e)
                }
            }
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

    private fun showSettingsPopup(sourceBundle: Bundle? = null) {
        // 如果已经显示，则不重复显示
        if (binding.flSet.isVisible) return

        // 加载弹窗布局
        val popupView = LayoutInflater.from(this).inflate(R.layout.layout_settings_popup, binding.flSet, false)
        binding.flSet.removeAllViews()
        binding.flSet.addView(popupView)
        // 获取遮罩和内容视图
        val vMask = popupView.findViewById<View>(R.id.vMask)
        val llContent = popupView.findViewById<FrameLayout>(R.id.llContent)
        val bundle = if (sourceBundle != null && !sourceBundle.isEmpty) {
            sourceBundle
        } else {
            ReactNativeBundle.getSetBundle()
        }
        val targetView = createReactRootView("canvas-kotlin", bundle)
        llContent.addView(targetView)

        // 设置初始状态
        binding.flSet.visibility = View.VISIBLE
        vMask.alpha = 0f
        llContent.scaleX = 0.7f
        llContent.scaleY = 0.7f
        llContent.alpha = 0f

        // 执行弹出动画
        vMask.animate()
            .alpha(1f)
            .setDuration(200)
            .start()

        llContent.animate()
            .scaleX(1f)
            .scaleY(1f)
            .alpha(1f)
            .setDuration(250)
            .setInterpolator(AccelerateDecelerateInterpolator())
            .start()

        // 点击遮罩关闭
        vMask.setOnClickListener {
            viewModel.hideSet()
        }
    }

    private fun hideSettingsPopup() {
        if (!binding.flSet.isVisible) return

        val popupView = binding.flSet.getChildAt(0) ?: return
        val vMask = popupView.findViewById<View>(R.id.vMask)
        val llContent = popupView.findViewById<View>(R.id.llContent)
        // 执行关闭动画
        vMask.animate()
            .alpha(0f)
            .setDuration(200)
            .start()

        llContent.animate()
            .scaleX(0.7f)
            .scaleY(0.7f)
            .alpha(0f)
            .setDuration(200)
            .setInterpolator(AccelerateDecelerateInterpolator())
            .withEndAction {
                binding.flSet.visibility = View.GONE
                binding.flSet.removeAllViews()
            }
            .start()
    }

    private fun showSearch(sourceBundle: Bundle? = null) {
        if (binding.flSearch.isVisible) return
        binding.flSearch.isVisible = true
        binding.flSearch.removeAllViews()
        val bundle = if (sourceBundle != null && !sourceBundle.isEmpty) {
            sourceBundle
        } else {
            ReactNativeBundle.getSearchBundle()
        }
        val targetView = createReactRootView("canvas-kotlin", bundle)
        binding.flSearch.addView(targetView)
    }

    private fun hideSearch() {
        binding.flSearch.isVisible = false
        hideKeyboard()
    }

    private fun showBookmark(sourceBundle: Bundle? = null) {
        Log.e("TabletActivity", "showBookmark")
        if (binding.flBookmark.isVisible) return
        binding.flBookmark.isVisible = true
        binding.flBookmark.removeAllViews()
        val bundle = if (sourceBundle != null && !sourceBundle.isEmpty) {
            sourceBundle
        } else {
            ReactNativeBundle.getBookmarkBundle()
        }
        val targetView = createReactRootView("canvas-kotlin", bundle)
        binding.flBookmark.addView(targetView)
    }

    private fun hideBookmark() {
        Log.e("TabletActivity", "hideBookmark")
        binding.flBookmark.isVisible = false
    }

    private fun hideKeyboard() {
        val imm = getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        val view = currentFocus ?: window.decorView
        imm.hideSoftInputFromWindow(view.windowToken, 0)
    }
}