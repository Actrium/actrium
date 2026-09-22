import Actr
import Foundation
import Testing

private struct DispatchRecord: Equatable, Sendable {
    let routeKey: String
    let payload: Data
}

private actor DispatchRecorder {
    private var dispatches: [DispatchRecord] = []

    func append(_ record: DispatchRecord) {
        dispatches.append(record)
    }

    func snapshot() -> [DispatchRecord] {
        dispatches
    }
}

private final class StaticWorkloadProbe: Workload, @unchecked Sendable {
    private let recorder = DispatchRecorder()

    func onStart(ctx _: Context) async throws {}

    func onReady(ctx _: Context) async throws {}

    func onStop(ctx _: Context) async throws {}

    func onError(ctx _: Context, event _: ErrorEvent) async throws {}

    func dispatch(ctx _: Context, envelope: RpcEnvelope) async throws -> Data {
        guard !envelope.routeKey.isEmpty else {
            throw ActrError.UnknownRoute(msg: envelope.routeKey)
        }

        await recorder.append(DispatchRecord(routeKey: envelope.routeKey, payload: envelope.payload))

        let payload = String(bytes: envelope.payload, encoding: .utf8) ?? ""
        return Data("swift-local:\(payload)".utf8)
    }

    func snapshot() async -> [DispatchRecord] {
        await recorder.snapshot()
    }

    func waitForDispatchCount(_ expected: Int) async throws -> [DispatchRecord] {
        for _ in 0..<100 {
            let records = await snapshot()
            if records.count >= expected {
                return records
            }
            try await Task.sleep(nanoseconds: 50_000_000)
        }
        return await snapshot()
    }
}

private final class RuntimeObserverProbe: SignalingObserver, WebRTCObserver, @unchecked Sendable {
    func onConnecting(ctx _: Context?) async {}

    func onConnected(ctx _: Context?) async {}

    func onDisconnected(ctx _: Context) async {}

    func onConnecting(ctx _: Context, event _: PeerEvent) async {}

    func onConnected(ctx _: Context, event _: PeerEvent) async {}

    func onDisconnected(ctx _: Context, event _: PeerEvent) async {}
}

private final class MobileLifecycleObserverProbe: MobileLifecycleObserver, @unchecked Sendable {
    func onMobileLifecycleEvent(_: MobileLifecycleEvent) {}
}

@Suite(.serialized)
private struct DynamicWorkloadTests {
    @Test func deliveryUncertainPreservesNonRetryableSemantics() {
        let error = ActrError.DeliveryUncertain(
            operationId: "swift-call-1",
            cause: "WebRTC generation replaced before Response"
        )

        #expect(actrErrorKind(error) == .internal)
        #expect(!actrErrorIsRetryable(error))
        #expect(error.errorDescription?.contains("swift-call-1") == true)
    }

    @Test func packageRuntimeObserversExposeReadinessHooksAndConnectionNotReady() async throws {
        let probe = RuntimeObserverProbe()
        let observers = runtimeObservers(signaling: probe, webrtc: probe)
        let mobileObserver = MobileLifecycleObserverProbe()
        let error = ActrError.ConnectionNotReady(
            info: ConnectionNotReadyInfo(retryAfterMs: 6_300)
        )

        #expect(actrErrorKind(error) == .transient)
        #expect(actrErrorIsRetryable(error))

        let remoteConfigURL = try #require(URL(string: "https://example.com/actr.toml"))
        let packageURL = URL(fileURLWithPath: "/tmp/example.actr")

        do {
            _ = try await ActrNode.from(
                packageConfig: remoteConfigURL,
                packageURL: packageURL,
                observers: observers,
                mobileLifecycleObservers: [mobileObserver]
            )
            Issue.record("package-backed observer API should reject non-file config URLs before starting runtime")
        } catch ActrError.Config(let msg) {
            #expect(msg == "packageConfig URL must be a file URL")
        } catch {
            Issue.record("Unexpected error: \(error)")
        }
    }

