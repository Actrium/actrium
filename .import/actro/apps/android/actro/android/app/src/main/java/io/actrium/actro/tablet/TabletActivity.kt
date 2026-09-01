package io.actrium.actro.tablet

import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.graphics.Bitmap
import android.os.Bundle
import android.util.Log
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.view.animation.AccelerateDecelerateInterpolator
import android.view.inputmethod.InputMethodManager
import android.widget.FrameLayout
import android.widget.ImageView
import android.widget.PopupWindow
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.core.view.isVisible
import androidx.lifecycle.lifecycleScope
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.LinearSmoothScroller
import androidx.recyclerview.widget.RecyclerView
import io.actrium.actro.utils.TabBundleUtils
import io.actrium.actro.model.ActiveModel
import io.actrium.actro.BuildConfig
import io.actrium.actro.MainApplication
import io.actrium.actro.R
import io.actrium.actro.core.ReactNativeBundle
import io.actrium.actro.databinding.ActivityTabletBinding
import io.actrium.actro.utils.RuntimeBridgeParams
import io.actrium.actro.utils.ViewAnimationHelper
import io.actrium.actro.utils.adjustKeyboard
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.launch
import java.io.File
import java.io.FileOutputStream
import com.facebook.react.ReactRootView
import com.facebook.react.modules.core.DefaultHardwareBackBtnHandler
import com.facebook.react.modules.core.PermissionAwareActivity
import com.facebook.react.modules.core.PermissionListener
import com.facebook.react.uimanager.PixelUtil.pxToDp

class TabletActivity : AppCompatActivity(), DefaultHardwareBackBtnHandler, PermissionAwareActivity {

    private lateinit var binding: ActivityTabletBinding
    private lateinit var adapter: ActiveAdapter
    private lateinit var gridAdapter: GridContentAdapter
    private var permissionListener: PermissionListener? = null
    private lateinit var viewModel: TabletViewModel
    private val gridLayoutManager: TabGridLayoutManager by lazy {
        TabGridLayoutManager(this, 16f / 9f)
    }
    private var gridItemDecoration: GridItemDecoration? = null
    private val gridSpacingPx: Int
        get() = (16 * resources.displayMetrics.density).toInt()
    private val gridSpanCount: Int
        get() = if (resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE) 4 else 3
    private val reactViewMap = mutableMapOf<String, ReactRootView>()

    // 记录 tab 列表的顺序（id 列表），用于通过 index 找到对应子 View
    private val tabIdList = mutableListOf<String>()

    // Grid → Tab 动画时传入的 sourceView
    private var pendingGridSourceView: View? = null
    private var pendingGridActiveModel: ActiveModel? = null

    // 追踪当前处于全屏状态的 tabId，避免重复调用 enterFullScreen / exitFullScreen
    private var currentFullScreenTabId: String? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityTabletBinding.inflate(layoutInflater)
        setContentView(binding.root)
        viewModel = TabletViewModel(applicationContext)
        // 初始化顶部应用列表 adapter
        adapter = ActiveAdapter()
        binding.rvApp.layoutManager = LinearLayoutManager(this, LinearLayoutManager.HORIZONTAL, false)
        binding.rvApp.adapter = adapter
        // 去除弹性（overscroll）效果，避免滚动到边界时的回弹抖动
        binding.rvApp.overScrollMode = View.OVER_SCROLL_NEVER

