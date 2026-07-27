#include "CDPAgentTarget.h"

#if defined(RILL_WIP_CDP_DEVTOOLS) && !defined(NDEBUG)

#include <hermes/AsyncDebuggerAPI.h>

#include <string>
#include <utility>

// Dev-only logging for the teardown paths. Late traffic dropped at the closed
// teardown gate is expected during shutdown -> debug level; having to leak a
// CDPAgent to avoid a use-after-free indicates a teardown-ordering bug in the
// caller -> error level.
#if defined(__APPLE__)
#include <os/log.h>
namespace {
void logDebug(const std::string &msg) {
  os_log_with_type(OS_LOG_DEFAULT, OS_LOG_TYPE_DEBUG, "[CDPAgentTarget] %{public}s",
                   msg.c_str());
}
void logError(const std::string &msg) {
  os_log_with_type(OS_LOG_DEFAULT, OS_LOG_TYPE_ERROR, "[CDPAgentTarget] %{public}s",
                   msg.c_str());
}
}  // namespace
#elif defined(__ANDROID__)
#include <android/log.h>
namespace {
void logDebug(const std::string &msg) {
  __android_log_print(ANDROID_LOG_DEBUG, "CDPAgentTarget", "%s", msg.c_str());
}
void logError(const std::string &msg) {
  __android_log_print(ANDROID_LOG_ERROR, "CDPAgentTarget", "%s", msg.c_str());
}
}  // namespace
#else
#include <cstdio>
namespace {
void logDebug(const std::string &msg) {
  fprintf(stderr, "[CDPAgentTarget][debug] %s\n", msg.c_str());
}
void logError(const std::string &msg) {
  fprintf(stderr, "[CDPAgentTarget][error] %s\n", msg.c_str());
}
}  // namespace
#endif

