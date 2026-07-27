/**
 * CdpTeardownGate.h
 *
 * Teardown gate between the Hermes CDP plumbing and the runtime owner's
 * dispose path (issue #40, bug B1).
 *
 * Problem: CDPAgent::handleCommand routes through RuntimeTaskRunner
 * (TaskQueues::All), whose triggerInterrupt_TS leg dereferences the
 * AsyncDebuggerAPI and the HermesRuntime AT ENQUEUE TIME. A liveness token
 * checked inside the queued closure therefore cannot protect a dispatch that
 * races the runtime's destruction: the racing thread touches freed memory
 * before the closure ever runs. The same applies to the disconnect
 * force-resume (also triggerInterrupt_TS) and to the pump task itself
 * (task(*rt)).
 *
 * Contract:
 *   - Every path that can reach into the runtime / AsyncDebuggerAPI / a
 *     CDPAgent from outside the dispose path opens a Section first and drops
 *     it when done. A Section that fails to open means teardown has begun:
 *     back off without touching anything.
 *     A Section wraps only the SYNCHRONOUS enqueue window (handleCommand /
 *     CDPAgent::create / the force-resume interrupt), which is what
 *     triggerInterrupt_TS dereferences the runtime inside. The asynchronous
 *     runtime-task pump does NOT take a Section: a task that parks at a
 *     breakpoint would hold it across the pause, and after dispose() closes the
 *     gate the CDP resume that ends the pause is itself refused here, so
 *     closeAndWait() would hang. The "resume before dispose" order is enforced
 *     upstream instead (the target owner force-resumes every paused connection
 *     before disposing), and the pump guards itself with a separate liveness
 *     token.
 *   - dispose() calls closeAndWait() exactly once BEFORE destroying the
 *     CDPDebugAPI and the runtime. When it returns, no Section is open and
 *     none can open again, so the destruction below it cannot race anyone.
 *   - closeAndWait() must NOT be called from inside an open Section on the
 *     same thread (self-deadlock). The dispose paths never run nested inside
 *     CDP-driven guest execution, so this cannot happen in practice.
 *
 * Whole file is dev-only + WIP-gated, like the rest of the CDP relay.
 */
#pragma once

#if defined(RILL_WIP_CDP_DEVTOOLS) && !defined(NDEBUG)

#include <condition_variable>
#include <mutex>

namespace rill::devtools {

class CdpTeardownGate {
public:
  CdpTeardownGate() = default;
  CdpTeardownGate(const CdpTeardownGate&) = delete;
  CdpTeardownGate& operator=(const CdpTeardownGate&) = delete;

  /// Owner's dispose path only. Blocks new Sections, then waits until every
  /// open Section has exited. Idempotent.
  void closeAndWait() {
    std::unique_lock<std::mutex> lock(mutex_);
    closed_ = true;
    cv_.wait(lock, [this] { return active_ == 0; });
  }

  bool isClosed() {
    std::lock_guard<std::mutex> lock(mutex_);
    return closed_;
  }

  /// RAII guarded section. `if (!section) return;` — a failed open means the
  /// gate is closed (teardown in progress or done).
  class Section {
  public:
    explicit Section(CdpTeardownGate& gate) : gate_(gate), entered_(gate.enter()) {}
    ~Section() {
      if (entered_) gate_.exit();
    }
    Section(const Section&) = delete;
    Section& operator=(const Section&) = delete;
    explicit operator bool() const { return entered_; }

  private:
    CdpTeardownGate& gate_;
    const bool entered_;
  };

private:
  bool enter() {
    std::lock_guard<std::mutex> lock(mutex_);
    if (closed_) return false;
    ++active_;
    return true;
  }
  void exit() {
    std::lock_guard<std::mutex> lock(mutex_);
    if (--active_ == 0 && closed_) cv_.notify_all();
  }

  std::mutex mutex_;
  std::condition_variable cv_;
  int active_ = 0;
  bool closed_ = false;
};

}  // namespace rill::devtools

#endif  // RILL_WIP_CDP_DEVTOOLS && !NDEBUG
