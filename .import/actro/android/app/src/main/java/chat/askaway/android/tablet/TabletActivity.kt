package chat.askaway.android.tablet

import android.content.Intent
import android.os.Bundle
import android.util.Log
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
import com.askaway.android.databinding.ActivityTabletBinding
import com.facebook.react.ReactRootView
import com.facebook.react.modules.core.DefaultHardwareBackBtnHandler
import com.facebook.react.modules.core.PermissionAwareActivity
import com.facebook.react.modules.core.PermissionListener
import com.facebook.react.uimanager.PixelUtil.pxToDp

class TabletActivity : AppCompatActivity(), DefaultHardwareBackBtnHandler, PermissionAwareActivity {

    private lateinit var binding: ActivityTabletBinding
    private lateinit var adapter: ActiveAdapter
    private var permissionListener: PermissionListener? = null
    private lateinit var gridAppAdapter: GridAppAdapter
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
        adapter.onDeleteClick = {
            for (i in binding.flRNContent.childCount - 1 downTo 0) {
                val child = binding.flRNContent.getChildAt(i)
                if (child.tag == it.id) {
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
            viewModel.removeActiveModel(it.id)
        }
        gridAppAdapter = GridAppAdapter()
        binding.gvApp.adapter = gridAppAdapter
        binding.gvApp.numColumns = 2

        // 监听 RecyclerView 的布局变化，更新 ivAdd 位置
        binding.rvApp.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ ->
            updateAddButtonPosition()
        }

        binding.flRNContent.post {
            viewModel.containerWidth = binding.flRNContent.width.pxToDp().toInt()
            viewModel.containerHeight = binding.flRNContent.height.pxToDp().toInt()
            gridWidth = binding.gvApp.width
            gridHeight = binding.gvApp.height
            gridAppAdapter.dimensionRatio = viewModel.containerWidth * 1.0f / viewModel.containerHeight
        }

        // 监听 RecyclerView 的滚动，更新 ivAdd 位置
        binding.rvApp.addOnScrollListener(object : RecyclerView.OnScrollListener() {
            override fun onScrolled(recyclerView: RecyclerView, dx: Int, dy: Int) {
                updateAddButtonPosition()
            }
        })
        binding.ivHome.setOnClickListener { viewModel.changeDisplayMode(PageMode.GRID) }
        binding.tvBack.setOnClickListener { viewModel.changeDisplayMode(PageMode.TAB) }

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
                            targetView.tag = currentTabId
                            binding.flRNContent.addView(targetView)
                        }
                    }

                    if (useCase.pageMode != viewModel.currentDisplayModel) {
                        changeMode(useCase.pageMode)
                        viewModel.currentDisplayModel = useCase.pageMode
                    }
                    Log.e("TabletActivity", "useCase.appList.size:${useCase.appList.size}")
                    val column = GridViewColumnCalculator.calculateNumColumns(
                        this@TabletActivity,
                        gridWidth,
                        gridHeight,
                        useCase.appList.size + 1,
                        viewModel.containerWidth * 1.0f / viewModel.containerHeight
                    )
                    binding.gvApp.numColumns = column
                    gridAppAdapter.updateAppList(useCase.appList, emptyList())
                }
            }
        }
        binding.ivAdd.setOnClickListener {
            viewModel.addApp()
        }
        binding.ivAdd.postDelayed(300L) {
            binding.ivAdd.performClick()
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
            binding.flGrid.isVisible = false
//            autoFitGridLayoutManager.updateItemAspectRatio(viewModel.containerWidth * 1.0f / viewModel.containerHeight)
        } else {
            binding.rlTitleGrid.isVisible = true
            binding.flGrid.isVisible = true
            binding.llTitleTab.isVisible = false
            binding.flRNContent.isVisible = false
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
}