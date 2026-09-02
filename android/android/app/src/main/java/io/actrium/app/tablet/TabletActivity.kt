package io.actrium.app.tablet

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
import io.actrium.app.utils.TabBundleUtils
import io.actrium.app.model.ActiveModel
import io.actrium.app.BuildConfig
import io.actrium.app.MainApplication
import io.actrium.app.R
import io.actrium.app.core.ReactNativeBundle
import io.actrium.app.databinding.ActivityTabletBinding
import io.actrium.app.utils.RuntimeBridgeParams
import io.actrium.app.utils.ViewAnimationHelper
import io.actrium.app.utils.adjustKeyboard
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

    // Records tab Order of list (id list) for the pass-through index Find the match. View
    private val tabIdList = mutableListOf<String>()

    // Grid → Tab It's animated during animation. sourceView
    private var pendingGridSourceView: View? = null
    private var pendingGridActiveModel: ActiveModel? = null

    // Tracks current full screen status tabId, avoid repeating calls enterFullScreen / exitFullScreen
    private var currentFullScreenTabId: String? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityTabletBinding.inflate(layoutInflater)
        setContentView(binding.root)
        viewModel = TabletViewModel(applicationContext)
        // List of applications for initializing top adapter
        adapter = ActiveAdapter()
        binding.rvApp.layoutManager = LinearLayoutManager(this, LinearLayoutManager.HORIZONTAL, false)
        binding.rvApp.adapter = adapter
        // Remove flex (%1)overscroll) Effects, avoid the backlash as you scroll to the border
        binding.rvApp.overScrollMode = View.OVER_SCROLL_NEVER

        // Scrolls when selected items change RecyclerView Make it completely visible.
        // Use Custom SmoothScrollerLet the target go. item Align to Left Edge 18dp Photo by Flickr user @un.org
        // Make sure you're the previous one. item The reverse angle is always visible.
        adapter.onSelectedChanged = { position ->
            binding.rvApp.post {
                val layoutManager = binding.rvApp.layoutManager as? LinearLayoutManager ?: return@post
                val adapterPosition = position + 1
                val offsetPx = (18 * resources.displayMetrics.density).toInt()
                // To judge the target. item Is it fully visible:
                // 1. Left margin visible range >= offsetPx(Reverse angle fully visible)
                // 2. Right edge not beyond visible area (item Not Right-Screwed)
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

        // Initialize Content Area adapter
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

            // Update container size
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

                // Update GridLayoutManager It's... aspectRatio
                val newAspectRatio = newWidth.toFloat() / newHeight.toFloat()
                gridLayoutManager.updateAspectRatio(newAspectRatio)

                // Update GridItemDecoration & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & & optionHeight
                gridItemDecoration?.let {
                    binding.rvGridContent.removeItemDecoration(it)
                }
                val optionHeightPxUpdate = (32 * resources.displayMetrics.density).toInt()
                gridItemDecoration = GridItemDecoration(gridSpanCount, gridSpacingPx, newAspectRatio, optionHeightPxUpdate)
                binding.rvGridContent.addItemDecoration(gridItemDecoration!!)

                // Refresh grid adapter Layout
                gridAdapter.notifyDataSetChanged()

                // Update rvGridContent It's... margin
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

        // Get the container size
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

        // Set the initial mode to TAB
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

        // Grid adapter Click Event
        gridAdapter.onItemClick = { position, sourceView ->
            // Save sourceView and ActiveModel Animation supply
            pendingGridSourceView = sourceView
            val appList = viewModel.tabletUseCase.value?.appList
            if (appList != null && position < appList.size) {
                pendingGridActiveModel = appList[position]
            }
            // Click in Grid Mode, switch to TAB Mode
            viewModel.changeDisplayMode(PageMode.TAB)
            // Updates the currently selected app
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

        // Listening RecyclerView , update ivAdd Location
        binding.rvApp.addOnLayoutChangeListener { _, _, _, _, _, _, _, _, _ ->
            updateAddButtonPosition()
        }

        // Listening RecyclerView , update ivAdd Location
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

        // Listening StateFlow , and update Adapter
        lifecycleScope.launchWhenStarted {
            viewModel.tabletUseCase.collect { useCase ->
                if (useCase != null) {
                    // For Current app Create ReactRootView(if not yet)
                    useCase.currentApp?.let { bundle ->
                        Log.e("TabletActivity", "bundle:${bundle}")
                        val currentTabId =
                            bundle.getString(RuntimeBridgeParams.rootCurrentTabIdKey)
                        if (currentTabId != null) {
                            if (reactViewMap[currentTabId] == null) {
                                // Create new ReactRootView
                                val targetView = createReactRootView(ReactNativeBundle.REACT_COMPONENT_NAME, bundle)
                                reactViewMap[currentTabId] = targetView
                                Log.d("TabletActivity", "Created new ReactRootView for $currentTabId ${bundle}")
                            }
                        }
                    }

                    for (item in useCase.appList) {
                        if (reactViewMap[item.id] == null) {
                            // Create new ReactRootView
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

                    // Update list of top applications
                    adapter.updateData(useCase.appList)
                    binding.rvApp.post {
                        updateAddButtonPosition()
                    }

                    // Update Content Area
                    gridAdapter.updateItems(useCase.appList)

                    // Sync flTabContent Middle View
                    syncTabContentViews(useCase.appList)

                    // Updates the currently selected app
                    useCase.currentApp?.let {
                        val currentTabId =
                            it.getString(RuntimeBridgeParams.rootCurrentTabIdKey)
                        adapter.currentId = currentTabId
                        Log.e("TabletActivity", "adapter.currentId:${adapter.currentId}")

                        // Yes. TAB Mode, switch to the currently selected app
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
                    // Switch Mode
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
        // Set button click event
        binding.ivSet.setOnClickListener {
            viewModel.showSet()
        }
        binding.llSearch.setOnClickListener {
            Log.e("TabletActivity", "llSearch click")
            viewModel.showSearch()
        }
        binding.navTag.setOnCheckedChangeListener { _, isChecked ->
            if (isChecked) {
                // Settings navTag Background is: bottom_nav_indicator_dark
                binding.navTag.background = ContextCompat.getDrawable(this, R.drawable.bottom_nav_indicator_dark)
                // Settings navActivity Background is empty
                binding.navActivity.background = null
                viewModel.showBookmark()
            } else {
                // Settings navTag Background is empty
                binding.navTag.background = null
                // Settings navActivity Background is: bottom_nav_indicator_dark
                binding.navActivity.background = ContextCompat.getDrawable(this, R.drawable.bottom_nav_indicator_dark)
                viewModel.hideBookmark()
            }
        }
//        binding.flRNContent.adjustKeyboard()

        // Handled with first startup deeplink
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
        // Sync Current Configuration to RN AppearanceModuleEnsure that dark-colour switching is detected
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
     * Parsing deeplink And send the data to React Native
     * Example of Link Format:
     *   actr://example.com/chat/123?token=abc
     *   https://app.actrium.chat/chat/123?token=abc
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
     * Long item Popup Operations Menu
     * Positioning with contact centre, maintaining the screen boundary 20px Spacing
     */
    private fun showLongPressPopup(activeModel: ActiveModel, rawX: Int, rawY: Int) {
        longPressPopup?.dismiss()

        val popupView = LayoutInflater.from(this).inflate(R.layout.popup_long_press, null)
        val llFav = popupView.findViewById<View>(R.id.llFav)
        val ivFav = popupView.findViewById<ImageView>(R.id.ivFav)
        val tvFav = popupView.findViewById<android.widget.TextView>(R.id.tvFav)
        val llCloseOther = popupView.findViewById<View>(R.id.llCloseOther)
        val vDivider = popupView.findViewById<View>(R.id.vDivider)

        // name Do not show collection lines for empty hours
        val showFav = activeModel.name.isNotEmpty()
        llFav.visibility = if (showFav) View.VISIBLE else View.GONE

        // Current only item Do not show time"Close Other Pages"
        val appList = viewModel.tabletUseCase.value?.appList ?: emptyList()
        val hasOtherItems = appList.size > 1
        llCloseOther.visibility = if (hasOtherItems) View.VISIBLE else View.GONE
        vDivider.visibility = if (hasOtherItems) View.VISIBLE else View.GONE

        // Switch icons and text from collection status
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

        // Measuring the size of the bullet window
        popupView.measure(View.MeasureSpec.UNSPECIFIED, View.MeasureSpec.UNSPECIFIED)
        val popupWidth = popupView.measuredWidth
        val popupHeight = popupView.measuredHeight

        val margin = 20
        val screenWidth = resources.displayMetrics.widthPixels
        val screenHeight = resources.displayMetrics.heightPixels

        // Calculate position with contact centre, then bind to the boundary
        val x = (rawX - popupWidth / 2)
            .coerceAtLeast(margin)
            .coerceAtMost(screenWidth - popupWidth - margin)

        val y = (rawY - popupHeight / 2)
            .coerceAtLeast(margin)
            .coerceAtMost(screenHeight - popupHeight - margin)

        popup.showAtLocation(window.decorView, android.view.Gravity.NO_GRAVITY, x, y)
    }

    /**
     * Delete Assign tab(unmounting) ReactRootView + Remove ViewModel Data + Empty List Switch Grid)
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
     * Close Discard Designation tab Everything else tab
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
            // If No Data item♪ Will ivAdd Place at start
            binding.ivAdd.translationX = 0f
            return
        }

        // Last Real Data item It's... adapter position = dataCount"Ahead of the left pad)
        val lastDataPosition = dataCount
        // First priority is finding the last real data. itemIf you can't see, then you can turn back to the last visible. item
        val targetPosition = if (layoutManager.findViewByPosition(lastDataPosition) != null) {
            lastDataPosition
        } else {
            layoutManager.findLastVisibleItemPosition()
        }
        val lastItemView = layoutManager.findViewByPosition(targetPosition)

        if (lastItemView != null) {
            // Calculating Last item Right edge position
            val lastItemRight = lastItemView.right.toFloat()

            // Retrieving the width of the parent container
            val parentWidth = binding.rvApp.parent?.let { (it as android.view.View).width } ?: 0

            // Calculate ivAdd Place to be (last) item Right)
            val targetX = lastItemRight

            // Make sure you don't go beyond the boundaries of the father's container.
            val maxX = parentWidth - binding.ivAdd.width
            val finalX = targetX.coerceAtMost(maxX.toFloat())

            binding.ivAdd.translationX = finalX
        }
    }

    private fun changeMode(mode: PageMode) {
        // Switch to Grid Before Mode, All item It's... ReactRootView Screenshot and Store to screenshot Fields
        if (mode == PageMode.GRID) {
            captureScreenshots()
        }

        if (mode == PageMode.TAB) {
            // TAB Mode
            val sourceView = pendingGridSourceView
            val activeModel = pendingGridActiveModel
            pendingGridSourceView = null
            pendingGridActiveModel = null

            if (sourceView != null && sourceView.isAttachedToWindow) {
                // Grid → Tab Animation
                // 1. Load full resolution from the screenshot file bitmapavoids small sizes view The screenshots are causing the magnification to become weaker.
                val bitmap = if (activeModel?.screenshot?.isNotEmpty() == true) {
                    decodeScreenshotFile(activeModel.screenshot)
                } else {
                    captureViewBitmap(sourceView)
                }
                if (bitmap != null) {
                    binding.ivTransition.setImageBitmap(bitmap)

                    // 2. Hide rvGridContent, the title bar is displayed
                    binding.rvGridContent.visibility = View.GONE
                    binding.rlTitleGrid.visibility = View.GONE
                    binding.llSearch.visibility = View.GONE
                    binding.llTitleTab.visibility = View.VISIBLE

                    // 3. flTabContent Set First As INVISIBLE(Specific sizes required but not shown)
                    binding.flTabContent.visibility = View.INVISIBLE
                    updateFullScreenState()

                    // 4. ivTransition Start Animation (from Grid item Zoom in to fullscreen)
                    binding.ivTransition.visibility = View.VISIBLE
                    binding.ivTransition.post {
                        ViewAnimationHelper.animateShow(binding.ivTransition, sourceView) {
                            // Recall on retry
                            pendingGridSourceView = sourceView
                            changeMode(PageMode.TAB)
                        }
                        // animateShow It's... onAnimationEnd Yes, I do. flTabContent Switch After Execute
                        // But... animateShow No, I don't. onAnimationEnd Rewind. We'll pass after the animation. postDelayed Take care of it.
                        binding.ivTransition.postDelayed({
                            binding.ivTransition.visibility = View.GONE
                            binding.flTabContent.visibility = View.VISIBLE
                            updateFullScreenState()
                        }, 260)
                    }
                } else {
                    // Screenshot failed. Switch it directly.
                    binding.llTitleTab.visibility = View.VISIBLE
                    binding.flTabContent.visibility = View.VISIBLE
                    binding.rlTitleGrid.visibility = View.GONE
                    binding.llSearch.visibility = View.GONE
                    binding.rvGridContent.visibility = View.GONE
                    updateFullScreenState()
                }
            } else {
                // None sourceView, switch directly
                binding.llTitleTab.visibility = View.VISIBLE
                binding.flTabContent.visibility = View.VISIBLE
                binding.rlTitleGrid.visibility = View.GONE
                binding.llSearch.visibility = View.GONE
                binding.rvGridContent.visibility = View.GONE
                updateFullScreenState()
            }
        } else {
            // GRID Mode - Tab → Grid Animation

            // 1. Intercept Current Tab It's... ReactRootView Screenshot Settings To ivTransition Go, go, go!
            val currentTabId = adapter.currentId
            val currentReactView = if (currentTabId != null) reactViewMap[currentTabId] else null
            val bitmap = if (currentReactView != null) captureViewBitmap(currentReactView) else null

            // 2. Show title bar first
            binding.rlTitleGrid.visibility = View.VISIBLE
            binding.llSearch.visibility = View.VISIBLE
            binding.llTitleTab.visibility = View.GONE

            // 3. Refresh grid adapter To Apply the Screenshot
            gridAdapter.notifyDataSetChanged()

            if (bitmap != null) {
                binding.ivTransition.setImageBitmap(bitmap)
                binding.ivTransition.visibility = View.VISIBLE

                // 4. Hide flTabContent
                binding.flTabContent.visibility = View.GONE
                updateFullScreenState()

                // 5. Let's go first. rvGridContent Layout complete.
                binding.rvGridContent.visibility = View.VISIBLE
                binding.rvGridContent.post {
                    // Find Grid Medium Counterpart item It's... View
                    val targetItemView = findGridItemView(currentTabId)
                    if (targetItemView != null) {
                        // Use animateHide Will ivTransition Zoom Out From Full Screen to Grid item Location
                        ViewAnimationHelper.animateHide(binding.ivTransition, targetItemView) {
                            binding.ivTransition.visibility = View.GONE
                        }
                    } else {
                        // No target found. item, hide directly
                        binding.ivTransition.visibility = View.GONE
                    }
                }
            } else {
                // Screenshot failed. Switch it directly.
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
            // Enable for new architecture Fabric
            if (BuildConfig.IS_NEW_ARCHITECTURE_ENABLED) {
                setIsFabric(true)
            }
            Log.e("TabletActivity", "createReactRootView:${launchOptions}")
            // Start React Apply
            startReactApplication(reactInstanceManager, componentName, launchOptions)
        }
    }

    /**
     * All ReactRootView Screenshot and store local storage path to match ActiveModel It's... screenshot field.
     */
    private fun captureScreenshots() {
        val appList = viewModel.tabletUseCase.value?.appList ?: return

        // Yes. UI Line-by-line intercept Bitmap(draw It must be. UI The #System)
        // I'll send one to you as soon as I've finished. IO Thread compression saves, lower memory peaks
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

        // Waiting for all storages to be completed (no blocking) UI Thread)
        lifecycleScope.launch {
            saveJobs.awaitAll()
        }
    }

    /**
     * Load full resolution from the screenshot file Bitmapfor Grid → Tab Zoom in on the animation
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
     * Intercept View It's... Bitmap
     * @param scaleFactor The scaling factor,1f It's the original resolution.0.5f We'll cut it down to half the size of the size.
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
     * Yes. rvGridContent Center to find corresponding tabId It's... GridItemViewHolder It's... ivScreenshot
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
     * Sync flTabContent Middle View, make sure appList Unanimously.
     */
    private fun syncTabContentViews(appList: List<io.actrium.app.model.ActiveModel>) {
        val flTabContent = binding.flTabContent

        // Update tabIdList
        tabIdList.clear()
        for (item in appList) {
            tabIdList.add(item.id)
        }

        // Remove no longer exists View
        val viewsToRemove = mutableListOf<View>()
        for (i in 0 until flTabContent.childCount) {
            val child = flTabContent.getChildAt(i)
            val tabId = child.tag as? String
            if (tabId != null && !tabIdList.contains(tabId)) {
                viewsToRemove.add(child)
            }
        }
        viewsToRemove.forEach { flTabContent.removeView(it) }

        // Add New View, ensure consistency of order
        for ((index, item) in appList.withIndex()) {
            val reactRootView = reactViewMap[item.id] ?: continue
            val existingView = flTabContent.findViewWithTag<View>(item.id)

            if (existingView == null) {
                // If there is one, parent, remove first
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
                // Ensure View In the right place.
                val currentIndex = flTabContent.indexOfChild(existingView)
                if (currentIndex != index) {
                    flTabContent.removeView(existingView)
                    flTabContent.addView(existingView, index)
                }
            }
        }
    }

    /**
     * Switch to Specify tabId It's... Tab, display the corresponding ReactRootView
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
     * Based on Current flTabContent And his children. View , update the full screen status.
     * When someone... tab It's... ReactRootView Yes. flTabContent Visible and flTabContent The blogger says that the government is not a party to the law.
     * Call enterFullScreen;when tab Call when no longer visible exitFullScreen.
     */
    private fun updateFullScreenState() {
        val flTabContent = binding.flTabContent
        val isTabContentVisible = flTabContent.visibility == View.VISIBLE

        // Find Current flTabContent Visible tab
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

        // Call when status changes, and avoid repeating the sending event
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
        // Do not repeat if already displayed
        if (binding.flSet.isVisible) return

        // Loading a Window Layout
        val popupView = LayoutInflater.from(this).inflate(R.layout.layout_settings_popup, binding.flSet, false)
        binding.flSet.removeAllViews()
        binding.flSet.addView(popupView)
        // Fetch Mask and Content View
        val vMask = popupView.findViewById<View>(R.id.vMask)
        val llContent = popupView.findViewById<FrameLayout>(R.id.llContent)
        val bundle = if (sourceBundle != null && !sourceBundle.isEmpty) {
            sourceBundle
        } else {
            ReactNativeBundle.getSetBundle()
        }
        val targetView = createReactRootView(ReactNativeBundle.REACT_COMPONENT_NAME, bundle)
        llContent.addView(targetView)

        // Set Initial Status
        binding.flSet.visibility = View.VISIBLE
        vMask.alpha = 0f
        llContent.scaleX = 0.7f
        llContent.scaleY = 0.7f
        llContent.alpha = 0f

        // Execute popup animations
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

        // Click Mask Close
        vMask.setOnClickListener {
            viewModel.hideSet()
        }
    }

    private fun hideSettingsPopup() {
        if (!binding.flSet.isVisible) return

        val popupView = binding.flSet.getChildAt(0) ?: return
        val vMask = popupView.findViewById<View>(R.id.vMask)
        val llContent = popupView.findViewById<View>(R.id.llContent)
        // Execute close animation
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