namespace rill::devtools {

namespace hcdp = facebook::hermes::cdp;
namespace hdbg = facebook::hermes::debugger;

CDPAgentTarget::CDPAgentTarget(int32_t executionContextId,
                               std::shared_ptr<hcdp::CDPDebugAPI> debugAPI,
                               hdbg::EnqueueRuntimeTaskFunc enqueue,
                               std::shared_ptr<CdpTeardownGate> gate,
                               std::shared_ptr<std::atomic<int>> clientsAttached)
    : execCtxId_(executionContextId),
      debugAPI_(debugAPI),
      enqueue_(std::move(enqueue)),
      gate_(std::move(gate)),
      clientsAttached_(std::move(clientsAttached)) {}

CDPAgentTarget::~CDPAgentTarget() {
  // Normal shutdown empties agents_ before this runs: the server disconnects
  // every bound connection (onClientDisconnect) and the sandbox's dispose()
  // sweeps survivors via detachAllAgentsForTeardown(). If anything is still
  // here, destroying it is only safe while the runtime is alive — ~CDPAgent
  // enqueues cleanup through the AsyncDebuggerAPI — so take the gate like any
  // other user. If the gate is already closed the debug API / runtime are (or
  // are about to be) gone: deliberately leak the agents instead of
  // dereferencing freed memory. Dev-only code; loud error either way, because
  // reaching this branch means a caller broke the teardown protocol.
  std::unordered_map<ConnectionId, std::shared_ptr<hcdp::CDPAgent>> doomed;
  {
    std::lock_guard<std::mutex> lock(agentsMutex_);
    doomed.swap(agents_);
  }
  if (doomed.empty()) return;

  CdpTeardownGate::Section gate(*gate_);
  if (gate && !debugAPI_.expired()) {
    logError("destroying " + std::to_string(doomed.size()) +
             " CDPAgent(s) left behind at ~CDPAgentTarget (missed disconnect?)");
    doomed.clear();  // inside the section: runtime still alive
    return;
  }
  logError("leaking " + std::to_string(doomed.size()) +
           " CDPAgent(s): target destroyed after runtime teardown");
  // Intentional leak: ~CDPAgent would enqueue into the destroyed
  // AsyncDebuggerAPI (use-after-free). Leaking is the safe failure mode.
  new std::unordered_map<ConnectionId, std::shared_ptr<hcdp::CDPAgent>>(
      std::move(doomed));
}

DomainSet CDPAgentTarget::ownedDomains() const {
  DomainSet d;
  d.runtime = true;
  d.debugger = true;
  d.profiler = true;  // CDPAgent owns Runtime+Debugger+Profiler; Log/HeapProfiler stay local (MVP)
  return d;
}

void CDPAgentTarget::onClientConnect(ConnectionId conn, CdpOutboundFn persistentSink) {
  // Under the teardown gate: CDPAgent::create dereferences the CDPDebugAPI,
  // and a connect racing dispose() must not build an agent on top of a
  // half-destroyed runtime.
  CdpTeardownGate::Section gate(*gate_);
  auto api = gate ? debugAPI_.lock() : nullptr;
  if (!api) {
    logDebug("dropping client connect after teardown, conn=" + std::to_string(conn));
    return;
  }
  // CdpOutboundFn and cdp::OutboundMessageFunc are both
  // std::function<void(const std::string&)> — pass the sink straight through.
  std::shared_ptr<hcdp::CDPAgent> agent =
      hcdp::CDPAgent::create(execCtxId_, *api, enqueue_,
                             std::move(persistentSink), hcdp::State{});
  std::lock_guard<std::mutex> lock(agentsMutex_);
  agents_[conn] = std::move(agent);
  // A client is now driving the debugger: the sandbox's watchdog callback must
  // stop auto-resuming its pauses (#77).
  clientsAttached_->fetch_add(1, std::memory_order_relaxed);
}

void CDPAgentTarget::dispatch(ConnectionId conn, const RawCdpMessage& raw) {
  // Hold the teardown gate for the WHOLE call (issue #40, B1). handleCommand
  // routes through RuntimeTaskRunner (TaskQueues::All), whose
  // triggerInterrupt_TS leg dereferences the AsyncDebuggerAPI and the runtime
  // AT ENQUEUE TIME — a liveness token inside the queued closure cannot
  // protect that. While this section is open the owner's dispose() blocks in
  // closeAndWait(), so runtime + CDPDebugAPI are guaranteed alive for the
  // whole command. The gate is never held across a blocking wait here:
  // handleCommand only enqueues, it does not wait for execution.
  CdpTeardownGate::Section gate(*gate_);
  if (!gate) {
    logDebug("dropping late dispatch after teardown, conn=" + std::to_string(conn));
    return;
  }
  // Take a strong reference under the map lock, call outside it. A concurrent
  // onClientDisconnect() may erase the map entry while this command is in
  // flight; our reference keeps the agent alive, so the worst case is a
  // command handled for an already-detached client (its sink then targets a
  // connection the server no longer knows — sendToConnection drops it).
  std::shared_ptr<hcdp::CDPAgent> agent;
  {
    std::lock_guard<std::mutex> lock(agentsMutex_);
    auto it = agents_.find(conn);
    if (it != agents_.end()) agent = it->second;
  }
  // handleCommand is safe from any thread; the response and any events arrive
  // later on the runtime thread and go out through this connection's sink.
  if (agent) agent->handleCommand(raw);
  // If the client disconnected mid-command, dropping `agent` here runs
  // ~CDPAgent after the command completed — never mid-use. `agent` is declared
  // after `gate`, so the destruction still happens inside the section
  // (~CDPAgent enqueues through the AsyncDebuggerAPI too).
}

void CDPAgentTarget::onClientDisconnect(ConnectionId conn) {
  // Under the teardown gate: the force-resume below enqueues via
  // triggerInterrupt_TS (touches the runtime at enqueue time), and destroying
  // the agent enqueues its cleanup the same way.
  CdpTeardownGate::Section gate(*gate_);
  if (!gate) {
    // Teardown already closed the gate: detachAllAgentsForTeardown() has swept
    // (or is about to sweep) every remaining agent, and the runtime may
    // already be gone — nothing to do and nothing safe to touch.
    logDebug("dropping late client disconnect after teardown, conn=" +
             std::to_string(conn));
    return;
  }
  std::shared_ptr<hcdp::CDPAgent> dead;
  {
    std::lock_guard<std::mutex> lock(agentsMutex_);
    auto it = agents_.find(conn);
    if (it == agents_.end()) return;
    dead = std::move(it->second);
    agents_.erase(it);
    // Last driver for this connection is gone; if it was the last client the
    // sandbox's watchdog callback resumes future pauses itself (#77).
    clientsAttached_->fetch_sub(1, std::memory_order_relaxed);
  }
  // If this client left the runtime paused at a breakpoint, no one else will
  // resume it and the host thread stays frozen. Force a resume from the
  // runtime thread via a thread-safe interrupt before destroying the agent.
  // Guard with isWaitingForCommand() (NOT isPaused()): resumeFromPaused is
  // only valid when the next command is expected.
  //
  // The closure captures the CDPDebugAPI WEAKLY (issue #40, B2). A strong
  // capture would store a shared_ptr inside the interrupt queue that the SAME
  // CDPDebugAPI owns; if the runtime never runs again (tenant destroyed right
  // after this disconnect) the interrupt never fires and the reference cycle
  // leaks the CDPDebugAPI — silently breaking the "debug API dies before the
  // runtime" ordering. With a weak capture a queued-but-never-run interrupt
  // pins nothing, and one drained during ~CDPDebugAPI fails the lock() and
  // backs off.
  if (auto api = debugAPI_.lock()) {
    std::weak_ptr<hcdp::CDPDebugAPI> weakApi = api;
    api->asyncDebuggerAPI().triggerInterrupt_TS(
        [weakApi](facebook::hermes::HermesRuntime&) {
          auto keep = weakApi.lock();
          if (!keep) return;
          auto& dbg = keep->asyncDebuggerAPI();
          if (dbg.isWaitingForCommand()) {
            dbg.resumeFromPaused(hdbg::AsyncDebugCommand::Continue);
          }
        });
  }
  // Drop our reference inside the gate section. ~CDPAgent runs here — or, if a
  // dispatch() is mid-handleCommand on another thread, when that call returns
  // and releases the last reference (also inside its own section). Either way
  // tasks enqueued during destruction still flow via enqueue_.
  dead.reset();
}

void CDPAgentTarget::detachAllAgentsForTeardown() {
  // Dispose-path only, called AFTER gate_->closeAndWait(): no section is open
  // and none can open, so nothing races this sweep. Runs while the
  // CDPDebugAPI and the runtime are still alive so ~CDPAgent may safely
  // enqueue its cleanup interrupts.
  std::unordered_map<ConnectionId, std::shared_ptr<hcdp::CDPAgent>> doomed;
  {
    std::lock_guard<std::mutex> lock(agentsMutex_);
    doomed.swap(agents_);
  }
  if (!doomed.empty()) {
    logDebug("teardown sweeping " + std::to_string(doomed.size()) +
             " CDPAgent(s) still attached at dispose");
  }
  doomed.clear();
}

}  // namespace rill::devtools

#endif  // RILL_WIP_CDP_DEVTOOLS && !NDEBUG
