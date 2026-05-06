package chat.askaway.android.splash

import android.content.Intent
import android.content.res.Configuration
import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import chat.askaway.android.tablet.TabletActivity
import com.askaway.android.MainActivity
import com.askaway.android.databinding.ActivitySplashBinding

class SplashActivity : AppCompatActivity() {
    private lateinit var binding: ActivitySplashBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (!isTaskRoot()) {
            finish()
            return
        }
        binding = ActivitySplashBinding.inflate(layoutInflater)
        setContentView(binding.root)
        val nextIntent = if (isTablet()) TabletActivity::class.java
        else MainActivity::class.java
        startActivity(Intent(this, nextIntent))
        finish()
    }

    private fun isTablet(): Boolean {
        return (resources.configuration.screenLayout and Configuration.SCREENLAYOUT_SIZE_MASK) >= Configuration.SCREENLAYOUT_SIZE_LARGE
    }
}