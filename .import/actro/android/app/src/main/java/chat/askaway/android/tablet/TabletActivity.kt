package chat.askaway.android.tablet

import android.os.Bundle
import androidx.appcompat.app.AppCompatActivity
import com.askaway.android.databinding.ActivityTabletBinding

class TabletActivity : AppCompatActivity() {

    private lateinit var binding: ActivityTabletBinding

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        binding = ActivityTabletBinding.inflate(layoutInflater)
        setContentView(binding.root)
    }
}