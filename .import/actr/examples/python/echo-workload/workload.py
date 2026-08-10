# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import time

from actr_workload import ActrContext, Workload as WorkloadProtocol

from generated.actr_dispatch import actr_dispatch
from generated.local import echo_pb2 as pb2


class EchoServiceHandler:
    async def echo_service_echo(
        self, req: pb2.EchoRequest, ctx: ActrContext
    ) -> pb2.EchoResponse:
        return pb2.EchoResponse(
            reply=f"echo from python: {req.message}",
            timestamp=int(time.time()),
        )


class Workload(WorkloadProtocol):
    async def dispatch(self, envelope, ctx) -> bytes:
        return await actr_dispatch(
            ctx, EchoServiceHandler(), envelope.route_key, envelope.payload, 30_000
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

    async def on_credential_renewed(self, event, ctx) -> None:
        return None

    async def on_credential_expiring(self, event, ctx) -> None:
        return None

    async def on_mailbox_backpressure(self, event, ctx) -> None:
        return None

    async def on_data_chunk(self, chunk, sender, ctx) -> None:
        return None


__all__ = ["Workload"]
