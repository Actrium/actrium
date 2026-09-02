# SPDX-License-Identifier: Apache-2.0

"""Concurrency probe workload for the run_concurrent e2e suite (#430).

Mirrors the in-flight probe contract of the Rust fixture
(`crates/actr-hyper/tests/wasm_actor_fixture`):

* `InflightProbe` bumps a per-instance in-flight counter, parks on the
  host-gated `test/gate` crossing, then reports the peak concurrency this
  instance ever observed. MAX_SEEN>=2 across two distinct conflict keys is
  direct proof they interleaved inside ONE Python guest instance.
* `BoomAfterAwait` parks on the `test/double_impl` crossing and then raises,
  fault-injecting a guest trap with siblings in flight.

The counters are MODULE-LEVEL globals, not handler attributes: the scaffold
instantiates a fresh handler per dispatch, and in a Python wasm component
module globals live in the linear memory — so a rebuilt instance (after a
trap) starts from zero, exactly like the Rust fixture's per-instance fields.
"""

from __future__ import annotations

import struct

from actr_workload import ActrContext, Workload as WorkloadProtocol

from generated.actr_dispatch import actr_dispatch
from generated.local import concurrency_pb2 as pb2

_in_flight = 0
_max_seen = 0


class ProbeServiceHandler:
    async def probe_service_inflight_probe(
        self, req: pb2.ProbeRequest, ctx: ActrContext
    ) -> pb2.ProbeResponse:
        global _in_flight, _max_seen
        _in_flight += 1
        _max_seen = max(_max_seen, _in_flight)
        try:
            await ctx.call_raw(
                ctx.self_id,
                "test/gate",
                struct.pack("<I", req.caller_serial),
                30_000,
            )
        finally:
            _in_flight -= 1
        return pb2.ProbeResponse(max_seen=_max_seen)

    async def probe_service_boom_after_await(
        self, req: pb2.BoomRequest, ctx: ActrContext
    ) -> pb2.BoomResponse:
        reply = await ctx.call_raw(
            ctx.self_id, "test/double_impl", struct.pack("<i", 21), 30_000
        )
        if struct.unpack("<i", reply)[0] != 42:
            raise RuntimeError("double_impl gate returned an unexpected value")
        raise RuntimeError("boom after await")


class Workload(WorkloadProtocol):
    async def dispatch(self, envelope, ctx) -> bytes:
        return await actr_dispatch(
            ctx, ProbeServiceHandler(), envelope.route_key, envelope.payload, 30_000
        )

    async def on_start(self, ctx) -> None:
        return None

    async def on_ready(self, ctx) -> None:
        return None

    async def on_stop(self, ctx) -> None:
        return None

    async def on_error(self, event, ctx) -> None:
        return None

    async def on_signaling_connecting(self, ctx) -> None:
        return None

    async def on_signaling_connected(self, ctx) -> None:
        return None

    async def on_signaling_disconnected(self, ctx) -> None:
        return None

    async def on_websocket_connecting(self, event, ctx) -> None:
        return None

    async def on_websocket_connected(self, event, ctx) -> None:
        return None

    async def on_websocket_disconnected(self, event, ctx) -> None:
        return None

    async def on_webrtc_connecting(self, event, ctx) -> None:
        return None

    async def on_webrtc_connected(self, event, ctx) -> None:
        return None

    async def on_webrtc_disconnected(self, event, ctx) -> None:
        return None

    async def on_webrtc_state_changed(self, event, ctx) -> None:
        return None

    async def on_credential_renewed(self, event, ctx) -> None:
        return None

    async def on_credential_expiring(self, event, ctx) -> None:
        return None

    async def on_mailbox_backpressure(self, event, ctx) -> None:
        return None

    async def on_data_chunk(self, chunk, sender, ctx) -> None:
        return None


__all__ = ["Workload"]
