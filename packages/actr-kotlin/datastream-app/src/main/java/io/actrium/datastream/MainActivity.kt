package io.actrium.datastream

import android.content.Intent
import android.os.Bundle
import android.view.ViewTreeObserver
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Cancel
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.IosShare
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.FileProvider
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.launch
import java.io.File

private const val TAG = "DataStreamApp"

class MainActivity : ComponentActivity() {
    private lateinit var service: ActrService

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // Process-scoped singleton (owned by DataStreamApp): one ActrService /
        // ACTR node per process, shared across Activity recreations. Avoids the
        // duplicate-node + per-Activity foreground-echo-gate reset that a
        // per-Activity ActrService caused.
        service = (application as DataStreamApp).service
        setContent {
            DataStreamAppTheme {
                Surface(modifier = Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
                    ContentView(service)
                }
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        // Intentionally do NOT stop the runtime here. The ActrService / node is
        // process-scoped (held by DataStreamApp); an Activity being destroyed
        // (config-change recreate, or a stacked instance from before
        // launchMode=singleTask) must not tear down the node the live Activity
        // still uses. The node lives until the process dies.
    }
}

@Composable
private fun DataStreamAppTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = lightColorScheme(
            primary = Color(0xFF1976D2),
            background = Color.White,
        ),
        content = content,
    )
}

