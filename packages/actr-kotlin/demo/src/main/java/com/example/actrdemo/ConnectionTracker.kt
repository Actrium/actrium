package com.example.actrdemo

import io.actrium.actr.ActrId
import io.actrium.actr.dsl.ActrContext
import io.actrium.actr.dsl.PeerEvent
import io.actrium.actr.dsl.SignalingObserver
import io.actrium.actr.dsl.WebRtcObserver
import io.actrium.actr.dsl.WebRtcPeerState
import io.actrium.actr.dsl.WebRtcPeerStatus
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.atomic.AtomicBoolean

/**
 * Tracks ACTR transport observations for the UI.
 *
 * Two independent signals are kept separate:
 *  - [SignalingObserver] drives [signalingReady] (service connection up/down).
 *  - [WebRtcObserver] drives per-peer [webRtcStatus].
 *
 * Hooks are observational only. Ordinary Call, Tell, and StreamReliable sends
 * wait for route recovery inside the framework; this class does not retain a
 * payload, replay a Call from a connected callback, or arm a fallback timer.
 */
class ConnectionTracker {
    val signalingReady = AtomicBoolean(false)

    /** Per-target WebRTC state. An absent entry means Idle (lazy, not created). */
    val webRtcStatus = ConcurrentHashMap<ActrId, WebRtcPeerStatus>()

    /** Invoked on the runtime thread after every transition; hop to Main to refresh UI. */
    var onEvent: ((String) -> Unit)? = null

    fun reset() {
        signalingReady.set(false)
        webRtcStatus.clear()
    }

    val signalingObserver =
        object : SignalingObserver {
            override suspend fun onConnecting(ctx: ActrContext?) {
                signalingReady.set(false)
                emit("signaling: connecting")
            }

            override suspend fun onConnected(ctx: ActrContext?) {
                signalingReady.set(true)
                emit("signaling: connected")
            }

            override suspend fun onDisconnected(ctx: ActrContext) {
                signalingReady.set(false)
                emit("signaling: disconnected")
            }
        }

    val webRtcObserver =
        object : WebRtcObserver {
            override suspend fun onConnecting(
                ctx: ActrContext,
                e: PeerEvent,
            ) {
                webRtcStatus[e.peer] = WebRtcPeerStatus.CONNECTING
                emit("webrtc ${e.peer.serialNumber}: connecting")
            }

            override suspend fun onConnected(
                ctx: ActrContext,
                e: PeerEvent,
            ) {
                webRtcStatus[e.peer] = WebRtcPeerStatus.CONNECTED
                emit("webrtc ${e.peer.serialNumber}: connected (relayed=${e.relayed})")
            }

            override suspend fun onDisconnected(
                ctx: ActrContext,
                e: PeerEvent,
            ) {
                val status = e.status ?: WebRtcPeerStatus.IDLE
                webRtcStatus[e.peer] = status
                emit("webrtc ${e.peer.serialNumber}: ${status.name.lowercase()}")
            }

            override suspend fun onStateChanged(
                ctx: ActrContext,
                state: WebRtcPeerState,
            ) {
                webRtcStatus[state.peer] = state.status
                emit(
                    "webrtc ${state.peer.serialNumber}: state rev=${state.revision} " +
                        "sendable=${state.sendable} path=${state.networkPath}",
                )
            }
        }

    private fun emit(message: String) {
        onEvent?.invoke(message)
    }
}