    @Test func packageMobileObserverUrlOverloadRejectsNonFileConfig() async throws {
        let mobileObserver = MobileLifecycleObserverProbe()
        let remoteConfigURL = try #require(URL(string: "https://example.com/actr.toml"))
        let packageURL = URL(fileURLWithPath: "/tmp/example.actr")

        do {
            _ = try await ActrNode.from(
                packageConfig: remoteConfigURL,
                packageURL: packageURL,
                mobileLifecycleObservers: [mobileObserver]
            )
            Issue.record("package mobile observer API should reject non-file config URLs")
        } catch ActrError.Config(let msg) {
            #expect(msg == "packageConfig URL must be a file URL")
        } catch {
            Issue.record("Unexpected error: \(error)")
        }
    }

    @Test func dynamicWorkloadAcceptsSwiftLifecycleForLinkedNodeApi() async throws {
        let lifecycle = StaticWorkloadProbe()
        let workload = dynamicWorkload(
            lifecycle: lifecycle,
            signaling: nil,
            websocket: nil,
            webrtc: nil,
            credential: nil,
            mailbox: nil
        )
        let actorType = ActrType(publisher: "acme", name: "EchoApp", version: "0.1.0")
        let remoteConfigURL = try #require(URL(string: "https://example.com/actr.toml"))
        let mobileObserver = MobileLifecycleObserverProbe()

        do {
            _ = try await ActrNode.linked(
                config: remoteConfigURL,
                type: actorType,
                workload: workload,
                mobileLifecycleObservers: [mobileObserver]
            )
            Issue.record("linked(config:type:workload:) should reject non-file config URLs before starting runtime")
        } catch ActrError.Config(let msg) {
            #expect(msg == "config URL must be a file URL")
        } catch {
            Issue.record("Unexpected error: \(error)")
        }
    }

    @Test
    func linkedNodeDispatchesLocalTellToSwiftLifecycle() async throws {
        guard ProcessInfo.processInfo.environment["ACTR_SWIFT_LINKED_RUNTIME_E2E"] == "1" else {
            return
        }

        let lifecycle = StaticWorkloadProbe()
        let workload = dynamicWorkload(
            lifecycle: lifecycle,
            signaling: nil,
            websocket: nil,
            webrtc: nil,
            credential: nil,
            mailbox: nil
        )
        let actorType = ActrType(publisher: "acme", name: "EchoApp", version: "0.1.0")
        let tempDir = try makeTemporaryDirectory()
        defer { try? FileManager.default.removeItem(at: tempDir) }

        let configURL = try writeLinkedWorkloadConfig(in: tempDir)
        let actrRef = try await startLinkedWorkloadRef(config: configURL, type: actorType, workload: workload)

        let response = try await actrRef.call(
            routeKey: "echoapp.LocalEchoService.Send",
            payloadType: .rpcReliable,
            requestPayload: Data("hello".utf8)
        )
        #expect(response == Data("swift-local:hello".utf8))

        let records = try await lifecycle.waitForDispatchCount(1)
        #expect(records == [
            DispatchRecord(routeKey: "echoapp.LocalEchoService.Send", payload: Data("hello".utf8)),
        ])

        await actrRef.stop()
    }
}

private func makeTemporaryDirectory() throws -> URL {
    let url = FileManager.default.temporaryDirectory
        .appendingPathComponent("actr-swift-linked-workload-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
}

private func writeLinkedWorkloadConfig(in tempDir: URL) throws -> URL {
    let configURL = tempDir.appendingPathComponent("actr.toml")
    let dataDir = tempDir.path.replacingOccurrences(of: "\\", with: "/")
    let config = """
    edition = 1

    [signaling]
    url = "ws://127.0.0.1:8081/signaling/ws"

    [issuer_endpoint]
    url = "http://127.0.0.1:8081/issuer"

    [deployment]
    realm_id = 1

    [hyper]
    data_dir = "\(dataDir)"

    [hyper.trust]
    kind = "dev_only"
    """
    try config.write(to: configURL, atomically: true, encoding: .utf8)
    return configURL
}

private func startLinkedWorkloadRef(config configURL: URL, type actorType: ActrType, workload: DynamicWorkload) async throws -> ActrRef {
    let node = try await ActrNode.linked(config: configURL, type: actorType, workload: workload)
    return try await node.start()
}
