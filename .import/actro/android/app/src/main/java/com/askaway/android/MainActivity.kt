package com.askaway.android

import android.os.Bundle
import androidx.activity.viewModels
import androidx.appcompat.app.AppCompatActivity
import androidx.fragment.app.Fragment
import com.askaway.android.databinding.ActivityMainBinding
import com.google.android.material.tabs.TabLayout
import com.facebook.react.ReactInstanceManager

class MainActivity : AppCompatActivity() {
    private lateinit var binding: ActivityMainBinding
    private val viewModel: MainViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityMainBinding.inflate(layoutInflater)
        setContentView(binding.root)

        val tabLayout = binding.tabLayout
        binding.ivAdd.setOnClickListener {
            // 可在此处通过 viewModel 操作数据

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
}
