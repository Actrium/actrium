package io.actrium.app

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
import io.actrium.app.data.database.AppDatabase
import io.actrium.app.data.repository.ActiveRepository
import io.actrium.app.data.sync.BrowserSyncStack
import io.actrium.app.core.ReactNativeBundle
import io.actrium.app.databinding.ActivityMainBinding
import io.actrium.app.global.Params
import io.actrium.app.model.ActiveModel
import io.actrium.app.utils.RuntimeBridgeParams
import com.facebook.react.ReactRootView
import com.facebook.react.modules.core.DefaultHardwareBackBtnHandler
import com.facebook.react.modules.core.PermissionAwareActivity
import com.facebook.react.modules.core.PermissionListener
import com.facebook.react.uimanager.PixelUtil.pxToDp
import java.io.File
import kotlinx.coroutines.launch
import io.actrium.app.utils.adjustKeyboard
import io.actrium.app.utils.getStatusBarHeight
import io.actrium.app.utils.screenshot
import io.actrium.app.utils.ViewAnimationHelper
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
    private lateinit var activeRecyclerAdapter: io.actrium.app.adapter.ActiveRecyclerAdapter
    private var isSearchMode = false
    private var isFromDeepLink = false
    private var originalSearchWidth = 0
    private var originalNavWidth = 0
    private var currentNightMode = 0

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        currentNightMode = resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK

        // Sets the insinuation to allow content to reach the status bar area while also processing the font colours of the status bar
        WindowCompat.setDecorFitsSystemWindows(window, false)
        window.statusBarColor = Color.TRANSPARENT
        // Assume that the background is light and set the status bar icon to dark
        WindowCompat.getInsetsController(window, window.decorView).isAppearanceLightStatusBars = false

        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)

        ViewCompat.setOnApplyWindowInsetsListener(binding.root) { v, insets ->
            val systemBars = insets.getInsets(WindowInsetsCompat.Type.systemBars())
            val navBars = insets.getInsets(WindowInsetsCompat.Type.navigationBars())
            val imeBars = insets.getInsets(WindowInsetsCompat.Type.ime())
            // Apply the inner margin of the left, right, bottom, and keep top unprocessed to achieve content extension to the status bar
            // Bottom padding Make sure the whole layout is above the virtual key
            v.setPadding(systemBars.left, 0, systemBars.right, navBars.bottom)
            // Root layout. paddingBottom The temperature of the virtual key is processed.flNavBar No additional requirements bottomMargin,
            // Otherwise the spacing is double calculated (padding + margin = twice as high as virtual keys)
            // When keyboard pops up IME inset Height of virtual keys is included (adjustKeyboard It'll be used as padding)
            val isImeVisible = imeBars.bottom > 0
            binding.flNavBar.updateLayoutParams<ViewGroup.MarginLayoutParams> {
                bottomMargin = 0
            }
            insets
        }


        // Initialize flReactNative To Hide Status
        binding.flReactNative.visibility = View.INVISIBLE // Use INVISIBLE Not GONE, to get height

        // Use ViewTreeObserver Make sure that the view is fully configured before setting the initial position
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

        // Controls remove buttons to show and hide when the search box contents change
        binding.etSearch.addTextChangedListener(object : android.text.TextWatcher {
            override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
            override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {
                binding.ivDel.visibility = if (s.isNullOrEmpty()) View.GONE else View.VISIBLE
            }
            override fun afterTextChanged(s: android.text.Editable?) {
                viewModel.changeInputText(s.toString())
            }
        })
        // Remove button click empty search box contents
        binding.ivDel.setOnClickListener {
            binding.etSearch.text.clear()
        }

        binding.ivSearch.setOnClickListener {
            showSearch()
        }

        binding.flNavButton.setOnClickListener {
            hideSearch()
        }

        // Set the navigation bar selection monitor
        setupNavigationListener()

        // Initialise the position and content of the navigation indicator
        binding.rgNavigation.post {
            // Set first navActivity Organisation
            binding.navActivity.isChecked = true
            // Do not use animation, directly to navActivity Location
            alignIndicatorToSelectedButton(animate = false)
        }

        // Initialize RecyclerView and Adapter
        activeRecyclerAdapter = io.actrium.app.adapter.ActiveRecyclerAdapter()
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
        // Settings item Click Event
        activeRecyclerAdapter.onItemClick = { activeModel, view ->
            // Show flRNContent
            showReactNativeView(view)
            // Through flRNContent The son. view♪ Will tag == activeModel.id It's... view bringToFront
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

        // Set to delete the click event
        activeRecyclerAdapter.onDeleteClick = { activeModel ->
            // Remove First flRNContent Corresponding child
            for (i in binding.flRNContent.childCount - 1 downTo 0) {
                val child = binding.flRNContent.getChildAt(i)
                if (child.tag == activeModel.id) {
                    binding.flRNContent.removeViewAt(i)
                    // It has to be manually unmounted. React Apply to Release JS Resources and discontinuation
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
            // Plays the deleted animation, updates it after the animation ViewModel
            activeRecyclerAdapter.removeItemWithAnimation(activeModel.id) {
                viewModel.removeActiveModel(activeModel.id)
            }
            // Not here anymore. finishby updateData Revert judgement
        }

        binding.ivAdd.postDelayed({
            if (isFromDeepLink) {
                // deeplink Do not automatically create or open when entering, by handleDeepLink Process
                isFromDeepLink = false
            } else if (viewModel.isActiveListEmpty()) {
                binding.ivAdd.performClick()
            } else {
                // List does not automatically click first when empty item
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

        // Handled with first startup deeplink
        handleDeepLink(intent)
    }

    /**
     * Set up the navigation bar listening device
     */
    private fun setupNavigationListener() {
        binding.rgNavigation.setOnCheckedChangeListener { _, checkedId ->
            alignIndicatorToSelectedButton(animate = true)
        }
    }

    /**
     * Align indicator to the selected RadioButton
     */
    private fun alignIndicatorToSelectedButton(animate: Boolean = false) {
        val checkedId = binding.rgNavigation.checkedRadioButtonId
        if (checkedId == -1) return

        // Fetch Selected RadioButton
        val selectedButton = binding.rgNavigation.findViewById<View>(checkedId) ?: return

        // Calculates the centre position of the selected button
        val buttonCenterX = selectedButton.left + selectedButton.width / 2f

        // Calculating the location to which the indicator should be moved (coordinate the center of the indicator with the center of the button)
        val indicatorCenterOffset = binding.flNavIndicator.width / 2f
        val targetX = buttonCenterX - indicatorCenterOffset

        Log.d("MainActivity", "Align indicator: checkedId=$checkedId, buttonCenterX=$buttonCenterX, targetX=$targetX")

        if (animate) {
            // Create Slide Animation
            ObjectAnimator.ofFloat(binding.flNavIndicator, "translationX", targetX).apply {
                duration = 200
                interpolator = DecelerateInterpolator()
                start()
            }
        } else {
            // Set position directly (no animation is required at initialization)
            binding.flNavIndicator.translationX = targetX
        }

        // Switch the corresponding contents to the selected label
        updateContentVisibility(checkedId)
    }

    /**
     * Update the invisible status of content components
     */
    private fun updateContentVisibility(checkedId: Int, showPage: Boolean = true) {
        when (checkedId) {
            R.id.navTag -> if (showPage) showBookmark()
            R.id.navActivity -> {
                // Show active pages
                binding.flBookmark.visibility = View.GONE
                binding.llActive.visibility = View.VISIBLE
                binding.flSet.visibility = View.GONE
                // Based on activeList Whether to empty the empty view invisible
                binding.llEmptyView.visibility = if (viewModel.isActiveListEmpty()) View.VISIBLE else View.GONE
            }

            R.id.navSettings -> if (showPage) showSet()
        }
        // Hide a blank view while switching to an inactive page
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
     * Creates a new structure to support ReactRootView
     */
    private fun createReactRootView(componentName: String, launchOptions: Bundle?): ReactRootView {
        val application = application as MainApplication
        val reactInstanceManager = application.reactNativeHost.reactInstanceManager

        return ReactRootView(this).apply {
            // Enable for new architecture Fabric
            if (BuildConfig.IS_NEW_ARCHITECTURE_ENABLED) {
                setIsFabric(true)
            }

            // Start React Apply
            startReactApplication(reactInstanceManager, componentName, launchOptions)
        }
    }

    /**
     * Show from Bottom ReactNative View
     */
    private fun showReactNativeView(sourceView: View? = null) {
        if (isReactNativeVisible) return
        isReactNativeVisible = true

        ViewAnimationHelper.animateShow(binding.flReactNative, sourceView) {
            // If View Requires retry (e.g., size not ready) and reset and retry later
            isReactNativeVisible = false
            showReactNativeView(sourceView)
        }
    }

    /**
     * Enter Search Mode
     */
    private fun enterSearchMode() {
        if (isSearchMode) return
        isSearchMode = true

        // Save original width
        originalSearchWidth = binding.ivSearch.width
        originalNavWidth = (binding.rgNavigation.parent as View).width

        // Based on currently selected RadioButton Sets the corresponding icons and text
        val checkedId = binding.rgNavigation.checkedRadioButtonId

        // Set Icon
        val iconRes = when (checkedId) {
            R.id.navTag -> R.drawable.nav_tag_selector
            R.id.navActivity -> R.drawable.nav_activity_selector
            R.id.navSettings -> R.drawable.nav_settings_selector
            else -> R.drawable.nav_activity_selector
        }

        // Set Text
        val textRes = when (checkedId) {
            R.id.navTag -> R.string.tab_bookmark
            R.id.navActivity -> R.string.tab_active
            R.id.navSettings -> R.string.tab_set
            else -> R.string.tab_active
        }

        // Use setCompoundDrawablesRelativeWithIntrinsicBounds And keep it. padding
//        binding.ivNav.setCompoundDrawablesRelativeWithIntrinsicBounds(0, iconRes, 0, 0)
        binding.ivNav.compoundDrawablePadding = (4 * resources.displayMetrics.density).toInt()
        binding.ivNav.setText(textRes)
        binding.ivNav.isChecked = false

        // Settings ivNav It's... drawable tint Colours labels_vibrant_controls_primary(RadioButtonUsesetButtonDrawableandsetCompoundDrawables)
        val topDrawable = ContextCompat.getDrawable(this, iconRes)
        topDrawable?.setTint(ContextCompat.getColor(this, R.color.labels_vibrant_controls_primary))
        binding.ivNav.setCompoundDrawablesWithIntrinsicBounds(null, topDrawable, null, null)
        binding.ivNav.setTextColor(ContextCompat.getColor(this, R.color.labels_vibrant_controls_primary))

        // Show llSearchMode
        binding.llSearchMode.visibility = View.VISIBLE
        binding.llSearchMode.alpha = 0f

        // Create animated
        val duration = 300L

        // ivSearch Zoom in to llEdit width
        val searchStartWidth = originalSearchWidth
        val searchEndWidth = binding.llEdit.width

        // rgNavigation Zoom in to flNavButton width
        val navStartWidth = originalNavWidth
        val navEndWidth = binding.flNavButton.width

        ValueAnimator.ofFloat(0f, 1f).apply {
            this.duration = duration
            interpolator = DecelerateInterpolator()

            addUpdateListener { animator ->
                val fraction = animator.animatedValue as Float

                // ivSearch Width change
                val currentSearchWidth = (searchStartWidth + (searchEndWidth - searchStartWidth) * fraction).toInt()
                binding.ivSearch.layoutParams = binding.ivSearch.layoutParams.apply {
                    width = currentSearchWidth
                }

                // rgNavigation Change in width of parent container
                (binding.rgNavigation.parent as? View)?.let { navParent ->
                    navParent.layoutParams = navParent.layoutParams.apply {
                        width = (navStartWidth + (navEndWidth - navStartWidth) * fraction).toInt()
                    }
                }

                // llSearchMode Change in transparency
                binding.llSearchMode.alpha = fraction

                // llNav Change in transparency
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
     * Exiting Search Mode
     */
    private fun exitSearchMode(onEnd: (() -> Unit)? = null) {
        if (!isSearchMode) return
        isSearchMode = false

        // Show llNav
        binding.llNav.visibility = View.VISIBLE
        binding.llNav.alpha = 0f

        // Create animated
        val duration = 300L

        // ivSearch Reduce original size back from current width
        val searchStartWidth = binding.ivSearch.width
        val searchEndWidth = originalSearchWidth

        //rgNavigation Magnify back to original size from current width
        val navStartWidth = (binding.rgNavigation.parent as View).width
        val navEndWidth = originalNavWidth

        ValueAnimator.ofFloat(0f, 1f).apply {
            this.duration = duration
            interpolator = DecelerateInterpolator()

            addUpdateListener { animator ->
                val fraction = animator.animatedValue as Float

                // ivSearch Width change
                val currentSearchWidth = (searchStartWidth + (searchEndWidth - searchStartWidth) * fraction).toInt()
                binding.ivSearch.layoutParams = binding.ivSearch.layoutParams.apply {
                    width = currentSearchWidth
                }

                // rgNavigation Change in width of parent container
                (binding.rgNavigation.parent as? View)?.let { navParent ->
                    navParent.layoutParams = navParent.layoutParams.apply {
                        width = (navStartWidth + (navEndWidth - navStartWidth) * fraction).toInt()
                    }
                }

                // llNav Change in transparency
                binding.llNav.alpha = fraction

                // llSearchMode Change in transparency
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
     * Show Search View From Bottom
     */
    private fun showSearch(sourceBundle: Bundle? = null) {
        if (isSearchVisible) return

        isSearchVisible = true
        enterSearchMode()
        // Empty Search Input Box
        binding.etSearch.text.clear()
        // Remove flSearch All sub-components in
        binding.flSearch.removeAllViews()

        // Load Search React Native View
        val bundle = sourceBundle ?: ReactNativeBundle.getSearchBundle()
        Log.e("MainActivity", "bundle:${bundle}")
        val targetView = createReactRootView(ReactNativeBundle.REACT_COMPONENT_NAME, bundle)
        binding.flSearch.addView(targetView)

        // Initial position at bottom of screen
        binding.flSearch.translationY = binding.flSearch.height.toFloat()
        binding.flSearch.visibility = View.VISIBLE
        // Yes flSearch Add WindowInsets Listen, handle keyboard problems.
        binding.flSearch.adjustKeyboard()
        // Request Application window insets
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

        // Create animated animations that slide from the bottom
        ObjectAnimator.ofFloat(binding.flSearch, "translationY", binding.flSearch.height.toFloat(), 0f).apply {
            duration = 300
            interpolator = DecelerateInterpolator()
            start()
        }
    }

    /**
     * Hide Search View Bottom
     */
    private fun hideSearch(onSearchHidden: (() -> Unit)? = null) {
        if (!isSearchVisible) return
        exitSearchMode {
            alignIndicatorToSelectedButton(animate = false)
            onSearchHidden?.invoke()
        }

        // Put the keyboard down.
        val imm = getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager
        binding.etSearch.clearFocus()
        imm.hideSoftInputFromWindow(binding.etSearch.windowToken, 0)

        // Create animated slides out of the bottom
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
     * Hide ReactNative View Bottom
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
     * Pass. tabId Calculate recyApps Medium Counterpart item Yes. window Position and Size in
     */
    private fun calculateItemRectByTabId(tabId: String?): android.graphics.Rect? {
        if (tabId.isNullOrEmpty()) return null
        val position = activeRecyclerAdapter.findPositionById(tabId)
        if (position == -1) return null

        val recyApps = binding.recyApps
        val density = resources.displayMetrics.density

        // item Fixed size: height 200dpUp and down. margin 8dpRight and left. margin 16dp
        val itemHeightPx = (200 * density).toInt()
        val itemMarginVPx = (8 * density).toInt()
        val itemMarginHPx = (16 * density).toInt()
        val itemWidthPx = recyApps.width - itemMarginHPx * 2

        // Calculate item Yes. recyApps Internal top Location
        // recyApps Yes. paddingTop=80dp, item Spacing is itemMarginVPx*2
        val paddingTopPx = recyApps.paddingTop
        val itemTopInRecycler = paddingTopPx + position * (itemHeightPx + itemMarginVPx * 2) + itemMarginVPx
        val itemLeftInRecycler = itemMarginHPx

        // Consider RecyclerView Scroll Offset
        val scrollOffset = recyApps.computeVerticalScrollOffset()

        // Will recyApps Coordinate to window Coordinates
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

        // Save ActiveList To Database
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
     * Based on uiMode Change to refresh all original views with colours and background
     * When? configChanges Organisation uiMode I'm not sure if I'm going to be able to do this.Activity No, I won't. recreate,
     * Rendered View Keeps old colour values, needs to be updated manually resources Read Reread
     */
    private fun applyThemeToViews() {
        val isDark = (currentNightMode == Configuration.UI_MODE_NIGHT_YES)

        // Updates the status bar icon colour: light colour pattern uses dark colour icons and dark colour pattern uses light colour icons
        WindowCompat.getInsetsController(window, window.decorView).isAppearanceLightStatusBars = !isDark

        // Reapplies the Navigator Bar Background (reference) @color/nav_bgLight colour=White, dark.=Gray)
        (binding.rgNavigation.parent as? View)?.background =
            ContextCompat.getDrawable(this, R.drawable.bottom_nav_bg)
        binding.flNavIndicator.background =
            ContextCompat.getDrawable(this, R.drawable.bottom_nav_indicator)

        // Reapplies RadioButton . Colours and icons for text tint(References) @color/nav_text_selector)
        val navTextSelector = ContextCompat.getColorStateList(this, R.color.nav_text_selector)
        listOf(binding.navTag, binding.navActivity, binding.navSettings).forEach { rb ->
            rb.setTextColor(navTextSelector)
            TextViewCompat.setCompoundDrawableTintList(rb, navTextSelector)
        }

        // Reapplies search icon background and tint(References) @drawable/bottom_nav_bg, @color/labels_vibrant_controls_primary)
        binding.ivSearch.background = ContextCompat.getDrawable(this, R.drawable.bottom_nav_bg)
        binding.ivSearch.imageTintList =
            ContextCompat.getColorStateList(this, R.color.labels_vibrant_controls_primary)

        // Reapplies search mode-related views
        binding.flNavButton.background = ContextCompat.getDrawable(this, R.drawable.bottom_nav_bg)
        binding.llEdit.background = ContextCompat.getDrawable(this, R.drawable.bottom_nav_bg)
        binding.ivSearchIcon.imageTintList =
            ContextCompat.getColorStateList(this, R.color.labels_vibrant_controls_primary)

        // Update if currently searching mode ivNav Colours
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

        // Refresh RecyclerView List Item
        activeRecyclerAdapter.notifyDataSetChanged()
    }

    companion object {
        /**
         * Toggle Dark/Light color mode
         * @param mode AppCompatDelegate.MODE_NIGHT_FOLLOW_SYSTEM / MODE_NIGHT_NO / MODE_NIGHT_YES
         */
        fun switchTheme(mode: Int) {
            AppCompatDelegate.setDefaultNightMode(mode)
        }
    }

    /**
     * Parsing deeplink And send the data to React Native
     * Example of Link Format:
     *   actr://example.com/chat/123?token=abc
     *   https://app.actrium.example/chat/123?token=abc
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

        // name Do not show collection lines for empty hours (and item Medium ivMark The hidden logic of the same)
        val showFav = activeModel.name.isNotEmpty()
        llFav.visibility = if (showFav) View.VISIBLE else View.GONE

        // Current only item Do not show time"Close Other Pages"
        val hasOtherItems = viewModel.activeList.value.size > 1
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
            // with item Collections on the same level of operation
            activeRecyclerAdapter.onMarkClick?.invoke(activeModel)
        }
        popupView.findViewById<View>(R.id.llClose).setOnClickListener {
            popup.dismiss()
            // with item The deletions on the list are consistent.
            activeRecyclerAdapter.onDeleteClick?.invoke(activeModel)
        }
        llCloseOther.setOnClickListener {
            closeOtherTabs(activeModel.id)
            popup.dismiss()
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
     * Close Discard Designation tab Everything else tab
     */
    private fun closeOtherTabs(keepId: String) {
        val allItems = viewModel.activeList.value
        val others = allItems.filter { it.id != keepId }
        for (model in others) {
            // Remove flRNContent Corresponding child
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
        // Hide the search view if it is being displayed
        if (isSearchVisible) {
            hideSearch()
            return
        }

        // If flReactNative Showing, hiding it
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
