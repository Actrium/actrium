/*
 * QuickJSEngineDebugger — implements keel::devtools::IEngineDebugger on top of
 * QuickJSDebugCore, so the shared DebuggerAdapter / AdapterDebugTarget relay can
 * drive the QuickJS engine over CDP (dev-only, gated on KEEL_QJS_DEBUG).
 *
 * M2 scope: enable/disable, line breakpoints, pause/resume, isPaused, and the
 * Debugger.paused notification (a single synthetic call frame). Call frames with
 * real scopes, stepping, and evaluate-on-frame are M3.
 *
 * Line numbers: CDP is 0-based; QuickJS (pc2line) is 1-based. Converted at the
 * boundary here.
 */
#pragma once

#ifdef KEEL_QJS_DEBUG

#include "devtools/DebuggerAdapter.h"  // IEngineDebugger + CDP types (native/core)

#include "QuickJSDebugCore.h"
#include "quickjs.h"  // JSValue (stored by value in the pause object registry)

#include <cstdint>
#include <functional>
#include <mutex>
#include <optional>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

namespace keel::qjs_debug {

class QuickJSEngineDebugger : public keel::devtools::IEngineDebugger {
public:
  // Emits a Debugger.paused. Wired by the assembler to DebuggerAdapter::onPaused.
  using PausedNotifier = std::function<void(
      keel::devtools::PauseReason,
      const std::vector<keel::devtools::CallFrame>&,
      const std::vector<std::string>& hitBreakpoints)>;

  // Emits a Debugger.scriptParsed. Wired by the assembler to
  // DebuggerAdapter::onScriptParsed.
  using ScriptParsedNotifier =
      std::function<void(const keel::devtools::ScriptInfo&)>;

  QuickJSEngineDebugger(QuickJSDebugCore* core, keel::devtools::TenantId tenantId);
  ~QuickJSEngineDebugger() override;

  void setPausedNotifier(PausedNotifier fn);
  void setScriptParsedNotifier(ScriptParsedNotifier fn);

  // IEngineDebugger
  bool enable(keel::devtools::TenantId) override;
  void disable(keel::devtools::TenantId) override;
  std::optional<std::string> setBreakpoint(keel::devtools::TenantId,
                                           const std::string& scriptId,
                                           int lineNumber, int columnNumber,
                                           const std::string& condition) override;
  bool removeBreakpoint(keel::devtools::TenantId,
                        const std::string& breakpointId) override;
  void pause(keel::devtools::TenantId) override;
  void resume(keel::devtools::TenantId) override;
  void step(keel::devtools::TenantId, keel::devtools::StepAction action) override;
  std::string evaluateOnCallFrame(keel::devtools::TenantId,
                                  const std::string& callFrameId,
                                  const std::string& expression) override;
  std::string getProperties(keel::devtools::TenantId,
                            const std::string& objectId) override;
  std::vector<keel::devtools::CallFrame> getCallFrames(keel::devtools::TenantId) override;
  std::vector<keel::devtools::ScriptInfo> getScripts(keel::devtools::TenantId) override;
  std::string getScriptSource(keel::devtools::TenantId,
                              const std::string& scriptId) override;
  bool isPaused(keel::devtools::TenantId) override;

private:
  // Runs on the QuickJS runtime thread when execution pauses.
  void onCorePaused(const std::string& scriptId, int line1Based,
                    PauseReason reason);
  static keel::devtools::PauseReason toCdpReason(PauseReason reason);
  // Runs on the QuickJS runtime thread the first time a script is seen.
  void onScriptSeen(const std::string& scriptId, const std::string& url,
                    const std::string& source);

  // Serialize a JSValue as a CDP Runtime.RemoteObject. Primitives are emitted by
  // value; objects/functions get a pause-scoped objectId (via registerObject) so
  // the client can expand them with getProperties. Runtime-thread-only.
  std::string toRemoteObjectJson(JSContext* ctx, JSValueConst v);
  // Mint an "obj:N" id for a JSValue and retain a dup of it until the pause ends.
  // Runtime-thread-only.
  std::string registerObject(JSContext* ctx, JSValueConst v);
  // Free every retained pause-scoped object. Fired on the runtime thread as the
  // pause tears down (QuickJSDebugCore::onResuming_) — the only thread that may
  // free these JSValues.
  void freePauseObjects(JSContext* ctx);
  // Emit each own enumerable string property of `obj` (capped) as a CDP
  // PropertyDescriptor into `out`. Runtime-thread-only. Shared by the object and
  // global getProperties branches.
  void emitOwnProps(JSContext* ctx, JSValueConst obj,
                    std::vector<std::string>& out);

  QuickJSDebugCore* core_;
  keel::devtools::TenantId tenantId_;
  PausedNotifier notifier_;
  ScriptParsedNotifier scriptNotifier_;

  std::mutex mutex_;
  std::uint64_t nextBreakpointId_ = 1;  // guarded by mutex_
  // engine breakpoint id -> (scriptId, 1-based line)
  std::unordered_map<std::string, std::pair<std::string, int>> breakpoints_;
  std::vector<keel::devtools::CallFrame> lastFrames_;  // last pause; guarded by mutex_
  // Script registry (scriptId == url == filename); guarded by mutex_.
  std::unordered_map<std::string, keel::devtools::ScriptInfo> scripts_;
  std::unordered_map<std::string, std::string> sources_;

  // Pause-scoped object registry: objectId ("obj:N") -> a dup'd JSValue kept
  // alive so the client can expand it after the value left the C++ stack. Touched
  // only on the runtime thread (mint during a paused eval/getProperties job, free
  // at pause exit), so it needs no lock. Every entry is dropped each time the
  // pause ends; ids never survive a resume.
  std::uint64_t nextObjectId_ = 1;
  std::unordered_map<std::string, JSValue> pauseObjects_;
};

}  // namespace keel::qjs_debug

#endif  // KEEL_QJS_DEBUG
