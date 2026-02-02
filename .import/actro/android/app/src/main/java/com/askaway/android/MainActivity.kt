package com.askaway.android

import android.os.Bundle
import android.view.ViewGroup
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
import androidx.fragment.app.Fragment
import com.askaway.android.databinding.ActivityMainBinding
import com.google.android.material.tabs.TabLayout
import com.facebook.react.ReactRootView
import com.facebook.react.interfaces.fabric.ReactSurface

class MainActivity : AppCompatActivity() {
    private lateinit var binding: ActivityMainBinding
    private val viewModel: MainViewModel by viewModels()
    private var reactSurface: ReactSurface? = null

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)

        val tabLayout = binding.tabLayout
        val reactHost = (application as MainApplication).reactHost
        binding.ivAdd.setOnClickListener {
            binding.flContent.removeAllViews()
            // 停止上一个 surface
            reactSurface?.stop()
            val bundle = Bundle()
            val tabModel = viewModel.genReactNativeTabModel()
            bundle.putBundle("tab", tabModel.toBundle())
            bundle.putBoolean("singleTabMode", true)
            // 创建并启动新的 ReactSurface
            reactSurface = reactHost.createSurface(
                this,
                "canvas-kotlin",
                bundle
            )
            reactSurface?.start()
            // 添加 ReactSurface 的 view
            reactSurface?.view?.let { view ->
                binding.flContent.addView(view)
            }
            reactHost.onHostResume(this)
        }

        // 观察 tabIndex 变化
        viewModel.tabIndex.observe(this) { index ->
            if (tabLayout.selectedTabPosition != index) {
                tabLayout.getTabAt(index)?.select()
            }
        }

        // 添加Tab
        tabLayout.addTab(tabLayout.newTab().setText("原生"))
        tabLayout.addTab(tabLayout.newTab().setText("React Native"))

        // 默认显示原生Fragment
        if (savedInstanceState == null) {
            showFragment(NativeFragment())
        }

        // Tab切换监听
        tabLayout.addOnTabSelectedListener(object : TabLayout.OnTabSelectedListener {
            override fun onTabSelected(tab: TabLayout.Tab?) {
                when (tab?.position) {
                    0 -> {
                        showFragment(NativeFragment())
                        viewModel.setTabIndex(0)
                    }

                    1 -> {
                        showFragment(ReactNativeFragment())
                        viewModel.setTabIndex(1)
                    }
                }
            }

            override fun onTabUnselected(tab: TabLayout.Tab?) {}
            override fun onTabReselected(tab: TabLayout.Tab?) {}
        })
    }

    private fun showFragment(fragment: Fragment) {
        supportFragmentManager.beginTransaction()
            .replace(R.id.fragmentContainer, fragment)
            .commit()
    }

    override fun onDestroy() {
        super.onDestroy()
        reactSurface?.stop()
    }
}
