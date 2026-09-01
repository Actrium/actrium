import Actr
import Foundation
import SwiftProtobuf
import SwiftUI

// ACTR: mutable scaffold
// User-owned business scaffold. Keep EchoApp/Generated immutable and customize this file.

@MainActor
final class ActrService: ObservableObject {
    @Published var status = "Starting ACTR node..."
    @Published var errorMessage: String?

    private var actrNode: ActrNode?
    private var actorRef: ActrRef?
    private var linkedWorkload: DynamicWorkload?
    private var hasAutoSent = false
    private var isStarting = false

    var isReady: Bool {
        actorRef != nil
    }

    func startIfNeeded() async {
        guard actorRef == nil, !isStarting else { return }
        isStarting = true
        defer { isStarting = false }

        do {
            let manifestURL = try materializeRuntimeConfig()
            let actorType = ActrType(manufacturer: "actrium", name: "EchoApp", version: "0.1.0")

            let handler = LocalEchoServiceHandlerImpl()
            let workload = Actr.dynamicWorkload(
                lifecycle: LocalEchoServiceLifecycleAdapter(handler: handler)
            )
            linkedWorkload = workload
            let node = try await ActrNode.linked(config: manifestURL, type: actorType, workload: workload)

            actorRef = try await node.start()
            actrNode = node
            status = "Ready: \(actorType.toStringRepr())"
        } catch {
            status = "ACTR startup failed: \(error)"
            errorMessage = String(describing: error)
        }
    }

    func stop() async {
        guard let actorRef else { return }
        await actorRef.stop()
        self.actorRef = nil
        actrNode = nil
    }

    func sendEcho(_ input: String) async throws -> String {
        guard let actorRef else {
            throw EchoAppError.actorUnavailable
        }

        var localRequest = Echoapp_LocalEchoRequest()
        localRequest.message = input
        let requestData = try EchoappLocalEchoServiceSendRpc.encodeRequest(localRequest)
        let responseData = try await actorRef.call(
            routeKey: EchoappLocalEchoServiceSendRpc.routeKey,
            payloadType: EchoappLocalEchoServiceSendRpc.payloadType,
            requestPayload: requestData,
            timeoutMs: 30_000
        )
        let response = try EchoappLocalEchoServiceSendRpc.decodeResponse(responseData)
        autoMarkSent()
        return response.reply
    }

    func sendEchoWhenReady(_ input: String) async throws -> String {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: .seconds(20))

        while true {
            do {
                return try await sendEcho(input)
            } catch let error as ActrError {
                guard case let .ConnectionNotReady(info) = error, clock.now < deadline else {
                    throw error
                }

                let retryAfterMs = min(max(info.retryAfterMs ?? 250, 100), 5_000)
                let marker = "ACTR_E2E_RETRY:ConnectionNotReady retryAfterMs=\(retryAfterMs)"
                print(marker)
                FileHandle.standardError.write(Data("\(marker)\n".utf8))

                // S6-8: post-Connected retry contract via the unified
                // snapshot. If any tracked peer is already sendable, retry
                // immediately instead of sleeping out the hint — the hint is
                // a fallback, not a readiness promise.
                if let actorRef,
                   await actorRef.listRtcPeerStatuses().contains(where: { $0.sendable })
                {
                    continue
                }

                try await Task.sleep(for: .milliseconds(Int64(retryAfterMs)))
            }
        }
    }

    func shouldAutoSend() -> Bool {
        ProcessInfo.processInfo.environment["ACTR_ECHOAPP_AUTO_SEND"] == "1" && !hasAutoSent
    }

    private func autoMarkSent() {
        hasAutoSent = true
    }

    private func materializeRuntimeConfig() throws -> URL {
        guard let templateURL = Bundle.main.url(forResource: "actr", withExtension: "toml") else {
            throw EchoAppError.missingConfigTemplate
        }
        guard let bundledManifestURL = Bundle.main.url(forResource: "manifest", withExtension: "toml") else {
            throw EchoAppError.missingManifest
        }

        let fileManager = FileManager.default
        let supportURL = try fileManager.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let appURL = supportURL.appendingPathComponent("EchoApp", isDirectory: true)
        let dataURL = appURL.appendingPathComponent("hyper", isDirectory: true)
        try fileManager.createDirectory(at: dataURL, withIntermediateDirectories: true)

        var config = try String(contentsOf: templateURL, encoding: .utf8)
        config += """

        [hyper]
        data_dir = "\(dataURL.path)"

        [hyper.trust]
        kind = "dev_only"
        """

        let configURL = appURL.appendingPathComponent("actr.toml")
        try config.write(to: configURL, atomically: true, encoding: .utf8)
        let manifestURL = appURL.appendingPathComponent("manifest.toml")
        try Data(contentsOf: bundledManifestURL).write(to: manifestURL, options: .atomic)
        return manifestURL
    }

}

private enum EchoAppError: Error {
    case missingConfigTemplate
    case missingManifest
    case actorUnavailable
    case remoteActorUnavailable
}

/// Safe: no mutable state. All fields are value types or immutable references.
private final class LocalEchoServiceHandlerImpl: ActrDispatchHandler, @unchecked Sendable {
    private let remoteEchoRoute = "echo.EchoService.Echo"

    init() {}

    func send(
        req localRequest: Echoapp_LocalEchoRequest,
        ctx: any ActrContext
    ) async throws(ActrError) -> Echoapp_LocalEchoResponse {
        let forwardedMessage = "swift-local:\(localRequest.message)"

        var remoteRequest = Echo_EchoRequest()
        remoteRequest.message = forwardedMessage
        let targetType = try await ctx.resolveDependency(alias: "echo")
        let targetId = try await ctx.discover(targetType: targetType)
        let requestData: Data
        do {
            requestData = try remoteRequest.serializedData()
        } catch {
            throw ActrError.DecodeFailure(msg: "Failed to encode echo request: \(error)")
        }
        let remotePayload = try await ctx.call(
            target: targetId,
            routeKey: remoteEchoRoute,
            payload: requestData,
            timeoutMs: 30_000
        )

        let remoteResponse: Echo_EchoResponse
        do {
            remoteResponse = try Echo_EchoResponse(serializedBytes: remotePayload)
        } catch {
            throw ActrError.DecodeFailure(msg: "Failed to decode echo response: \(error)")
        }
        var localResponse = Echoapp_LocalEchoResponse()
        localResponse.reply = remoteResponse.reply
        localResponse.forwardedMessage = forwardedMessage
        return localResponse
    }
}

/// Safe: no mutable state. `handler` is an immutable reference.
private final class LocalEchoServiceLifecycleAdapter: Workload, @unchecked Sendable {
    private let handler: LocalEchoServiceHandlerImpl

    init(handler: LocalEchoServiceHandlerImpl) {
        self.handler = handler
    }

    func onStart(ctx: Context) async throws {}

    func onReady(ctx: Context) async throws {}

    func onStop(ctx: Context) async throws {}

    func onError(ctx: Context, event: ErrorEvent) async throws {
        print("LocalEchoServiceLifecycleAdapter error: \(event)")
    }

    func dispatch(ctx: Context, envelope: RpcEnvelope) async throws -> Data {
        return try await actrDispatch(
            ctx: ctx,
            handler: handler,
            routeKey: envelope.routeKey,
            payload: envelope.payload,
            timeoutMs: 30_000
        )
    }
}
