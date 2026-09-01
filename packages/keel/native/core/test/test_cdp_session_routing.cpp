/**
 * test_cdp_session_routing.cpp
 *
 * CDPServer session-routing hardening tests:
 *   - a nested params.sessionId must not hijack request routing (top-level only);
 *   - a sessionId is only honored from the connection that owns it;
 *   - path-less connections (Apple nw_ws transport) fall back to the sole
 *     registered debug target in single-tenant mode, and do NOT fall back when
 *     several targets are registered.
 */
#include "test_framework.h"
#include "../src/devtools/CDPServer.h"
#include "../src/devtools/EngineDebugTarget.h"

#include <memory>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

using namespace keel::devtools;
using namespace keel::test;

namespace {

// Transport double: records outbound frames and lets tests inject
// connect/message/disconnect events.
struct MockTransport : public CDPTransport {
  std::vector<std::pair<ConnectionId, std::string>> sent;

  bool start(const std::string&, uint16_t) override { return true; }
  void stop() override {}
  void send(ConnectionId c, const std::string& m) override { sent.push_back({c, m}); }
  void close(ConnectionId) override {}

  void simulateConnect(ConnectionId c, const std::string& path = "") {
    if (onConnect_) onConnect_(c, path);
  }
  void simulateMessage(ConnectionId c, const std::string& m) {
    if (onMessage_) onMessage_(c, m);
  }
  void simulateDisconnect(ConnectionId c) {
    if (onDisconnect_) onDisconnect_(c);
  }

  // All frames the transport delivered to connection `c`, concatenated.
  std::string allSentTo(ConnectionId c) const {
    std::string out;
    for (const auto& [conn, msg] : sent) {
      if (conn == c) out += msg + "\n";
    }
    return out;
  }
};

// Debug target double: owns Runtime+Debugger, records every dispatched request,
// answers each with a recognizable response through the persistent sink.
struct RecordingTarget : public IEngineDebugTarget {
  std::vector<std::string> received;
  std::unordered_map<ConnectionId, CdpOutboundFn> sinks;

