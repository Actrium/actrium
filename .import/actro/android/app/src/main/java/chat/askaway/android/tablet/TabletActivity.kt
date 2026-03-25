package chat.askaway.android.tablet

import android.content.Intent
import android.os.Bundle
import android.util.Log
import android.view.LayoutInflater
import android.view.View
import android.widget.FrameLayout
import android.view.animation.AccelerateDecelerateInterpolator
import androidx.appcompat.app.AppCompatActivity
import androidx.core.content.ContextCompat
import androidx.core.view.isVisible
import androidx.core.view.postDelayed
import androidx.lifecycle.lifecycleScope
import androidx.recyclerview.widget.LinearLayoutManager
import androidx.recyclerview.widget.RecyclerView
import chat.askaway.android.utils.GridViewColumnCalculator
import com.askaway.android.BuildConfig
import com.askaway.android.MainApplication
import com.askaway.android.R
import com.askaway.android.databinding.ActivityTabletBinding
import com.facebook.react.ReactRootView
import com.facebook.react.modules.core.DefaultHardwareBackBtnHandler
import com.facebook.react.modules.core.PermissionAwareActivity
import com.facebook.react.modules.core.PermissionListener
import com.facebook.react.uimanager.PixelUtil.pxToDp
import com.askaway.android.core.ReactNativeBundle
import android.content.Context
import android.view.inputmethod.InputMethodManager

class TabletActivity : AppCompatActivity(), DefaultHardwareBackBtnHandler, PermissionAwareActivity {

    private lateinit var binding: ActivityTabletBinding
    private lateinit var adapter: ActiveAdapter
    private var permissionListener: PermissionListener? = null
    private lateinit var viewModel: TabletViewModel
    private var gridWidth = 0
    private var gridHeight = 0

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityTabletBinding.inflate(layoutInflater)
        setContentView(binding.root)
        viewModel = TabletViewModel()
        adapter = ActiveAdapter()
        binding.rvApp.layoutManager = LinearLayoutManager(this, LinearLayoutManager.HORIZONTAL, false)
        binding.rvApp.adapter = adapter
        adapter.onItemClick = {
            viewModel.changeActiveModel(it.id)
        }
        adapter.onDeleteClick = {
            for (i in binding.flRNContent.childCount - 1 downTo 0) {
                val child = binding.flRNContent.getChildAt(i)
                if (child.tag == it.id) {
                    binding.flRNContent.removeViewAt(i)
                    // 必须手动卸载 React 应用以释放 JS 资源并停止执行
                    // 如果是 ScalableContainer，需要获取内部的 ReactRootView
                    val reactRootView = if (child is ScalableContainer && child.childCount > 0) {
                        child.getChildAt(0) as? ReactRootView
                    } else if (child is ReactRootView) {
                        child
                    } else {
                        null
                    }

                    reactRootView?.let { rootView ->
                        try {
                            rootView.unmountReactApplication()
                        } catch (e: Exception) {
                            Log.e("MainActivity", "Error unmounting React application", e)
                        }
                    }
                    break
                }
            }
            // 移除当前 item
            viewModel.removeActiveModel(it.id)
        }