        // 选中项变化时，滚动 RecyclerView 使其完全可见
        // 使用自定义 SmoothScroller，让目标 item 对齐到距左边缘 18dp 处，
        // 确保前一个 item 的反圆角始终可见
        adapter.onSelectedChanged = { position ->
            binding.rvApp.post {
                val layoutManager = binding.rvApp.layoutManager as? LinearLayoutManager ?: return@post
                val adapterPosition = position + 1
                val offsetPx = (18 * resources.displayMetrics.density).toInt()
                // 判断目标 item 是否已经完全可见：
                // 1. 左边缘距可见区域 >= offsetPx（反圆角完全可见）
                // 2. 右边缘未超出可见区域（item 未被右侧裁切）
                val childView = layoutManager.findViewByPosition(adapterPosition)
                if (childView != null) {
                    val leftEdge = childView.left - binding.rvApp.paddingLeft
                    val rightEdge = childView.right
                    val visibleRight = binding.rvApp.width - binding.rvApp.paddingRight
                    if (leftEdge >= offsetPx && rightEdge <= visibleRight) {
                        return@post
                    }
                }
                val smoothScroller = object : LinearSmoothScroller(this@TabletActivity) {
                    override fun getHorizontalSnapPreference(): Int = SNAP_TO_START
                    override fun calculateDtToFit(
                        viewStart: Int, viewEnd: Int, boxStart: Int, boxEnd: Int, snapPreference: Int
                    ): Int = boxStart + offsetPx - viewStart
                }
                smoothScroller.targetPosition = adapterPosition
                layoutManager.startSmoothScroll(smoothScroller)
            }
        }

        // 初始化内容区域 adapter
        gridAdapter = GridContentAdapter()

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

                val marginHorizontalPx = (viewModel.containerWidth * resources.displayMetrics.density * 0.05).toInt()

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
        binding.flTabContent.post {
            val w = binding.flTabContent.width
            val h = binding.flTabContent.height
            val marginHorizontalPx = (viewModel.containerWidth * resources.displayMetrics.density * 0.05).toInt()

            gridAdapter.containerWidth = w
            gridAdapter.containerHeight = h
            gridAdapter.spanCount = gridSpanCount
            gridAdapter.spacingPx = gridSpacingPx
            gridAdapter.marginHorizontalPx = marginHorizontalPx

            viewModel.containerWidth = w.pxToDp().toInt()
            viewModel.containerHeight = h.pxToDp().toInt()
        }
        binding.flTabContent.adjustKeyboard()

        // 设置初始模式为 TAB
        changeMode(PageMode.TAB)

        adapter.onItemClick = {
            viewModel.changeActiveModel(it.id)
        }
        adapter.onDeleteClick = {
            deleteTab(it.id)
        }
        adapter.onItemLongClick = { activeModel, rawX, rawY ->
            showLongPressPopup(activeModel, rawX, rawY)
        }

        // Grid adapter 的点击事件
        gridAdapter.onItemClick = { position, sourceView ->
            // 保存 sourceView 和 ActiveModel 供动画使用
            pendingGridSourceView = sourceView
            val appList = viewModel.tabletUseCase.value?.appList
            if (appList != null && position < appList.size) {
                pendingGridActiveModel = appList[position]
            }
            // 在网格模式下点击，切换到 TAB 模式
            viewModel.changeDisplayMode(PageMode.TAB)
            // 更新当前选中的 app
            if (appList != null && position < appList.size) {
                viewModel.changeActiveModel(appList[position].id)
            }
        }

        gridAdapter.onAddClick = {
            viewModel.changeDisplayMode(PageMode.TAB)
            viewModel.addApp()
        }

