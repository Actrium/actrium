/** Canonical workload-context RPC operations. */
package io.actrium.actr.dsl

import io.actrium.actr.ActrId
import io.actrium.actr.PayloadType

/** Perform a typed RPC call with an explicit timeout. */
suspend fun <Req, Resp> ActrContext.call(
    target: ActrId,
    rpc: RpcRequest<Req, Resp>,
    request: Req,
    timeoutMs: Long,
): Resp {
    require(timeoutMs > 0) { "timeoutMs must be > 0, got $timeoutMs" }
    val response = callRaw(
        target,
        rpc.routeKey,
        rpc.payloadType,
        rpc.serializeRequest(request),
        timeoutMs,
    )
    return rpc.deserializeResponse(response)
}

/** Forward raw RPC bytes on the reliable lane with an explicit timeout. */
suspend fun ActrContext.callRaw(
    target: ActrId,
    routeKey: String,
    payload: ByteArray,
    timeoutMs: Long,
): ByteArray {
    require(timeoutMs > 0) { "timeoutMs must be > 0, got $timeoutMs" }
    return callRaw(target, routeKey, PayloadType.RPC_RELIABLE, payload, timeoutMs)
}

/** Send a typed one-way message using its generated route and payload lane. */
suspend fun <Req, Resp> ActrContext.tell(
    target: ActrId,
    rpc: RpcRequest<Req, Resp>,
    message: Req,
) = tellRaw(target, rpc.routeKey, rpc.payloadType, rpc.serializeRequest(message))
