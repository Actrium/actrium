@testable import Actr
import Testing

@Suite(.serialized)
struct ActrErrorBridgeTests {
    @Test
    func deliveryUncertainBridgeRoundTripPreservesFields() {
        let original = ActrError.DeliveryUncertain(
            operationId: "swift-call-1",
            cause: "WebRTC generation replaced before Response"
        )

        let roundTrip = ActrError(bridge: original.bridge)

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
        // the Swift reader (discriminant, field order, integer width) is
        // exercised against the Rust writer, not just against itself.
        do {
            try actrErrorThrowForTest(
                err: .DeliveryUncertain(
                    operationId: "swift-ffi-1",
                    cause: "ambiguous partial write"
                ))
            Issue.record("actrErrorThrowForTest was expected to throw")
        } catch let error as ActrError {
            guard case let .DeliveryUncertain(operationId, cause) = error else {
                Issue.record("expected DeliveryUncertain, got \(error)")
                return
            }
            #expect(operationId == "swift-ffi-1")
            #expect(cause == "ambiguous partial write")
            #expect(actrErrorKind(error) == .internal)
            #expect(!actrErrorIsRetryable(error))
        }
    }
}
