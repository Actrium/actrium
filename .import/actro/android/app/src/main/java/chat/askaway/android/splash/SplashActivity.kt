package chat.askaway.android.splash

import android.os.Bundle
import android.content.res.Configuration
import androidx.appcompat.app.AppCompatActivity
import chat.askaway.android.tablet.TabletActivity
import com.askaway.android.databinding.ActivitySplashBinding
import com.askaway.android.MainActivity
import android.content.Intent

class SplashActivity : AppCompatActivity() {
    private lateinit var binding: ActivitySplashBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
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