// Native integration e2e for the Hermes CDP relay.
//
// Wires a REAL facebook::hermes::HermesRuntime + CDPDebugAPI to our production
// keel::devtools::CDPAgentTarget (the same class the pod ships) and drives it
// with raw CDP messages, exactly as CDPServer would. It proves the runtime
// behaviour that syntax checks cannot:
//   1. Debugger.enable / Runtime.enable round-trip responses through the
//      per-connection sink.
//   2. A `debugger;` statement pauses the runtime thread and a Debugger.paused
//      event is delivered out-of-band through that sink.
//   3. Debugger.resume, fed through CDPAgentTarget::dispatch() from another
//      thread, unblocks the paused runtime so the eval finishes (side effect
//      __x === 42 becomes observable).
//   4. onClientDisconnect tears the agent down without deadlock.
//
// The task pump here stands in for the RN CallInvoker: it delivers Hermes
// runtime tasks onto the one thread that owns the runtime.
//
// This test links against a Hermes built WITH the debugger (CDP symbols), so it
// is not part of the portable native/core suite. Build + run with build-run.sh,
// pointing HERMES_DESTROOT at a React Native Hermes pod's `destroot` directory
// (the one that ships hermes.framework and include/hermes/cdp/*).

#include <hermes/AsyncDebuggerAPI.h>
#include <hermes/Public/RuntimeConfig.h>
#include <hermes/cdp/CDPAgent.h>
#include <hermes/cdp/CDPDebugAPI.h>
#include <hermes/hermes.h>

#include <jsi/jsi.h>

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <deque>
#include <functional>
#include <future>
#include <iostream>
#include <mutex>
#include <string>
#include <thread>
#include <utility>
#include <vector>

// Activate the gated body of the production target.
#define KEEL_WIP_CDP_DEVTOOLS 1
#include "CDPAgentTarget.h"

using namespace facebook;
namespace hcdp = facebook::hermes::cdp;
namespace hdbg = facebook::hermes::debugger;

namespace {

int g_failures = 0;
void check(bool ok, const std::string& what) {
  std::cout << (ok ? "  PASS  " : "  FAIL  ") << what << "\n";
  if (!ok) ++g_failures;
}

// Runtime-thread task pump (stands in for the host CallInvoker).
struct TaskPump {
  std::mutex m;
  std::condition_variable cv;
  std::deque<std::function<void()>> q;
  bool stop = false;

  void push(std::function<void()> f) {
    {
      std::lock_guard<std::mutex> lk(m);
      q.push_back(std::move(f));
    }
    cv.notify_all();
  }
  void requestStop() {
    {
      std::lock_guard<std::mutex> lk(m);
      stop = true;
    }
    cv.notify_all();
  }
};

// Thread-safe collector for messages emitted on the connection sink.
struct Sink {
  std::mutex m;
  std::condition_variable cv;
  std::string all;

  void operator()(const std::string& msg) {
    std::lock_guard<std::mutex> lk(m);
    all += msg;
    all += '\n';
    cv.notify_all();
  }
  // Wait until `needle` has appeared, or fail after a hang-guard deadline.
  bool waitFor(const std::string& needle, const char* label) {
    std::unique_lock<std::mutex> lk(m);
    bool ok = cv.wait_for(lk, std::chrono::seconds(10),
                          [&] { return all.find(needle) != std::string::npos; });
    if (!ok) {
      std::cout << "  (timeout waiting for " << label << ")\n";
    }
    return ok;
  }
};

// A runtime thread that owns a HermesRuntime + CDPDebugAPI and drains runtime
// tasks, mirroring main()'s setup so each adversarial scenario is self-contained.
struct Fixture {
  TaskPump pump;
  std::thread thread;
  facebook::hermes::HermesRuntime* rt = nullptr;
  std::shared_ptr<hcdp::CDPDebugAPI> debugAPI;
  std::weak_ptr<hcdp::CDPDebugAPI> debugAPIWeak;  // to observe destruction
  std::shared_ptr<keel::devtools::CdpTeardownGate> gate =
      std::make_shared<keel::devtools::CdpTeardownGate>();
  std::shared_ptr<std::atomic<int>> clientsAttached =
      std::make_shared<std::atomic<int>>(0);

