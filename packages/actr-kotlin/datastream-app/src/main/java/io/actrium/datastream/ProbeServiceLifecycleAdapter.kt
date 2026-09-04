package io.actrium.datastream

import io.actrium.actr.dsl.ActrContext
import io.actrium.actr.dsl.ErrorEvent
import io.actrium.actr.dsl.RpcEnvelope
import io.actrium.actr.dsl.Workload

// 1:1 port of ProbeServiceLifecycleAdapter.swift (generic scaffold).
// Wraps a ProbeServiceWorkload<Handler> as a Workload, delegating dispatch to
// the workload. The real adapter used at runtime is ProbeLifecycleAdapter
// (private, in ActrService.kt).
class ProbeServiceLifecycleAdapter<Handler : ProbeServiceHandler>(
    private val workload: ProbeServiceWorkload<Handler>,
) : Workload {
    constructor(handler: Handler) : this(ProbeServiceWorkload(handler))

    override suspend fun onStart(ctx: ActrContext) {
        // Optionally customize startup logic
    }

    override suspend fun onReady(ctx: ActrContext) {
        // Optionally customize ready logic
    }

    override suspend fun onStop(ctx: ActrContext) {
        // Optionally customize shutdown logic
    }

    override suspend fun onError(ctx: ActrContext, event: ErrorEvent) {
        // Optionally customize error handling
    }

    override suspend fun dispatch(ctx: ActrContext, envelope: RpcEnvelope): ByteArray =
        workload.dispatch(ctx, envelope)
}