        gridAdapter.onDeleteClick = { tabId ->
            gridAdapter.removeItemWithAnimation(tabId) {
                deleteTab(tabId)
            }
        }
        gridAdapter.onMarkClick = { activeModel ->
            viewModel.toggleMarkStatus(activeModel)
        }
        gridAdapter.onItemLongClick = { activeModel, rawX, rawY ->
            showLongPressPopup(activeModel, rawX, rawY)
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
                    // 为当前 app 创建 ReactRootView（如果还没有）
                    useCase.currentApp?.let { bundle ->
                        Log.e("TabletActivity", "bundle:${bundle}")
                        val currentTabId =
                            bundle.getString(RuntimeBridgeParams.rootCurrentTabIdKey)
                        if (currentTabId != null) {
                            if (reactViewMap[currentTabId] == null) {
                                // 创建新的 ReactRootView
                                val targetView = createReactRootView(ReactNativeBundle.REACT_COMPONENT_NAME, bundle)
                                reactViewMap[currentTabId] = targetView
                                Log.d("TabletActivity", "Created new ReactRootView for $currentTabId ${bundle}")
                            }
                        }
                    }

                    for (item in useCase.appList) {
                        if (reactViewMap[item.id] == null) {
                            // 创建新的 ReactRootView
                            val bundle = TabBundleUtils.genBundle(
                                item,
                                useCase.appList,
                                viewModel.containerHeight,
                                viewModel.containerWidth
                            )
                            val targetView = createReactRootView(ReactNativeBundle.REACT_COMPONENT_NAME, bundle)
                            reactViewMap[item.id] = targetView
                        }
                    }

                    // 更新顶部应用列表
                    adapter.updateData(useCase.appList)
                    binding.rvApp.post {
                        updateAddButtonPosition()
                    }

                    // 更新内容区域
                    gridAdapter.updateItems(useCase.appList)

                    // 同步 flTabContent 中的子 View
                    syncTabContentViews(useCase.appList)

                    // 更新当前选中的 app
                    useCase.currentApp?.let {
                        val currentTabId =
                            it.getString(RuntimeBridgeParams.rootCurrentTabIdKey)
                        adapter.currentId = currentTabId
                        Log.e("TabletActivity", "adapter.currentId:${adapter.currentId}")

                        // 在 TAB 模式下，切换到当前选中的 app
                        if (useCase.pageMode == PageMode.TAB && currentTabId != null) {
                            switchToTab(currentTabId)
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
                        switchToTab(adapter.currentId!!)
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

        // 处理首次启动时携带的 deeplink
        handleDeepLink(intent)
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
        // 同步当前配置到 RN AppearanceModule，确保深色模式切换后被感知
        application.reactNativeHost.reactInstanceManager.onConfigurationChanged(this, resources.configuration)
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        val application = application as MainApplication
        application.reactNativeHost.reactInstanceManager.onConfigurationChanged(this, newConfig)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        handleDeepLink(intent)
    }

    /**
     * 解析 deeplink 并将数据发送给 React Native
     * 链接格式示例:
     *   actro://chat/123?token=abc
     *   https://app.actrium.example/chat/123?token=abc
     */
    private fun handleDeepLink(intent: Intent?) {
        val uri = intent?.data ?: return
        val appId = uri.getQueryParameter("appid")
        Log.d("TabletActivity", "DeepLink received: $uri, appid=$appId")

        if (appId != null) {
            viewModel.isFromDeepLink = true
            viewModel.addApp(appId = appId)
        }
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

        // name 为空时不显示收藏行
        val showFav = activeModel.name.isNotEmpty()
        llFav.visibility = if (showFav) View.VISIBLE else View.GONE

        // 只有当前 item 时不显示"关闭其他页签"
        val appList = viewModel.tabletUseCase.value?.appList ?: emptyList()
        val hasOtherItems = appList.size > 1
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
            viewModel.toggleMarkStatus(activeModel)
        }
        popupView.findViewById<View>(R.id.llClose).setOnClickListener {
            popup.dismiss()
            deleteTab(activeModel.id)
        }
        llCloseOther.setOnClickListener {
            popup.dismiss()
            closeOtherTabs(activeModel.id)
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
     * 删除指定 tab（卸载 ReactRootView + 移除 ViewModel 数据 + 空列表切换 Grid）
     */
    private fun deleteTab(tabId: String) {
        val reactRootView = reactViewMap.remove(tabId)
        reactRootView?.let { rootView ->
            try {
                rootView.unmountReactApplication()
            } catch (e: Exception) {
                Log.e("TabletActivity", "Error unmounting React application", e)
            }
        }
        viewModel.removeActiveModel(tabId)
        if (reactViewMap.isEmpty()) {
            viewModel.changeDisplayMode(PageMode.GRID)
        }
    }

    /**
     * 关闭除指定 tab 以外的所有 tab
     */
    private fun closeOtherTabs(keepId: String) {
        val appList = viewModel.tabletUseCase.value?.appList ?: emptyList()
        val others = appList.filter { it.id != keepId }
        for (model in others) {
            deleteTab(model.id)
        }
    }

    private fun updateAddButtonPosition() {
        val layoutManager = binding.rvApp.layoutManager as? LinearLayoutManager ?: return
        val dataCount = adapter.dataItemCount

        if (dataCount == 0) {
            // 如果没有数据 item，将 ivAdd 放在起始位置
            binding.ivAdd.translationX = 0f
            return
        }

        // 最后一个真实数据 item 的 adapter position = dataCount（前面有 left pad）
        val lastDataPosition = dataCount
        // 优先找最后一个真实数据 item，若不可见则回退到最后可见的 item
        val targetPosition = if (layoutManager.findViewByPosition(lastDataPosition) != null) {
            lastDataPosition
        } else {
            layoutManager.findLastVisibleItemPosition()
        }
        val lastItemView = layoutManager.findViewByPosition(targetPosition)

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
            // TAB 模式
            val sourceView = pendingGridSourceView
            val activeModel = pendingGridActiveModel
            pendingGridSourceView = null
            pendingGridActiveModel = null

            if (sourceView != null && sourceView.isAttachedToWindow) {
                // Grid → Tab 动画
                // 1. 从截图文件加载全分辨率 bitmap，避免从小尺寸 view 截图导致放大变虚
                val bitmap = if (activeModel?.screenshot?.isNotEmpty() == true) {
                    decodeScreenshotFile(activeModel.screenshot)
                } else {
                    captureViewBitmap(sourceView)
                }
                if (bitmap != null) {
                    binding.ivTransition.setImageBitmap(bitmap)

                    // 2. 隐藏 rvGridContent，显示标题栏
                    binding.rvGridContent.visibility = View.GONE
                    binding.rlTitleGrid.visibility = View.GONE
                    binding.llSearch.visibility = View.GONE
                    binding.llTitleTab.visibility = View.VISIBLE

                    // 3. flTabContent 先设置为 INVISIBLE（需要测量尺寸但不显示）
                    binding.flTabContent.visibility = View.INVISIBLE
                    updateFullScreenState()

                    // 4. ivTransition 开始动画（从 Grid item 位置放大到全屏）
                    binding.ivTransition.visibility = View.VISIBLE
                    binding.ivTransition.post {
                        ViewAnimationHelper.animateShow(binding.ivTransition, sourceView) {
                            // 重试时重新调用
                            pendingGridSourceView = sourceView
                            changeMode(PageMode.TAB)
                        }
                        // animateShow 的 onAnimationEnd 需要在 flTabContent 切换后执行
                        // 但 animateShow 没有 onAnimationEnd 回调，我们在动画后通过 postDelayed 来处理
                        binding.ivTransition.postDelayed({
                            binding.ivTransition.visibility = View.GONE
                            binding.flTabContent.visibility = View.VISIBLE
                            updateFullScreenState()
                        }, 260)
                    }
                } else {
                    // 截图失败，直接切换
                    binding.llTitleTab.visibility = View.VISIBLE
                    binding.flTabContent.visibility = View.VISIBLE
                    binding.rlTitleGrid.visibility = View.GONE
                    binding.llSearch.visibility = View.GONE
                    binding.rvGridContent.visibility = View.GONE
                    updateFullScreenState()
                }
            } else {
                // 无 sourceView，直接切换
                binding.llTitleTab.visibility = View.VISIBLE
                binding.flTabContent.visibility = View.VISIBLE
                binding.rlTitleGrid.visibility = View.GONE
                binding.llSearch.visibility = View.GONE
                binding.rvGridContent.visibility = View.GONE
                updateFullScreenState()
            }
        } else {
            // GRID 模式 - Tab → Grid 动画

            // 1. 截取当前 Tab 的 ReactRootView 截图设置到 ivTransition 上
            val currentTabId = adapter.currentId
            val currentReactView = if (currentTabId != null) reactViewMap[currentTabId] else null
            val bitmap = if (currentReactView != null) captureViewBitmap(currentReactView) else null

            // 2. 先显示标题栏
            binding.rlTitleGrid.visibility = View.VISIBLE
            binding.llSearch.visibility = View.VISIBLE
            binding.llTitleTab.visibility = View.GONE

            // 3. 刷新 grid adapter 以应用截图
            gridAdapter.notifyDataSetChanged()

            if (bitmap != null) {
                binding.ivTransition.setImageBitmap(bitmap)
                binding.ivTransition.visibility = View.VISIBLE

                // 4. 隐藏 flTabContent
                binding.flTabContent.visibility = View.GONE
                updateFullScreenState()

                // 5. 先让 rvGridContent 布局完成
                binding.rvGridContent.visibility = View.VISIBLE
                binding.rvGridContent.post {
                    // 查找 Grid 中对应 item 的 View
                    val targetItemView = findGridItemView(currentTabId)
                    if (targetItemView != null) {
                        // 使用 animateHide 将 ivTransition 从全屏缩小到 Grid item 的位置
                        ViewAnimationHelper.animateHide(binding.ivTransition, targetItemView) {
                            binding.ivTransition.visibility = View.GONE
                        }
                    } else {
                        // 找不到目标 item，直接隐藏
                        binding.ivTransition.visibility = View.GONE
                    }
                }
            } else {
                // 截图失败，直接切换
                binding.flTabContent.visibility = View.GONE
                binding.rvGridContent.visibility = View.VISIBLE
                binding.ivTransition.visibility = View.GONE
                updateFullScreenState()
            }
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
            Log.e("TabletActivity", "createReactRootView:${launchOptions}")
            // 启动 React 应用
            startReactApplication(reactInstanceManager, componentName, launchOptions)
        }
    }

    /**
     * 对所有 ReactRootView 截图，并将本地存储路径存储到对应 ActiveModel 的 screenshot 字段。
     */
    private fun captureScreenshots() {
        val appList = viewModel.tabletUseCase.value?.appList ?: return

        // 在 UI 线程逐个截取 Bitmap（draw 必须在 UI 线程），
        // 截完一张立刻提交到 IO 线程压缩保存，降低内存峰值
        val saveJobs = mutableListOf<kotlinx.coroutines.Deferred<Unit>>()
        for (item in appList) {
            val reactRootView = reactViewMap[item.id]
            if (reactRootView != null && reactRootView.width > 0 && reactRootView.height > 0) {
                try {
                    val bitmap = captureViewBitmap(reactRootView, scaleFactor = 0.5f) ?: continue
                    saveJobs.add(lifecycleScope.async<Unit>(Dispatchers.IO) {
                        try {
                            val file = File(cacheDir, "${item.id}.jpg")
                            FileOutputStream(file).use { out ->
                                bitmap.compress(Bitmap.CompressFormat.JPEG, 70, out)
                            }
                            item.screenshot = file.absolutePath
                        } catch (e: Exception) {
                            Log.e("TabletActivity", "Failed to save screenshot for ${item.id}", e)
                        } finally {
                            bitmap.recycle()
                        }
                    })
                } catch (e: Exception) {
                    Log.e("TabletActivity", "Failed to capture screenshot for ${item.id}", e)
                }
            }
        }

        // 等待所有保存完成（不阻塞 UI 线程）
        lifecycleScope.launch {
            saveJobs.awaitAll()
        }
    }

    /**
     * 从截图文件加载全分辨率 Bitmap，用于 Grid → Tab 放大动画
     */
    private fun decodeScreenshotFile(path: String): Bitmap? {
        return try {
            android.graphics.BitmapFactory.decodeFile(path)
        } catch (e: Exception) {
            Log.e("TabletActivity", "Failed to decode screenshot file", e)
            null
        }
    }

    /**
     * 截取 View 的 Bitmap
     * @param scaleFactor 缩放因子，1f 为原始分辨率，0.5f 为宽高各缩小一半
     */
    private fun captureViewBitmap(view: View, scaleFactor: Float = 1f): Bitmap? {
        if (view.width <= 0 || view.height <= 0) return null
        return try {
            val w = (view.width * scaleFactor).toInt().coerceAtLeast(1)
            val h = (view.height * scaleFactor).toInt().coerceAtLeast(1)
            val bitmap = Bitmap.createBitmap(w, h, Bitmap.Config.RGB_565)
            bitmap.density = resources.displayMetrics.densityDpi
            val canvas = android.graphics.Canvas(bitmap)
            if (scaleFactor != 1f) {
                canvas.scale(scaleFactor, scaleFactor)
            }
            view.draw(canvas)
            bitmap
        } catch (e: Exception) {
            Log.e("TabletActivity", "Failed to capture view bitmap", e)
            null
        }
    }

    /**
     * 在 rvGridContent 中查找对应 tabId 的 GridItemViewHolder 的 ivScreenshot
     */
    private fun findGridItemView(tabId: String?): View? {
        if (tabId == null) return null
        val appList = viewModel.tabletUseCase.value?.appList ?: return null
        val position = appList.indexOfFirst { it.id == tabId }
        if (position < 0) return null

        val viewHolder = binding.rvGridContent.findViewHolderForAdapterPosition(position)
        if (viewHolder is GridContentAdapter.GridItemViewHolder) {
            return viewHolder.ivScreenshot
        }
        return null
    }

    /**
     * 同步 flTabContent 中的子 View，确保与 appList 一致
     */
    private fun syncTabContentViews(appList: List<io.actrium.actro.model.ActiveModel>) {
        val flTabContent = binding.flTabContent

        // 更新 tabIdList
        tabIdList.clear()
        for (item in appList) {
            tabIdList.add(item.id)
        }

        // 移除不再存在的 View
        val viewsToRemove = mutableListOf<View>()
        for (i in 0 until flTabContent.childCount) {
            val child = flTabContent.getChildAt(i)
            val tabId = child.tag as? String
            if (tabId != null && !tabIdList.contains(tabId)) {
                viewsToRemove.add(child)
            }
        }
        viewsToRemove.forEach { flTabContent.removeView(it) }

        // 添加新的 View，确保顺序一致
        for ((index, item) in appList.withIndex()) {
            val reactRootView = reactViewMap[item.id] ?: continue
            val existingView = flTabContent.findViewWithTag<View>(item.id)

            if (existingView == null) {
                // 如果已经有 parent，先移除
                (reactRootView.parent as? ViewGroup)?.removeView(reactRootView)
                reactRootView.tag = item.id
                reactRootView.visibility = View.INVISIBLE
                flTabContent.addView(
                    reactRootView, index, FrameLayout.LayoutParams(
                        FrameLayout.LayoutParams.MATCH_PARENT,
                        FrameLayout.LayoutParams.MATCH_PARENT
                    )
                )
            } else {
                // 确保 View 在正确的位置
                val currentIndex = flTabContent.indexOfChild(existingView)
                if (currentIndex != index) {
                    flTabContent.removeView(existingView)
                    flTabContent.addView(existingView, index)
                }
            }
        }
    }

    /**
     * 切换到指定 tabId 的 Tab，显示对应的 ReactRootView
     */
    private fun switchToTab(tabId: String) {
        val flTabContent = binding.flTabContent
        for (i in 0 until flTabContent.childCount) {
            val child = flTabContent.getChildAt(i)
            child.visibility = if (child.tag == tabId) View.VISIBLE else View.INVISIBLE
        }
        updateFullScreenState()
    }

    /**
     * 根据当前 flTabContent 及其子 View 的可见性，更新全屏状态。
     * 当某个 tab 的 ReactRootView 在 flTabContent 中可见且 flTabContent 本身可见时，
     * 调用 enterFullScreen；当 tab 不再可见时，调用 exitFullScreen。
     */
    private fun updateFullScreenState() {
        val flTabContent = binding.flTabContent
        val isTabContentVisible = flTabContent.visibility == View.VISIBLE

        // 查找当前在 flTabContent 中可见的 tab
        var visibleTabId: String? = null
        if (isTabContentVisible) {
            for (i in 0 until flTabContent.childCount) {
                val child = flTabContent.getChildAt(i)
                if (child.visibility == View.VISIBLE) {
                    visibleTabId = child.tag as? String
                    break
                }
            }
        }

        // 状态发生变化时才调用，避免重复发送事件
        if (visibleTabId != currentFullScreenTabId) {
            currentFullScreenTabId?.let { viewModel.exitFullScreen(it) }
            visibleTabId?.let { viewModel.enterFullScreen(it) }
            currentFullScreenTabId = visibleTabId
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
        val targetView = createReactRootView(ReactNativeBundle.REACT_COMPONENT_NAME, bundle)
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
        val targetView = createReactRootView(ReactNativeBundle.REACT_COMPONENT_NAME, bundle)
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
        val targetView = createReactRootView(ReactNativeBundle.REACT_COMPONENT_NAME, bundle)
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
