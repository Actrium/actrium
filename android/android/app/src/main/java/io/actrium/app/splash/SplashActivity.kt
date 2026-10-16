package io.actrium.app.splash

import android.content.Intent
import android.content.res.Configuration
import android.net.Uri
import android.os.Bundle
import android.util.Log
import androidx.appcompat.app.AppCompatActivity
import io.actrium.app.tablet.TabletActivity
import io.actrium.app.MainActivity
import io.actrium.app.databinding.ActivitySplashBinding

class SplashActivity : AppCompatActivity() {
    private lateinit var binding: ActivitySplashBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (!isTaskRoot()) {
            // App Already running if there is deeplink Send to Target Activity
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
        // Will deeplink Data forward to target Activity
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
     * Will deeplink Forward to run target Activity
     * Use FLAG_ACTIVITY_SINGLE_TOP Make the target Activity Copy that. onNewIntent , not create a new instance
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