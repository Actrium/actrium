package io.actrium.datastream

import io.actrium.actr.ActrException
import io.actrium.actr.dsl.ActrContext
import io.actrium.datastream.proto.StartProbeRequest
import io.actrium.datastream.proto.StartProbeResponse

// ACTR: mutable scaffold. 1:1 port of ProbeServiceHandlerImpl.swift.
// User-owned RPC implementation stub. The REAL handler is ProbeHandlerImpl
// (private, in ActrService.kt) — this scaffold is kept for file parity and is
// not wired into the runtime.
class ProbeServiceHandlerImpl : ProbeServiceHandler {
    override suspend fun startProbe(req: StartProbeRequest, ctx: ActrContext): StartProbeResponse {
        throw ActrException.NotImplemented("StartProbe is not implemented")
    }
}