@Composable
private fun ContentView(service: ActrService) {
    val state by service.uiState.collectAsState()
    var echoInputText by remember { mutableStateOf("hello") }
    var streamChunkCountText by remember { mutableStateOf("3") }
    var isExportingLog by remember { mutableStateOf(false) }
    var logExportError by remember { mutableStateOf<String?>(null) }
    val context = LocalContext.current
    val scope = rememberCoroutineScope()

    val streamChunkCount: Int? = streamChunkCountText.trim().toIntOrNull()?.let { if (it > 0) it else null }
    val isReady = state.status.startsWith("Ready")

    // .task { startIfNeeded(); auto-run } + .onChange(scenePhase) foreground echo
    LaunchedEffect(Unit) {
        service.startIfNeeded()
        android.util.Log.i(TAG, "startIfNeeded returned, shouldAutoRun=${service.shouldAutoRun}, autoStreamCount=${service.autoStreamCount}, isReady=${service.isReady}")
        if (service.shouldAutoRun) {
            while (!service.isReady) { kotlinx.coroutines.delay(500) }
            kotlinx.coroutines.delay(1000)
            service.runAllProbes()
            while (service.uiState.value.isRunning) { kotlinx.coroutines.delay(500) }
        }
        service.autoStreamCount?.let { count ->
            while (!service.isReady) { kotlinx.coroutines.delay(500) }
            while (service.uiState.value.isRunning) { kotlinx.coroutines.delay(500) }
            kotlinx.coroutines.delay(1000)
            service.sendHelloStreamChunks(count)
            while (service.uiState.value.isSendingStream) { kotlinx.coroutines.delay(500) }
            service.autoResultFilename?.let { service.writeAutoResultFile(it) }
        }
    }

    val lifecycleOwner = LocalLifecycleOwner.current
    val view = LocalView.current
    val currentEchoInputText by rememberUpdatedState(echoInputText)
    val foregroundEchoTrigger = remember { ForegroundEchoTrigger() }

    // Swift's scenePhase becomes inactive while Control Center is open and
    // active again when it closes. Android's notification/quick-settings
    // shade does not pause the Activity: it remains RESUMED while StatusBar
    // owns window focus. Observe both lifecycle and window focus to preserve
    // that behavior, while ForegroundEchoTrigger coalesces a real
    // pause/resume plus its accompanying focus transition into one echo.
    DisposableEffect(lifecycleOwner, view) {
        fun requestEcho(source: String) {
            service.logToFile("[DataStreamApp] foreground_echo_trigger source=$source")
            service.requestForegroundEcho(currentEchoInputText)
        }

        val lifecycleObserver = LifecycleEventObserver { _, event ->
            when (event) {
                Lifecycle.Event.ON_RESUME -> {
                    service.onAppForeground()
                    if (foregroundEchoTrigger.onResume()) {
                        requestEcho("activity-resume")
                    }
                }
                Lifecycle.Event.ON_PAUSE -> {
                    foregroundEchoTrigger.onPause()
                    service.onAppBackground()
                }
                else -> {}
            }
        }

        val windowFocusObserver =
            ViewTreeObserver.OnWindowFocusChangeListener { hasFocus ->
                val isResumed =
                    lifecycleOwner.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED)
                if (foregroundEchoTrigger.onWindowFocusChanged(hasFocus, isResumed)) {
                    requestEcho("window-focus")
                }
            }

        val viewTreeObserver = view.viewTreeObserver
        lifecycleOwner.lifecycle.addObserver(lifecycleObserver)
        viewTreeObserver.addOnWindowFocusChangeListener(windowFocusObserver)
        onDispose {
            lifecycleOwner.lifecycle.removeObserver(lifecycleObserver)
            if (viewTreeObserver.isAlive) {
                viewTreeObserver.removeOnWindowFocusChangeListener(windowFocusObserver)
            }
        }
    }

    Column(modifier = Modifier.fillMaxSize().padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        // Status
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(
                modifier = Modifier
                    .size(10.dp)
                    .clip(CircleShape)
                    .background(if (isReady) Color(0xFF4CAF50) else Color(0xFFFFA726)),
            )
            Spacer(Modifier.width(8.dp))
            Text(
                text = state.status,
                fontSize = 12.sp,
                color = if (isReady) Color(0xFF4CAF50) else MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        // Hook state (3 monospaced lines)
        Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
            Text(
                "Signaling hook: ${state.signalingHookStatus.name.lowercase()} · events ${state.signalingHookEventCount}",
                fontFamily = FontFamily.Monospace,
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                "WebSocket hook: ${state.webSocketHookStatus.name.lowercase()} · connecting ${state.webSocketConnectingCount} · connected ${state.webSocketConnectedCount} · disconnected ${state.webSocketDisconnectedCount}",
                fontFamily = FontFamily.Monospace,
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Text(
                "WebRTC hook: ${webRtcHookStatusName(state.webRtcStatus)} · connected ${state.webRtcConnectedCount} · recovering ${state.webRtcRecoveringCount} · idle ${state.webRtcIdleCount}",
                fontFamily = FontFamily.Monospace,
                fontSize = 11.sp,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        // Run All button
        Button(
            onClick = { scope.launch { service.runAllProbes() } },
            enabled = service.canSend && !state.isRunning && !state.isSendingStream,
            modifier = Modifier.fillMaxWidth(),
        ) {
            if (state.isRunning) {
                CircularProgressIndicator(modifier = Modifier.size(16.dp), color = Color.White, strokeWidth = 2.dp)
                Spacer(Modifier.width(8.dp))
            }
            Text(if (state.isRunning) "Running..." else "Run All")
        }

        // Echo sender
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = echoInputText,
                    onValueChange = { echoInputText = it },
                    label = { Text("Message") },
                    enabled = !state.isSendingEcho,
                    modifier = Modifier.weight(1f),
                    singleLine = true,
                )
                Spacer(Modifier.width(8.dp))
                Button(
                    onClick = {
                        val msg = echoInputText.trim()
                        if (msg.isNotEmpty()) scope.launch { service.sendEcho(msg) }
                    },
                    enabled = service.canSend && echoInputText.trim().isNotEmpty(),
                    modifier = Modifier.weight(1f),
                ) {
                    if (state.isSendingEcho) {
                        CircularProgressIndicator(modifier = Modifier.size(16.dp), color = Color.White, strokeWidth = 2.dp)
                        Spacer(Modifier.width(8.dp))
                    }
                    Text(if (state.isSendingEcho) "Calling..." else "Send Echo")
                }
            }

            state.echoResult?.let { result ->
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(8.dp))
                        .background(MaterialTheme.colorScheme.surfaceVariant)
                        .padding(8.dp),
                ) {
                    Text(
                        result,
                        fontFamily = FontFamily.Monospace,
                        fontSize = 12.sp,
                        color = if (result.startsWith("[FAIL]")) Color.Red else Color(0xFF4CAF50),
                    )
                }
            }
        }

        // Stream sender
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = streamChunkCountText,
                    onValueChange = { streamChunkCountText = it.filter { c -> c.isDigit() } },
                    label = { Text("Chunks") },
                    enabled = !state.isSendingStream,
                    modifier = Modifier.width(110.dp),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                    singleLine = true,
                )
                Spacer(Modifier.width(8.dp))
                Button(
                    onClick = {
                        val count = streamChunkCount ?: return@Button
                        scope.launch { service.sendHelloStreamChunks(count) }
                    },
                    enabled = service.canSend && !state.isRunning && !state.isSendingStream && streamChunkCount != null,
                    modifier = Modifier.weight(1f),
                ) {
                    if (state.isSendingStream) {
                        CircularProgressIndicator(modifier = Modifier.size(16.dp), color = Color.White, strokeWidth = 2.dp)
                        Spacer(Modifier.width(8.dp))
                    }
                    Text(if (state.isSendingStream) "Sending..." else "Send Stream")
                }
            }

            if (state.receivedEchoLines.isNotEmpty()) {
                val listState = rememberLazyListState()
                LaunchedEffect(state.receivedEchoLines.size) {
                    if (state.receivedEchoLines.isNotEmpty()) {
                        listState.animateScrollToItem(state.receivedEchoLines.size - 1)
                    }
                }
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .height(120.dp)
                        .clip(RoundedCornerShape(8.dp))
                        .background(MaterialTheme.colorScheme.surfaceVariant)
                        .padding(8.dp),
                ) {
                    LazyColumn(state = listState, verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        items(state.receivedEchoLines) { line ->
                            Text(
                                receivedDisplayText(line),
                                fontFamily = FontFamily.Monospace,
                                fontSize = 12.sp,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
            }
        }

        // Probe results
        if (state.results.isNotEmpty()) {
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(260.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(MaterialTheme.colorScheme.surface),
            ) {
                LazyColumn {
                    items(state.results) { result ->
                        Row(
                            modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Icon(
                                if (result.passed) Icons.Filled.CheckCircle else Icons.Filled.Cancel,
                                contentDescription = null,
                                tint = if (result.passed) Color(0xFF4CAF50) else Color.Red,
                                modifier = Modifier.size(20.dp),
                            )
                            Spacer(Modifier.width(8.dp))
                            Column(modifier = Modifier.weight(1f)) {
                                Text(result.name, fontSize = 14.sp)
                                Text(result.details, fontSize = 12.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                            Text("${result.durationMs}ms", fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                    }
                }
            }
        }

        // Log output
        if (state.logLines.isNotEmpty()) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedButton(onClick = { service.clearLogs() }) {
                    Icon(Icons.Filled.Delete, contentDescription = null, modifier = Modifier.size(16.dp))
                    Spacer(Modifier.width(4.dp))
                    Text("Clear")
                }
                Spacer(Modifier.weight(1f))
                OutlinedButton(
                    onClick = {
                        isExportingLog = true
                        logExportError = null
                        try {
                            val file = service.exportLogFile()
                            shareLogFile(context, file)
                        } catch (e: Exception) {
                            logExportError = "Log export failed: ${e.message}"
                        }
                        isExportingLog = false
                    },
                    enabled = !isExportingLog,
                ) {
                    if (isExportingLog) {
                        CircularProgressIndicator(modifier = Modifier.size(16.dp), strokeWidth = 2.dp)
                    } else {
                        Icon(Icons.Filled.IosShare, contentDescription = null, modifier = Modifier.size(16.dp))
                        Spacer(Modifier.width(4.dp))
                        Text("Download Log")
                    }
                }
            }

            logExportError?.let { Text(it, fontSize = 12.sp, color = Color.Red) }

            val logState = rememberLazyListState()
            LaunchedEffect(state.logLines.size) {
                if (state.logLines.isNotEmpty()) logState.animateScrollToItem(state.logLines.size - 1)
            }
            Box(
                modifier = Modifier
                    .fillMaxWidth()
                    .height(200.dp)
                    .clip(RoundedCornerShape(8.dp))
                    .background(MaterialTheme.colorScheme.surfaceVariant)
                    .padding(8.dp),
            ) {
                LazyColumn(state = logState, verticalArrangement = Arrangement.spacedBy(2.dp)) {
                    items(state.logLines) { line ->
                        Text(
                            line,
                            fontFamily = FontFamily.Monospace,
                            fontSize = 12.sp,
                            color = when {
                                line.contains("[PASS]") -> Color(0xFF4CAF50)
                                line.contains("[FAIL]") -> Color.Red
                                else -> MaterialTheme.colorScheme.onSurface
                            },
                        )
                    }
                }
            }
        }
    }
}

/**
 * Maps Android lifecycle and window-focus callbacks to SwiftUI scene-active
 * transitions without emitting twice for a normal Activity pause/resume.
 */
internal class ForegroundEchoTrigger {
    private var pausedSinceLastResume = false
    private var lostFocusWhileResumed = false

    fun onPause() {
        pausedSinceLastResume = true
        lostFocusWhileResumed = false
    }

    fun onResume(): Boolean {
        val shouldRequestEcho = pausedSinceLastResume
        pausedSinceLastResume = false
        lostFocusWhileResumed = false
        return shouldRequestEcho
    }

    fun onWindowFocusChanged(
        hasFocus: Boolean,
        isResumed: Boolean,
    ): Boolean {
        if (!hasFocus) {
            if (isResumed && !pausedSinceLastResume) {
                lostFocusWhileResumed = true
            }
            return false
        }

        val shouldRequestEcho =
            isResumed && !pausedSinceLastResume && lostFocusWhileResumed
        lostFocusWhileResumed = false
        return shouldRequestEcho
    }
}

private fun receivedDisplayText(line: String): String =
    if (line.startsWith("received:")) line else "received: $line"

private fun webRtcHookStatusName(status: io.actrium.actr.dsl.WebRtcPeerStatus): String =
    when (status) {
        io.actrium.actr.dsl.WebRtcPeerStatus.IDLE -> "idle"
        io.actrium.actr.dsl.WebRtcPeerStatus.CONNECTING -> "connecting"
        io.actrium.actr.dsl.WebRtcPeerStatus.CONNECTED -> "connected"
        io.actrium.actr.dsl.WebRtcPeerStatus.RECOVERING -> "recovering"
    }

private fun shareLogFile(context: android.content.Context, file: File) {
    val uri = FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
    val intent = Intent(Intent.ACTION_SEND).apply {
        type = "text/plain"
        putExtra(Intent.EXTRA_STREAM, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    context.startActivity(Intent.createChooser(intent, "Share log"))
}