        // 监听 RecyclerView 的布局变化，更新 ivAdd 位置
        binding.rvApp.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ ->
            updateAddButtonPosition()
        }

        binding.flRNContent.post {
            viewModel.containerWidth = binding.flRNContent.width.pxToDp().toInt()
            viewModel.containerHeight = binding.flRNContent.height.pxToDp().toInt()
            binding.flRNContent.itemAspectRatio = viewModel.containerWidth * 1.0f / viewModel.containerHeight
        }

        // 设置 TabGridContainer 的点击事件
        binding.flRNContent.onItemClick = { position ->
            if (binding.flRNContent.isGridMode) {
                // 点击网格中的 item，切换到 TAB 模式并显示该 item
                binding.flRNContent.currentPosition = position
                viewModel.changeDisplayMode(PageMode.TAB)
                // 同时更新 ViewModel 中的当前 app
                val appList = viewModel.tabletUseCase.value?.appList
                if (appList != null && position < appList.size) {
                    // 这里需要根据你的业务逻辑来更新当前选中的 app
                }
            }
        }

        // 设置网格模式下的添加按钮点击事件
        binding.flRNContent.onAddClick = {
            viewModel.addApp()
        }

        // 监听 RecyclerView 的滚动，更新 ivAdd 位置
        binding.rvApp.addOnScrollListener(object : RecyclerView.OnScrollListener() {
            override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
                updateAddButtonPosition()
            }
        })
        binding.ivHome.setOnClickListener { viewModel.changeDisplayMode(PageMode.GRID) }
        binding.tvBack.setOnClickListener {
            viewModel.hideBookmark()
            viewModel.changeDisplayMode(PageMode.TAB)
        }

        // 监听 StateFlow 并更新 Adapter
        lifecycleScope.launchWhenStarted {
            viewModel.tabletUseCase.collect { useCase ->
                if (useCase != null) {
                    adapter.updateData(useCase.appList)
                    // 数据更新后，延迟更新位置以确保布局完成
                    binding.rvApp.post {
                        updateAddButtonPosition()
                    }
                    useCase.currentApp?.let {
                        val currentTabId = it.getString("currentTabId")
                        adapter.currentId = currentTabId
                        Log.e("TabletActivity", "adapter.currentId:${adapter.currentId}")
                        var found = false
                        var foundIndex = -1
                        for (i in 0 until binding.flRNContent.childCount) {
                            val child = binding.flRNContent.getChildAt(i)
                            if (child.tag == currentTabId) {
                                found = true
                                foundIndex = i
                                break
                            }
                        }
                        if (!found) {
                            val targetView = createReactRootView("canvas-kotlin", it)

                            // 创建 ScalableContainer 包装 targetView
                            val scalableContainer = ScalableContainer(this@TabletActivity)
                            scalableContainer.addView(
                                targetView, FrameLayout.LayoutParams(
                                    FrameLayout.LayoutParams.MATCH_PARENT,
                                    FrameLayout.LayoutParams.MATCH_PARENT
                                )
                            )

                            // 创建 tablet_item_app 布局
                            val itemAppView = layoutInflater.inflate(R.layout.tablet_item_app, binding.flRNContent, false)
                            itemAppView.tag = currentTabId

                            // 将 scalableContainer 添加到 flAppContent 中
                            val flAppContent = itemAppView.findViewById<FrameLayout>(R.id.flAppContent)
                            flAppContent.addView(
                                scalableContainer, FrameLayout.LayoutParams(
                                    FrameLayout.LayoutParams.MATCH_PARENT,
                                    FrameLayout.LayoutParams.MATCH_PARENT
                                )
                            )

                            // 将 itemAppView 添加到 flRNContent
                            binding.flRNContent.addView(itemAppView)
                            foundIndex = binding.flRNContent.childCount - 1
                        }
                        // 更新当前显示的位置
                        if (foundIndex >= 0) {
                            binding.flRNContent.currentPosition = foundIndex
                        }
                    }

                    if (useCase.pageMode != viewModel.currentDisplayModel) {
                        changeMode(useCase.pageMode)
                        viewModel.currentDisplayModel = useCase.pageMode
                    }
                    if (useCase.showPageType == ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_SETTINGS) {
                        showSettingsPopup()
                    } else {
                        hideSettingsPopup()
                    }
                    if (useCase.showPageType == ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_SEARCH) {
                        showSearch()
                    } else {
                        hideSearch()
                    }
                    if (useCase.showPageType == ReactNativeBundle.REACT_NATIVE_VIEW_TYPE_BOOKMARK) {
                        showBookmark()
                    } else {
                        hideBookmark()
                    }

                    Log.e("TabletActivity", "useCase.appList.size:${useCase.appList.size}")
                    val column = GridViewColumnCalculator.calculateNumColumns(
                        this@TabletActivity,
                        gridWidth,
                        gridHeight,
                        useCase.appList.size + 1,
                        viewModel.containerWidth * 1.0f / viewModel.containerHeight
                    )
                }
            }
        }
        binding.ivAdd.setOnClickListener {
            viewModel.addApp()
        }
        binding.ivAdd.postDelayed(300L) {
            binding.ivAdd.performClick()
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
                viewModel.showBookmark()
            } else {
                viewModel.hideBookmark()
            }
        }
//        binding.flRNContent.adjustKeyboard()
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
        if (mode == PageMode.TAB) {
            binding.llTitleTab.isVisible = true
            binding.flRNContent.isVisible = true
            binding.rlTitleGrid.isVisible = false
            // 切换到 TAB 模式
            binding.flRNContent.isGridMode = false
            binding.llSearch.isVisible = false
        } else {
            binding.rlTitleGrid.isVisible = true
            binding.llSearch.isVisible = true  // 隐藏旧的 GridView
            binding.llTitleTab.isVisible = false
            binding.flRNContent.isVisible = true  // 显示 flRNContent
            // 切换到 GRID 模式
            binding.flRNContent.isGridMode = true
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
        val bundle = sourceBundle ?: ReactNativeBundle.getSetBundle()
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
        val bundle = sourceBundle ?: ReactNativeBundle.getSearchBundle()
        val targetView = createReactRootView("canvas-kotlin", bundle)
        binding.flSearch.addView(targetView)
    }

    private fun hideSearch() {
        binding.flSearch.isVisible = false
        hideKeyboard()
    }

    private fun showBookmark(sourceBundle: Bundle? = null) {
        if (binding.flBookmark.isVisible) return
        binding.flBookmark.isVisible = true
        binding.flBookmark.removeAllViews()
        val bundle = sourceBundle ?: ReactNativeBundle.getBookmarkBundle()
        val targetView = createReactRootView("canvas-kotlin", bundle)
        binding.flBookmark.addView(targetView)
    }

    private fun hideBookmark() {
        binding.flBookmark.isVisible = false
    }

    private fun hideKeyboard() {
        val imm = getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        val view = currentFocus ?: window.decorView
        imm.hideSoftInputFromWindow(view.windowToken, 0)
    }
}