  Fixture() {
    std::promise<std::pair<facebook::hermes::HermesRuntime*,
                           std::shared_ptr<hcdp::CDPDebugAPI>>>
        ready;
    auto fut = ready.get_future();
    thread = std::thread([&] {
      auto runtime =
          facebook::hermes::makeHermesRuntime(::hermes::vm::RuntimeConfig());
      auto* rtPtr = runtime.get();
      std::shared_ptr<hcdp::CDPDebugAPI> api = hcdp::CDPDebugAPI::create(*runtime);
      ready.set_value({rtPtr, api});
      for (;;) {
        std::function<void()> task;
        {
          std::unique_lock<std::mutex> lk(pump.m);
          pump.cv.wait(lk, [&] { return !pump.q.empty() || pump.stop; });
          if (pump.q.empty() && pump.stop) break;
          task = std::move(pump.q.front());
          pump.q.pop_front();
        }
        task();
      }
      api.reset();
      runtime.reset();
    });
    auto pair = fut.get();
    rt = pair.first;
    debugAPI = pair.second;
    debugAPIWeak = debugAPI;
  }

  hdbg::EnqueueRuntimeTaskFunc enqueue() {
    auto* rtPtr = rt;
    return [this, rtPtr](hdbg::RuntimeTask t) {
      pump.push([rtPtr, t = std::move(t)]() { t(*rtPtr); });
    };
  }

  void stop() {
    pump.requestStop();
    if (thread.joinable()) thread.join();
  }
};

// A: disconnect while the runtime is paused at a breakpoint. onClientDisconnect
// must force-resume the paused runtime (via the interrupt) so the eval finishes
// and teardown does not deadlock — even though no CDP client is left to resume.
void test_disconnect_while_paused() {
  std::cout << "--- disconnect while paused ---\n";
  Fixture fx;
  Sink sink;
  auto target = std::make_shared<keel::devtools::CDPAgentTarget>(
      1, fx.debugAPI, fx.enqueue(), fx.gate, fx.clientsAttached);
  const keel::devtools::ConnectionId conn = 1;
  target->onClientConnect(conn, [&sink](const std::string& m) { sink(m); });
  target->dispatch(conn, R"({"id":1,"method":"Debugger.enable"})");
  check(sink.waitFor("\"id\":1", "enable ack"), "disconnect-paused: enable acked");

  std::promise<void> evalDone;
  auto evalFut = evalDone.get_future();
  fx.pump.push([&] {
    try {
      fx.rt->evaluateJavaScript(
          std::make_shared<jsi::StringBuffer>("debugger; globalThis.__y = 7;"),
          "a.js");
    } catch (const std::exception&) {
    }
    evalDone.set_value();
  });
  check(sink.waitFor("\"Debugger.paused\"", "paused"),
        "disconnect-paused: runtime paused at debugger;");

  // Disconnect while paused: the force-resume interrupt must unblock the eval.
  target->onClientDisconnect(conn);
  bool resumed =
      evalFut.wait_for(std::chrono::seconds(10)) == std::future_status::ready;
  check(resumed, "disconnect-paused: onClientDisconnect force-resumed the runtime");

  fx.gate->closeAndWait();
  target->detachAllAgentsForTeardown();
  target.reset();
  fx.debugAPI.reset();
  fx.stop();
  check(true, "disconnect-paused: teardown without deadlock");
}

// B: many dispatch() calls racing a gate close (the sandbox dispose path). The
// gate must serialize so closeAndWait() returns only after every in-flight
// dispatch section has exited, and dispatches after the close are dropped, not
// crashed. Correctness is under ASan (build-run.sh -DKEEL_ASAN=1); here we
// assert progress + the post-close drop.
void test_dispatch_vs_teardown_race() {
  std::cout << "--- dispatch vs teardown race ---\n";
  Fixture fx;
  Sink sink;
  auto target = std::make_shared<keel::devtools::CDPAgentTarget>(
      1, fx.debugAPI, fx.enqueue(), fx.gate, fx.clientsAttached);
  const keel::devtools::ConnectionId conn = 1;
  target->onClientConnect(conn, [&sink](const std::string& m) { sink(m); });
  target->dispatch(conn, R"({"id":1,"method":"Debugger.enable"})");
  check(sink.waitFor("\"id\":1", "enable ack"), "race: enable acked");

  std::atomic<int> dispatched{0};
  std::atomic<bool> go{false};
  std::vector<std::thread> hammer;
  for (int i = 0; i < 4; ++i) {
    hammer.emplace_back([&] {
      while (!go.load()) {
      }
      for (int k = 0; k < 200; ++k) {
        target->dispatch(conn, R"({"id":9,"method":"Runtime.enable"})");
        dispatched.fetch_add(1);
      }
    });
  }
  go.store(true);
  // Close the gate concurrently with the hammering (stands in for dispose()).
  fx.gate->closeAndWait();
  for (auto& t : hammer) t.join();
  check(dispatched.load() > 0, "race: dispatches ran before/around the close");

  // After the gate is closed every further dispatch is refused at the section.
  size_t before = 0;
  {
    std::lock_guard<std::mutex> lk(sink.m);
    before = sink.all.size();
  }
  target->dispatch(conn, R"({"id":123,"method":"Runtime.enable"})");
  std::this_thread::yield();
  size_t after = 0;
  {
    std::lock_guard<std::mutex> lk(sink.m);
    after = sink.all.size();
  }
  check(after == before, "race: dispatch after gate close is dropped (no output)");

  target->detachAllAgentsForTeardown();
  target.reset();
  fx.debugAPI.reset();
  fx.stop();
  check(true, "race: teardown without deadlock");
}

// C: destroy a tenant whose disconnect enqueued a force-resume interrupt that
// NEVER fires. The runtime is IDLE (not paused, not running JS) when the client
// disconnects, so the queued interrupt sits unprocessed. The B2 bug: the
// interrupt closure captured the CDPDebugAPI by shared_ptr and was stored in
// the AsyncDebuggerAPI queue that same CDPDebugAPI owns -> a self-cycle whose
// refcount never reaches zero, so the CDPDebugAPI leaks and its destruction
// (which must precede the runtime's) silently never happens. With the weak
// capture there is no cycle, so dropping every external ref destructs it.
void test_destroy_with_pending_interrupt() {
  std::cout << "--- destroy with pending interrupt ---\n";
  Fixture fx;
  Sink sink;
  auto target = std::make_shared<keel::devtools::CDPAgentTarget>(
      1, fx.debugAPI, fx.enqueue(), fx.gate, fx.clientsAttached);
  const keel::devtools::ConnectionId conn = 1;
  target->onClientConnect(conn, [&sink](const std::string& m) { sink(m); });
  target->dispatch(conn, R"({"id":1,"method":"Debugger.enable"})");
  check(sink.waitFor("\"id\":1", "enable ack"), "pending-int: enable acked");

  // Runtime is idle here (enable was processed; no JS running, not paused).
  // Disconnect enqueues the force-resume interrupt; nothing will ever run JS to
  // fire it.
  target->onClientDisconnect(conn);

  // Production teardown order: close the gate, sweep, drop every external
  // CDPDebugAPI reference, then stop the (idle) runtime thread so it drops its
  // own reference. The interrupt is still queued and unfired the whole time.
  fx.gate->closeAndWait();
  target->detachAllAgentsForTeardown();
  target.reset();
  fx.debugAPI.reset();  // drop our ref
  fx.stop();            // idle thread: queue empty + stop -> api.reset(); runtime.reset()

  // With the bug's strong capture, the queued-but-never-fired interrupt still
  // holds a CDPDebugAPI ref through the self-cycle, so the weak_ptr stays valid
  // (leak). With the weak capture the cycle is broken and it must have expired.
  check(fx.debugAPIWeak.expired(),
        "pending-int: CDPDebugAPI destructed (no interrupt-cycle leak)");
}

}  // namespace

