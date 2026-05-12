package chat.askaway.android.splash

import android.content.Intent
import android.content.res.Configuration
import android.net.Uri
import android.os.Bundle
import android.util.Log
import androidx.appcompat.app.AppCompatActivity
import chat.askaway.android.tablet.TabletActivity
import com.askaway.android.MainActivity
import com.askaway.android.databinding.ActivitySplashBinding

class SplashActivity : AppCompatActivity() {
    private lateinit var binding: ActivitySplashBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (!isTaskRoot()) {
            // App 已在运行，如果有 deeplink 则转发到目标 Activity
            val deepLinkUri = intent.data
            if (deepLinkUri != null) {
                forwardDeepLink(deepLinkUri)
            }
            finish()
            return
        }
        binding = ActivitySplashBinding.inflate(layoutInflater)
        setContentView(binding.root)
        val nextIntent = if (isTablet()) TabletActivity::class.java
        else MainActivity::class.java
        val intent = Intent(this, nextIntent)
        // 将 deeplink 数据转发到目标 Activity
        intent.data = getIntent().data
        intent.action = getIntent().action
        intent.flags = getIntent().flags
        startActivity(intent)
        finish()
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        val deepLinkUri = intent.data
        if (deepLinkUri != null) {
            Log.d("SplashActivity", "onNewIntent deeplink: $deepLinkUri")
            forwardDeepLink(deepLinkUri)
        }
        finish()
    }

    /**
     * 将 deeplink 转发到已运行的目标 Activity
     * 使用 FLAG_ACTIVITY_SINGLE_TOP 使得目标 Activity 收到 onNewIntent 而不是新建实例
     */
    private fun forwardDeepLink(uri: Uri) {
        val nextClass = if (isTablet()) TabletActivity::class.java
        else MainActivity::class.java
        val forwardIntent = Intent(this, nextClass)
        forwardIntent.data = uri
        forwardIntent.action = Intent.ACTION_VIEW
        forwardIntent.flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
        startActivity(forwardIntent)
    }

    private fun isTablet(): Boolean {
        return (resources.configuration.screenLayout and Configuration.SCREENLAYOUT_SIZE_MASK) >= Configuration.SCREENLAYOUT_SIZE_LARGE
    }
}