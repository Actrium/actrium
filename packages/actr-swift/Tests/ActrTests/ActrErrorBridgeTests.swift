@testable import Actr
import ActrBindings
import Foundation
import Testing

@Suite(.serialized)
struct ActrErrorBridgeTests {
    @Test
    func allMessagesMatchRustDisplayAfterThrowAndPublicBridge() throws {
        let text = "\u{4e2d}\u{6587}\n\"quoted\"\\path"
        let cases: [(ActrBindings.ActrError, String)] = [
            (.Unavailable(msg: text), "unavailable: \(text)"),
            (.ConnectionNotReady(info: .init(retryAfterMs: 42)),
             "connection not ready: retry after 42ms"),
            (.ConnectionNotReady(info: .init(retryAfterMs: nil)),
             "connection not ready: no retry hint"),
            (.TimedOut, "timed out"),
            (.NotFound(msg: text), "not found: \(text)"),
            (.PermissionDenied(msg: text), "permission denied: \(text)"),
            (.InvalidArgument(msg: text), "invalid argument: \(text)"),
            (.UnknownRoute(msg: text), "unknown route: \(text)"),
            (.DependencyNotFound(serviceName: "echo", detail: text), "dependency 'echo' not found: \(text)"),
            (.DecodeFailure(msg: text), "decode failure: \(text)"),
            (.NotImplemented(msg: text), "not implemented: \(text)"),
            (.Internal(msg: text), "internal error: \(text)"),
            (.Config(msg: text), "config: \(text)"),
            (.DeliveryUncertain(operationId: "op-1", cause: text), "delivery uncertain for operation op-1: \(text)"),
            (.Internal(msg: ""), "internal error: "),
        ]
        for (original, expected) in cases {
            #expect(ActrBindings.actrErrorMessage(err: original) == expected)
            #expect(original.localizedDescription == expected)
            do {
                try ActrBindings.actrErrorThrowForTest(err: original)
                Issue.record("expected Rust to throw")
            } catch let error as ActrBindings.ActrError {
                #expect(error == original)
                #expect(error.localizedDescription == expected)
                let publicError = Actr.ActrError(bridge: error)
                #expect(publicError.localizedDescription == expected)
                #expect(publicError.bridge == original)
            }
        }
    }

    @Test
    func deliveryUncertainBridgeRoundTripPreservesFields() {
        // Both modules define `ActrError` (the high-level wrapper in `Actr`
        // and the generated enum in `ActrBindings`), so every reference here
        // is module-qualified.
        let original = Actr.ActrError.DeliveryUncertain(
            operationId: "swift-call-1",
            cause: "WebRTC generation replaced before Response"
        )

        let roundTrip = Actr.ActrError(bridge: original.bridge)

        #expect(roundTrip == original)
        // Send the generated discriminant through the Rust FFI classifier as
        // well, so the Rust reader and Swift writer agree on case 13.
        #expect(actrErrorKind(original) == .internal)
        #expect(!actrErrorIsRetryable(original))
        guard case let .DeliveryUncertain(operationId, cause) = roundTrip else {
            Issue.record("DeliveryUncertain was not preserved by the bridge")
            return
        }
        #expect(operationId == "swift-call-1")
        #expect(cause == "WebRTC generation replaced before Response")
    }

    @Test
    func rustThrownDeliveryUncertainLiftsThroughErrorBuffer() throws {
        // Drive a genuine Rust UniFFI throw through the FFI error buffer so
        // the generated reader (discriminant, field order, integer width) is
        // exercised against the Rust writer, not just against itself. The
        // throw/lift cycle stays at the ActrBindings level; the lifted value
        // is then fed through the high-level classifier wrappers.
        do {
            try ActrBindings.actrErrorThrowForTest(
                err: .DeliveryUncertain(
                    operationId: "swift-ffi-1",
                    causeDetail: "ambiguous partial write"
                ))
            Issue.record("actrErrorThrowForTest was expected to throw")
        } catch let error as ActrBindings.ActrError {
            guard case let .DeliveryUncertain(operationId, cause) = error else {
                Issue.record("expected DeliveryUncertain, got \(error)")
                return
            }
            #expect(operationId == "swift-ffi-1")
            #expect(cause == "ambiguous partial write")
            let lifted = Actr.ActrError(bridge: error)
            #expect(actrErrorKind(lifted) == .internal)
            #expect(!actrErrorIsRetryable(lifted))
        }
    }
}
