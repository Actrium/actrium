//
//  IPCConnectionStateMachineTests.swift
//  CanvasTests
//
//  Tests for IPCConnectionStateMachine state transitions.
//

import Foundation
import Testing
@testable import Canvas

struct IPCConnectionStateMachineTests {

    // MARK: - Initial State

    @Test func initialState_isIdle() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")
        let state = await machine.state
        #expect(state == .idle)
    }

    // MARK: - State Transitions

    @Test func idle_connect_transitionsToConnecting() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        let result = await machine.process(.connect)
        #expect(result.wasSuccessful)

        let state = await machine.state
        #expect(state == .connecting)
    }

    @Test func connecting_connectionEstablished_transitionsToReady() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.process(.connect)
        let result = await machine.process(.connectionEstablished)
        #expect(result.wasSuccessful)

        let state = await machine.state
        #expect(state == .ready)
    }

    @Test func ready_connectionInterruption_transitionsToInterrupted() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.process(.connect)
        _ = await machine.process(.connectionEstablished)
        let result = await machine.process(.connectionInterruption)
        #expect(result.wasSuccessful)

        let state = await machine.state
        #expect(state == .interrupted)
    }

    @Test func interrupted_connectionEstablished_transitionsToReady() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.process(.connect)
        _ = await machine.process(.connectionEstablished)
        _ = await machine.process(.connectionInterruption)
        let result = await machine.process(.connectionEstablished)
        #expect(result.wasSuccessful)

        let state = await machine.state
        #expect(state == .ready)
    }

    @Test func ready_connectionInvalidation_transitionsToInvalidated() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.process(.connect)
        _ = await machine.process(.connectionEstablished)
        let result = await machine.process(.connectionInvalidation(reason: "test"))
        #expect(result.wasSuccessful)

        let state = await machine.state
        if case .invalidated = state {
            // expected
        } else {
            Issue.record("Expected .invalidated state, got \(state)")
        }
    }

    @Test func anyState_disable_transitionsToDisabled() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.process(.connect)
        _ = await machine.process(.connectionEstablished)
        let result = await machine.process(.disable)
        #expect(result.wasSuccessful)

        let state = await machine.state
        #expect(state == .disabled)
    }

    @Test func disabled_enable_transitionsToIdle() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.process(.disable)
        let result = await machine.process(.enable)
        #expect(result.wasSuccessful)

        let state = await machine.state
        #expect(state == .idle)
    }

    @Test func invalidated_reset_transitionsToIdle() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.process(.connect)
        _ = await machine.process(.connectionEstablished)
        _ = await machine.process(.connectionInvalidation(reason: "test"))
        let result = await machine.process(.reset)
        #expect(result.wasSuccessful)

        let state = await machine.state
        #expect(state == .idle)
    }

    // MARK: - Invalid Transitions

    @Test func idle_connectionEstablished_isRejected() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        let result = await machine.process(.connectionEstablished)
        #expect(!result.wasSuccessful)

        let state = await machine.state
        #expect(state == .idle)
    }

    @Test func ready_connect_isRejected() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.process(.connect)
        _ = await machine.process(.connectionEstablished)
        let result = await machine.process(.connect)
        #expect(!result.wasSuccessful)

        let state = await machine.state
        #expect(state == .ready)
    }

    @Test func invalidated_connect_isRejected() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.process(.connect)
        _ = await machine.process(.connectionEstablished)
        _ = await machine.process(.connectionInvalidation(reason: "test"))
        let result = await machine.process(.connect)
        #expect(!result.wasSuccessful)

        let state = await machine.state
        if case .invalidated = state {
            // expected
        } else {
            Issue.record("Expected .invalidated state, got \(state)")
        }
    }

    // MARK: - State Properties

    @Test func ready_canAcceptRequests_isTrue() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.process(.connect)
        _ = await machine.process(.connectionEstablished)

        let canSend = await machine.canAcceptRequests
        #expect(canSend == true)
    }

    @Test func idle_canAcceptRequests_isFalse() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        let canSend = await machine.canAcceptRequests
        #expect(canSend == false)
    }

    @Test func interrupted_canAcceptRequests_isFalse() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.process(.connect)
        _ = await machine.process(.connectionEstablished)
        _ = await machine.process(.connectionInterruption)

        let canSend = await machine.canAcceptRequests
        #expect(canSend == false)
    }

    @Test func connecting_shouldQueue_isTrue() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.process(.connect)

        let shouldQueue = await machine.shouldQueueRequests
        #expect(shouldQueue == true)
    }

    @Test func interrupted_shouldQueue_isTrue() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.process(.connect)
        _ = await machine.process(.connectionEstablished)
        _ = await machine.process(.connectionInterruption)

        let shouldQueue = await machine.shouldQueueRequests
        #expect(shouldQueue == true)
    }

    @Test func invalidated_shouldQueue_isFalse() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.process(.connect)
        _ = await machine.process(.connectionEstablished)
        _ = await machine.process(.connectionInvalidation(reason: "test"))

        let shouldQueue = await machine.shouldQueueRequests
        #expect(shouldQueue == false)
    }
}
