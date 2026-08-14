@testable import Actr
import Foundation
import ActrBindings
import SwiftProtobuf
import Testing

private final class InvalidResponseActrRefWrapper: ActrBindings.ActrRefWrapper, @unchecked Sendable {
    private(set) var receivedPayloadType: ActrBindings.PayloadType?

    init() {
        super.init(noHandle: ActrBindings.ActrRefWrapper.NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        super.init(unsafeFromHandle: handle)
    }

    override func call(
        routeKey _: String,
        payloadType: ActrBindings.PayloadType,
        requestPayload _: Data,
        timeoutMs _: Int64
    ) async throws -> Data {
        receivedPayloadType = payloadType
        return Data([0xff])
    }
}

private final class MonitoringResourceProbe: MobileMonitoringResource, @unchecked Sendable {}

private final class RecordingActrContext: ActrContext, @unchecked Sendable {
    struct RecordedCall {
        let routeKey: String
        let payloadType: Actr.PayloadType
        let timeoutMs: Int64
    }

    let selfId = Actr.ActrId(
        realm: Actr.Realm(realmId: 1),
        serialNumber: 7,
        type: Actr.ActrType(manufacturer: "test", name: "Recorder", version: "1.0.0")
    )
    let callerId: Actr.ActrId? = nil
    let requestId = "request-1"
    private(set) var calls: [RecordedCall] = []

    func callRaw(
        target _: Actr.ActrId,
        routeKey: String,
        payloadType: Actr.PayloadType,
        payload _: Data,
        timeoutMs: Int64
    ) async throws(Actr.ActrError) -> Data {
        calls.append(RecordedCall(routeKey: routeKey, payloadType: payloadType, timeoutMs: timeoutMs))
        return Data()
    }

    func discover(targetType _: Actr.ActrType) async throws(Actr.ActrError) -> Actr.ActrId {
        selfId
    }

    func tellRaw(
        target _: Actr.ActrId,
        routeKey _: String,
        payloadType _: Actr.PayloadType,
        payload _: Data
    ) async throws(Actr.ActrError) {}

    func log(level _: Actr.LogLevel, msg _: String) {}
}

extension Google_Protobuf_Empty: RpcRequest {
    public typealias Response = Google_Protobuf_Empty

    public static var routeKey: String {
        "test.Empty/Echo"
    }

    public static var payloadType: Actr.PayloadType {
        .rpcSignal
    }
}

@Test func typedCallWrapsResponseDecodeFailuresAsActrError() async throws {
    let wrapper = InvalidResponseActrRefWrapper()
    let ref = ActrRef(inner: wrapper)

    do {
        _ = try await ref.call(Google_Protobuf_Empty())
        Issue.record("Expected typed call to wrap the response decode failure")
    } catch let error {
        guard case let .DecodeFailure(msg) = error else {
            Issue.record("Expected decode failure ActrError, got \(error)")
            return
        }
        #expect(!msg.isEmpty)
    }

    #expect(wrapper.receivedPayloadType == .rpcSignal)
}

@Test func runningReferenceRetainsMobileMonitoringResource() {
    let wrapper = InvalidResponseActrRefWrapper()
    weak var weakProbe: MonitoringResourceProbe?
    var ref: ActrRef?

    do {
        let probe = MonitoringResourceProbe()
        weakProbe = probe
        ref = ActrRef(inner: wrapper, retainedMonitoringResource: probe)
    }

    #expect(weakProbe != nil)
    withExtendedLifetime(ref) {}
    ref = nil
    #expect(weakProbe == nil)
}

@Test func injectedActrContextSupportsTypedCalls() async throws {
    let ctx = RecordingActrContext()

    _ = try await ctx.callWithTimeout(target: ctx.selfId, request: Google_Protobuf_Empty(), timeoutMs: 5)

    #expect(ctx.calls.count == 1)
    #expect(ctx.calls.first?.routeKey == Google_Protobuf_Empty.routeKey)
    #expect(ctx.calls.first?.payloadType == .rpcSignal)
    #expect(ctx.calls.first?.timeoutMs == 5)
}

@Test func injectedActrContextDefaultLaneAppliesThirtySecondDefault() async throws {
    let ctx = RecordingActrContext()

    _ = try await ctx.call(target: ctx.selfId, request: Google_Protobuf_Empty())
    _ = try await ctx.callRaw(target: ctx.selfId, routeKey: "test.Empty/Echo", payload: Data())

    #expect(ctx.calls.count == 2)
    #expect(ctx.calls.allSatisfy { $0.timeoutMs == defaultRpcTimeoutMs })
    #expect(ctx.calls.last?.payloadType == .rpcReliable)
}

@Test func explicitTimeoutLanesRejectNonPositiveTimeout() async throws {
    let ctx = RecordingActrContext()

    do {
        _ = try await ctx.callWithTimeout(target: ctx.selfId, request: Google_Protobuf_Empty(), timeoutMs: 0)
        Issue.record("Expected InvalidArgument for timeoutMs=0")
    } catch Actr.ActrError.InvalidArgument {}

    do {
        _ = try await ctx.callRawWithTimeout(target: ctx.selfId, routeKey: "test.Empty/Echo", payload: Data(), timeoutMs: -1)
        Issue.record("Expected InvalidArgument for timeoutMs=-1")
    } catch Actr.ActrError.InvalidArgument {}

    #expect(ctx.calls.isEmpty)
}
