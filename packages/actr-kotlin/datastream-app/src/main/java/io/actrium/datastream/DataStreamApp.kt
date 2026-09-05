package io.actrium.datastream

import android.app.Application
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

/// Process-level holder of the single [ActrService] — and thus the single ACTR
/// node. Activities are created and destroyed (config-change recreate, a
/// stacked instance from before launchMode=singleTask, etc.), but there must be
/// exactly one node per process: multiple concurrent nodes each register with
/// the signaling server (duplicate connect/disconnect events) and each new
/// Activity's ForegroundEchoTrigger starts in its initial state, so its first
/// onResume skips the echo. Owning the service in the Application guarantees
/// every Activity shares one node + one foreground-recovery gate.
class DataStreamApp : Application() {
    val service: ActrService by lazy {
        ActrService(this, CoroutineScope(SupervisorJob() + Dispatchers.Main))
    }
}