  DomainSet ownedDomains() const override {
    DomainSet d;
    d.runtime = true;
    d.debugger = true;
    return d;
  }
  void onClientConnect(ConnectionId c, CdpOutboundFn s) override { sinks[c] = std::move(s); }
  void onClientDisconnect(ConnectionId c) override { sinks.erase(c); }
  void dispatch(ConnectionId c, const RawCdpMessage& req) override {
    received.push_back(req);
    auto it = sinks.find(c);
    if (it == sinks.end()) return;
    auto id = cdp::parseJSONInt(req, "id");
    it->second("{\"id\":" + std::to_string(id ? *id : 0) +
               ",\"result\":{\"from\":\"target\"}}");
  }
};

TestSuite createCDPSessionRoutingTests() {
  TestSuite suite{"CDPSessionRouting", {}};

  // ---- issue #144: nested params.sessionId must not hijack routing ----------

  suite.cases.push_back({"nested params.sessionId does not select a session", []() {
    auto transport = std::make_shared<MockTransport>();
    CDPServerConfig config;
    config.enabled = true;
    config.transport = transport;
    CDPServer server(config);
    server.registerTenant(1, "App");
    server.start();
    transport->simulateConnect(600, "/tenant/1");

    // Target.detachFromTarget carries sessionId INSIDE params. With naive
    // whole-message search this was read as the request's top-level sessionId;
    // the (nonexistent) session then failed the lookup and the request died
    // with SESSION_NOT_FOUND instead of reaching the Target handler.
    transport->simulateMessage(600,
        R"({"id":5,"method":"Target.detachFromTarget","params":{"sessionId":"tenant-9-nonexistent"}})");

    std::string out = transport->allSentTo(600);
    assertTrue(out.find("\"id\":5") != std::string::npos, "got a reply for id 5");
    assertTrue(out.find("-32001") == std::string::npos,
               "nested sessionId did not trigger SESSION_NOT_FOUND");
    assertTrue(out.find("\"result\"") != std::string::npos, "handled by the Target handler");
    server.stop();
  }});

  suite.cases.push_back({"top-level sessionId wins over a nested decoy", []() {
    auto transport = std::make_shared<MockTransport>();
    CDPServerConfig config;
    config.enabled = true;
    config.transport = transport;
    CDPServer server(config);
    server.registerTenant(1, "App");
    server.start();
    transport->simulateConnect(610, "/tenant/1");

    // Attach to get a real session id.
    transport->simulateMessage(610,
        R"({"id":1,"method":"Target.attachToTarget","params":{"targetId":"1"}})");
    std::string sid;
    for (const auto& [conn, msg] : transport->sent) {
      (void)conn;
      if (msg.find("\"id\":1") != std::string::npos && msg.find("\"result\"") != std::string::npos) {
        auto v = cdp::parseJSONString(msg, "sessionId");
        if (v) sid = *v;
      }
    }
    assertTrue(!sid.empty(), "attachToTarget returned a sessionId");

    // params carries a bogus decoy BEFORE the real top-level sessionId. The
    // naive first-occurrence search picked the decoy and failed the lookup.
    transport->sent.clear();
    transport->simulateMessage(610,
        std::string(R"({"id":2,"method":"Target.getTargets","params":{"sessionId":"decoy"},"sessionId":")") +
        sid + "\"}");

    std::string out = transport->allSentTo(610);
    assertTrue(out.find("-32001") == std::string::npos, "decoy did not break the lookup");
    assertTrue(out.find("\"targetInfos\"") != std::string::npos,
               "request routed via the real top-level session");
    server.stop();
  }});

  // ---- issue #144: cross-connection session ownership -----------------------

  suite.cases.push_back({"a sessionId from another connection is rejected", []() {
    auto transport = std::make_shared<MockTransport>();
    auto target = std::make_shared<RecordingTarget>();
    CDPServerConfig config;
    config.enabled = true;
    config.transport = transport;
    CDPServer server(config);
    server.registerTenant(1, "Victim");
    server.registerDebugTarget(1, target);
    server.start();

    // Connection A attaches and owns the session.
    transport->simulateConnect(620, "/tenant/1");
    transport->simulateMessage(620,
        R"({"id":1,"method":"Target.attachToTarget","params":{"targetId":"1"}})");
    std::string sid;
    for (const auto& [conn, msg] : transport->sent) {
      (void)conn;
      if (msg.find("\"id\":1") != std::string::npos && msg.find("\"result\"") != std::string::npos) {
        auto v = cdp::parseJSONString(msg, "sessionId");
        if (v) sid = *v;
      }
    }
    assertTrue(!sid.empty(), "connection A holds a session");

    // Connection B (a different local client) replays A's sessionId.
    transport->simulateConnect(621);
    transport->sent.clear();
    transport->simulateMessage(621,
        std::string(R"({"id":9,"method":"Runtime.evaluate","sessionId":")") + sid +
        R"(","params":{"expression":"1"}})");

    std::string outB = transport->allSentTo(621);
    assertTrue(outB.find("-32001") != std::string::npos, "hijack rejected as SESSION_NOT_FOUND");
    assertEqual(target->received.size(), size_t(0), "target never saw the hijacked request");

    // The legitimate owner still works with the very same sessionId.
    transport->sent.clear();
    transport->simulateMessage(620,
        std::string(R"({"id":10,"method":"Runtime.evaluate","sessionId":")") + sid +
        R"(","params":{"expression":"1"}})");
    assertEqual(target->received.size(), size_t(1), "owner request reached the target");
    std::string outA = transport->allSentTo(620);
    assertTrue(outA.find("\"from\":\"target\"") != std::string::npos, "owner got the target reply");
    server.stop();
  }});

  // ---- issue #145: single-tenant fallback for path-less transports ----------

  suite.cases.push_back({"tenantId 0 falls back to the sole debug target", []() {
    auto transport = std::make_shared<MockTransport>();
    auto target = std::make_shared<RecordingTarget>();
    CDPServerConfig config;
    config.enabled = true;
    config.transport = transport;
    CDPServer server(config);
    server.registerTenant(1, "OnlyApp");
    server.registerDebugTarget(1, target);
    server.start();

    // Apple nw_ws transport cannot supply the upgrade path: empty path, so the
    // connection's session keeps tenantId 0 (tenant ids start at 1).
    transport->simulateConnect(700);
    transport->simulateMessage(700, R"({"id":3,"method":"Debugger.enable"})");

    assertEqual(target->received.size(), size_t(1), "request reached the sole target");
    assertTrue(target->received[0].find("Debugger.enable") != std::string::npos,
               "forwarded verbatim");
    std::string out = transport->allSentTo(700);
    assertTrue(out.find("\"from\":\"target\"") != std::string::npos,
               "reply came from the target, not the local stub handler");
    server.stop();
  }});

  suite.cases.push_back({"no fallback with multiple debug targets", []() {
    auto transport = std::make_shared<MockTransport>();
    auto target1 = std::make_shared<RecordingTarget>();
    auto target2 = std::make_shared<RecordingTarget>();
    CDPServerConfig config;
    config.enabled = true;
    config.transport = transport;
    CDPServer server(config);
    server.registerTenant(1, "App1");
    server.registerTenant(2, "App2");
    server.registerDebugTarget(1, target1);
    server.registerDebugTarget(2, target2);
    server.start();

    // Ambiguous: two tenants, path-less connection. Must NOT guess a target —
    // the client has to attach explicitly via Target.attachToTarget.
    transport->simulateConnect(701);
    transport->simulateMessage(701, R"({"id":4,"method":"Debugger.enable"})");

    assertEqual(target1->received.size(), size_t(0), "target 1 untouched");
    assertEqual(target2->received.size(), size_t(0), "target 2 untouched");
    std::string out = transport->allSentTo(701);
    assertTrue(out.find("\"id\":4") != std::string::npos, "local handler replied");
    assertTrue(out.find("\"from\":\"target\"") == std::string::npos, "no target reply");
    server.stop();
  }});

  suite.cases.push_back({"fallback still honors explicit tenant path binding", []() {
    // A path-bound connection to tenant 2 must keep routing to tenant 2's
    // target even while tenant-0 fallback logic exists.
    auto transport = std::make_shared<MockTransport>();
    auto target2 = std::make_shared<RecordingTarget>();
    CDPServerConfig config;
    config.enabled = true;
    config.transport = transport;
    CDPServer server(config);
    server.registerTenant(2, "App2");
    server.registerDebugTarget(2, target2);
    server.start();

    transport->simulateConnect(702, "/tenant/2");
    transport->simulateMessage(702, R"({"id":6,"method":"Runtime.enable"})");

    assertEqual(target2->received.size(), size_t(1), "path-bound route unchanged");
    server.stop();
  }});

  return suite;
}

}  // anonymous namespace

// Register with the test runner (static-init self-registration).
static struct CDPSessionRoutingRegistrar {
  CDPSessionRoutingRegistrar() {
    TestRunner::instance().addSuite(createCDPSessionRoutingTests());
  }
} s_cdpSessionRoutingRegistrar;