int main() {
  std::cout << std::unitbuf;  // flush each line so a crash doesn't swallow output
  std::cout << "=== Hermes CDP relay native e2e ===\n";

  TaskPump pump;
  std::promise<
      std::pair<facebook::hermes::HermesRuntime*, std::shared_ptr<hcdp::CDPDebugAPI>>>
      ready;
  auto readyFut = ready.get_future();

  // Runtime thread: owns the runtime + CDPDebugAPI and runs all runtime tasks.
  std::thread runtimeThread([&] {
    auto runtime = facebook::hermes::makeHermesRuntime(::hermes::vm::RuntimeConfig());
    facebook::hermes::HermesRuntime* rtPtr = runtime.get();
    std::shared_ptr<hcdp::CDPDebugAPI> debugAPI =
        hcdp::CDPDebugAPI::create(*runtime);
    ready.set_value({rtPtr, debugAPI});

    for (;;) {
      std::function<void()> task;
      {
        std::unique_lock<std::mutex> lk(pump.m);
        pump.cv.wait(lk, [&] { return !pump.q.empty() || pump.stop; });
        if (pump.q.empty() && pump.stop) break;
        task = std::move(pump.q.front());
        pump.q.pop_front();
      }
      task();  // a paused eval blocks HERE; resume arrives via Hermes interrupt
    }

    // Destroy CDPDebugAPI before the runtime, on the runtime thread.
    debugAPI.reset();
    runtime.reset();
  });

  // Plain locals (not structured bindings) so the task lambdas can capture them
  // under C++17 without the C++20 structured-binding-capture extension.
  auto readyPair = readyFut.get();
  facebook::hermes::HermesRuntime* rtPtr = readyPair.first;
  std::shared_ptr<hcdp::CDPDebugAPI> debugAPI = readyPair.second;

  // enqueue: deliver each Hermes runtime task onto the runtime thread.
  hdbg::EnqueueRuntimeTaskFunc enqueue = [&pump, rtPtr](hdbg::RuntimeTask t) {
    pump.push([rtPtr, t = std::move(t)]() { t(*rtPtr); });
  };

  Sink sink;
  auto gate = std::make_shared<keel::devtools::CdpTeardownGate>();
  auto clientsAttached = std::make_shared<std::atomic<int>>(0);
  auto target = std::make_shared<keel::devtools::CDPAgentTarget>(
      /*execCtxId=*/1, debugAPI, std::move(enqueue), gate, clientsAttached);

  const keel::devtools::ConnectionId conn = 1;
  target->onClientConnect(conn, [&sink](const std::string& m) { sink(m); });

  // 1. Enable domains and confirm responses come back through the sink.
  target->dispatch(conn, R"({"id":1,"method":"Debugger.enable"})");
  check(sink.waitFor("\"id\":1", "Debugger.enable response"),
        "Debugger.enable acknowledged via sink");
  target->dispatch(conn, R"({"id":2,"method":"Runtime.enable"})");
  check(sink.waitFor("\"id\":2", "Runtime.enable response"),
        "Runtime.enable acknowledged via sink");

  // 2. Evaluate JS that hits `debugger;` -> the runtime thread pauses inside it.
  std::promise<void> evalDone;
  auto evalFut = evalDone.get_future();
  pump.push([rtPtr, &evalDone] {
    try {
      rtPtr->evaluateJavaScript(
          std::make_shared<jsi::StringBuffer>("debugger; globalThis.__x = 42;"),
          "e2e.js");
    } catch (const std::exception& e) {
      std::cout << "  (eval threw: " << e.what() << ")\n";
    }
    evalDone.set_value();
  });

  check(sink.waitFor("\"Debugger.paused\"", "Debugger.paused event"),
        "debugger; statement paused and reported Debugger.paused out-of-band");

  // eval must still be blocked (paused) at this point.
  bool blockedWhilePaused =
      evalFut.wait_for(std::chrono::milliseconds(200)) != std::future_status::ready;
  check(blockedWhilePaused, "runtime thread stays blocked while paused");

  // 3. Resume through dispatch() from this (non-runtime) thread.
  target->dispatch(conn, R"({"id":3,"method":"Debugger.resume"})");
  bool resumed =
      evalFut.wait_for(std::chrono::seconds(10)) == std::future_status::ready;
  check(resumed, "Debugger.resume unblocked the paused runtime thread");

  // 4. Confirm the post-resume side effect executed.
  std::promise<int> xVal;
  auto xFut = xVal.get_future();
  pump.push([rtPtr, &xVal] {
    auto v = rtPtr->global().getProperty(*rtPtr, "__x");
    xVal.set_value(v.isNumber() ? static_cast<int>(v.getNumber()) : -1);
  });
  int x = xFut.wait_for(std::chrono::seconds(5)) == std::future_status::ready
              ? xFut.get()
              : -1;
  check(x == 42, "post-resume side effect observed (__x === 42)");

  // 5. Teardown in production order: disconnect, close the gate + sweep (as the
  //    sandbox dispose() does), then drop the target and stop the thread.
  target->onClientDisconnect(conn);
  gate->closeAndWait();
  target->detachAllAgentsForTeardown();  // empty after the disconnect above
  target.reset();  // release our CDPDebugAPI ref before the runtime thread exits
  // Drop BOTH main-thread CDPDebugAPI refs (the named copy AND the one still
  // held inside readyPair) before joining, so the runtime thread owns the last
  // reference: ~CDPDebugAPI then runs on the runtime thread, before it resets
  // the runtime. Otherwise readyPair keeps the CDPDebugAPI alive until main
  // exits and its destructor touches the already-freed runtime (a latent UAF
  // in this harness).
  debugAPI.reset();
  readyPair.second.reset();
  pump.requestStop();
  runtimeThread.join();
  check(true, "clean teardown without deadlock");

  // Additional adversarial teardown scenarios (issue #40).
  test_disconnect_while_paused();
  test_dispatch_vs_teardown_race();
  test_destroy_with_pending_interrupt();

  std::cout << "=== " << (g_failures == 0 ? "ALL PASS" : "FAILURES")
            << " (" << g_failures << " failed) ===\n";
  return g_failures == 0 ? 0 : 1;
}
