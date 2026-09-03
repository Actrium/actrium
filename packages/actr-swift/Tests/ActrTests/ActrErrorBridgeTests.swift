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
        guard case let .DeliveryUncertain(operationId, cause) = roundTrip else {
            Issue.record("DeliveryUncertain was not preserved by the bridge")
            return
        }
        #expect(operationId == "swift-call-1")
        #expect(cause == "WebRTC generation replaced before Response")
    }
}
