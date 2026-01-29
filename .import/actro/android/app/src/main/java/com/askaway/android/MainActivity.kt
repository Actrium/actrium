package com.askaway.android

import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import androidx.fragment.app.Fragment
import com.google.android.material.tabs.TabLayout

class MainActivity : AppCompatActivity() {

  override fun onCreate(savedInstanceState: Bundle?) {
    super.onCreate(savedInstanceState)
    setContentView(R.layout.activity_main)

    val tabLayout = findViewById<TabLayout>(R.id.tabLayout)

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
          0 -> showFragment(NativeFragment())
          1 -> showFragment(ReactNativeFragment())
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
