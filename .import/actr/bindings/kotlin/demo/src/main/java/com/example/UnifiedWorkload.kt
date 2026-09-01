/**
 * Unified Workload for all services (linked mode)
 *
 * This Workload handles all service requests using the generated `actrDispatch`
 * function with exact route-key matching. Local requests are routed to your
 * ActrDispatchHandler implementation. Remote requests are resolved via
 * dependency alias, discovered, and called raw.
 */
package com.example

import android.util.Log
import com.example.generated.ActrDispatchHandler
import com.example.generated.actrDispatch
import io.actrium.actr.dsl.ActrContext
import io.actrium.actr.dsl.ErrorEvent
import io.actrium.actr.dsl.RpcEnvelope

/**
 * Unified Workload lifecycle scaffold
 *
 * This handles dispatch and lifecycle-like callbacks for the linked Android client.
 * UnifiedLifecycleAdapter wraps it for the SDK-facing lifecycle bridge.
 *
 * Usage:
 * ```kotlin
 * val handler = MyUnifiedHandler()
 * val workload = UnifiedWorkload(handler)
 * val lifecycle = UnifiedLifecycleAdapter(workload)
 * val dynamicWorkload = lifecycle.toDynamicWorkload()
 * ```
 */
class UnifiedWorkload(
    private val handler: ActrDispatchHandler,
) {
    companion object {
        private const val TAG = "UnifiedWorkload"
    }

    suspend fun onStart(ctx: ActrContext) {
        Log.i(TAG, "UnifiedWorkload.onStart")
    }

    suspend fun onReady(ctx: ActrContext) {
        Log.i(TAG, "UnifiedWorkload.onReady")
    }

    suspend fun onStop(ctx: ActrContext) {
        Log.i(TAG, "UnifiedWorkload.onStop")
    }

    suspend fun onError(
        ctx: ActrContext,
        event: ErrorEvent,
    ) {
        Log.e(TAG, "UnifiedWorkload.onError: $event")
    }

    /**
     * Dispatch RPC requests
     *
     * Uses the generated `actrDispatch` function to route requests with
     * exact route-key matching to local handlers or remote actors.
     */
    suspend fun dispatch(
        ctx: ActrContext,
        envelope: RpcEnvelope,
    ): ByteArray {
        Log.i(TAG, "🔀 dispatch() called")
        Log.i(TAG, "   route_key: ${envelope.routeKey}")
        Log.i(TAG, "   request_id: ${envelope.requestId}")
        Log.i(TAG, "   payload size: ${envelope.payload.size} bytes")

        return actrDispatch(ctx, handler, envelope.routeKey, envelope.payload, 30_000L)
    }
}
