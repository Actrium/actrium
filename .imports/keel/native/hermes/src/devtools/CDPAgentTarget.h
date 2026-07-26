/**
 * CDPAgentTarget.h
 *
 * Hermes passthrough behind the relay seam (IEngineDebugTarget). A CDP-native
 * engine speaks the protocol end-to-end, so this is a thin adapter:
 *   - one facebook::hermes::cdp::CDPDebugAPI per tenant (per HermesRuntime),
 *     created and owned by the sandbox context; referenced weakly in here so
 *     the sandbox's dispose() is the one deterministic destruction point;
 *   - one facebook::hermes::cdp::CDPAgent per DevTools connection.
 *
 * We do NOT write a pause pump: Hermes' AsyncDebuggerAPI blocks the runtime
 * thread in processInterruptWhilePaused() and CDPAgent routes commands through
 * RuntimeTaskRunner (TaskQueues::All) so a resume reaches a paused runtime via
 * the interrupt path. Our only obligation is the integrator enqueue callback
 * (a thin callInvoker adapter, injected) and the per-connection outbound sink.
 *
 * Lifetime (issue #40): every entry point that reaches into the runtime /
 * AsyncDebuggerAPI (dispatch, connect, disconnect — all of which enqueue via
 * triggerInterrupt_TS, which touches the runtime AT ENQUEUE TIME) runs inside
 * a CdpTeardownGate section shared with the sandbox's dispose(). Once dispose
 * closes the gate, late traffic is dropped (debug-logged) instead of touching
 * a destroyed runtime.
 *
 * Whole TU is dev-only + WIP-gated: it pulls in Hermes CDP headers.
 */
#pragma once

#if defined(RILL_WIP_CDP_DEVTOOLS) && !defined(NDEBUG)

#include "devtools/EngineDebugTarget.h"  // IEngineDebugTarget, ConnectionId, CdpOutboundFn

#include "CdpTeardownGate.h"

#include <hermes/RuntimeTaskRunner.h>    // debugger::EnqueueRuntimeTaskFunc
#include <hermes/cdp/CDPAgent.h>         // cdp::CDPAgent, cdp::State, cdp::OutboundMessageFunc
#include <hermes/cdp/CDPDebugAPI.h>      // cdp::CDPDebugAPI

#include <cstdint>
#include <memory>
#include <mutex>
#include <unordered_map>

namespace rill::devtools {

class CDPAgentTarget : public IEngineDebugTarget {
public:
  // `debugAPI` is kept as a weak reference: the sandbox context is the sole
  // owner and destroys it in dispose() (after closing `gate`), which keeps the
  // "CDPDebugAPI dies before the runtime" order deterministic no matter how
  // long the CDPServer keeps this target alive. `gate` must be non-null and
  // must be the same gate that the owner closes in dispose().
  CDPAgentTarget(int32_t executionContextId,
                 std::shared_ptr<facebook::hermes::cdp::CDPDebugAPI> debugAPI,
                 facebook::hermes::debugger::EnqueueRuntimeTaskFunc enqueue,
                 std::shared_ptr<CdpTeardownGate> gate);
  ~CDPAgentTarget() override;

  DomainSet ownedDomains() const override;
  void onClientConnect(ConnectionId conn, CdpOutboundFn persistentSink) override;
  void onClientDisconnect(ConnectionId conn) override;
  void dispatch(ConnectionId conn, const RawCdpMessage& raw) override;

  // Owner's dispose path only, AFTER gate->closeAndWait(): destroys every
  // remaining per-connection CDPAgent while the CDPDebugAPI and the runtime
  // are still alive (~CDPAgent enqueues its cleanup through the
  // AsyncDebuggerAPI, so it must not run after them).
  void detachAllAgentsForTeardown();

private:
  const int32_t execCtxId_;
  // weak, not shared: see the constructor comment. Locking it inside an open
  // gate section always succeeds — the owner resets the strong reference only
  // after closing the gate.
  std::weak_ptr<facebook::hermes::cdp::CDPDebugAPI> debugAPI_;
  facebook::hermes::debugger::EnqueueRuntimeTaskFunc enqueue_;
  const std::shared_ptr<CdpTeardownGate> gate_;

  std::mutex agentsMutex_;
  // shared_ptr, not unique_ptr: dispatch() must call handleCommand() OUTSIDE
  // agentsMutex_ (the CDPServer contract releases its own lock too), while
  // onClientDisconnect() can concurrently remove the agent. The in-flight
  // dispatch keeps its own reference, so the agent is destroyed only after the
  // last user — never mid-handleCommand.
  std::unordered_map<ConnectionId,
                     std::shared_ptr<facebook::hermes::cdp::CDPAgent>> agents_;
};

}  // namespace rill::devtools

#endif  // RILL_WIP_CDP_DEVTOOLS && !NDEBUG
