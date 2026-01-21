//
//  IPCConnectionStateMachineTests.swift
//  CanvasTests
//
//  Tests for IPCConnectionStateMachine state transitions.
//

import Testing
@testable import Canvas

struct IPCConnectionStateMachineTests {

    // MARK: - Initial State

    @Test func initialState_isIdle() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")
        let state = await machine.currentState
        #expect(state == .idle)
    }

    // MARK: - State Transitions

    @Test func idle_connect_transitionsToConnecting() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        let result = await machine.handle(.connect)
        #expect(result == true)

        let state = await machine.currentState
        #expect(state == .connecting)
    }

    @Test func connecting_connectionEstablished_transitionsToReady() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.handle(.connect)
        let result = await machine.handle(.connectionEstablished)
        #expect(result == true)

        let state = await machine.currentState
        #expect(state == .ready)
    }

    @Test func ready_connectionInterruption_transitionsToInterrupted() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.handle(.connect)
        _ = await machine.handle(.connectionEstablished)
        let result = await machine.handle(.connectionInterruption)
        #expect(result == true)

        let state = await machine.currentState
        #expect(state == .interrupted)
    }

    @Test func interrupted_connectionEstablished_transitionsToReady() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.handle(.connect)
        _ = await machine.handle(.connectionEstablished)
        _ = await machine.handle(.connectionInterruption)
        let result = await machine.handle(.connectionEstablished)
        #expect(result == true)

        let state = await machine.currentState
        #expect(state == .ready)
    }

    @Test func ready_connectionInvalidation_transitionsToInvalidated() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.handle(.connect)
        _ = await machine.handle(.connectionEstablished)
        let result = await machine.handle(.connectionInvalidation)
        #expect(result == true)

        let state = await machine.currentState
        #expect(state == .invalidated)
    }

    @Test func anyState_disable_transitionsToDisabled() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.handle(.connect)
        _ = await machine.handle(.connectionEstablished)
        let result = await machine.handle(.disable)
        #expect(result == true)

        let state = await machine.currentState
        #expect(state == .disabled)
    }

    @Test func disabled_enable_transitionsToIdle() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.handle(.disable)
        let result = await machine.handle(.enable)
        #expect(result == true)

        let state = await machine.currentState
        #expect(state == .idle)
    }

    @Test func invalidated_reset_transitionsToIdle() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.handle(.connect)
        _ = await machine.handle(.connectionEstablished)
        _ = await machine.handle(.connectionInvalidation)
        let result = await machine.handle(.reset)
        #expect(result == true)

        let state = await machine.currentState
        #expect(state == .idle)
    }

    // MARK: - Invalid Transitions

    @Test func idle_connectionEstablished_isRejected() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        let result = await machine.handle(.connectionEstablished)
        #expect(result == false)

        let state = await machine.currentState
        #expect(state == .idle)
    }

    @Test func ready_connect_isRejected() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.handle(.connect)
        _ = await machine.handle(.connectionEstablished)
        let result = await machine.handle(.connect)
        #expect(result == false)

        let state = await machine.currentState
        #expect(state == .ready)
    }

    @Test func invalidated_connect_isRejected() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.handle(.connect)
        _ = await machine.handle(.connectionEstablished)
        _ = await machine.handle(.connectionInvalidation)
        let result = await machine.handle(.connect)
        #expect(result == false)

        let state = await machine.currentState
        #expect(state == .invalidated)
    }

    // MARK: - State Properties

    @Test func ready_canSendRequests_isTrue() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.handle(.connect)
        _ = await machine.handle(.connectionEstablished)

        let canSend = await machine.canSendRequests
        #expect(canSend == true)
    }

    @Test func idle_canSendRequests_isFalse() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        let canSend = await machine.canSendRequests
        #expect(canSend == false)
    }

    @Test func interrupted_canSendRequests_isFalse() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.handle(.connect)
        _ = await machine.handle(.connectionEstablished)
        _ = await machine.handle(.connectionInterruption)

        let canSend = await machine.canSendRequests
        #expect(canSend == false)
    }

    @Test func connecting_shouldQueue_isTrue() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.handle(.connect)

        let shouldQueue = await machine.shouldQueueRequests
        #expect(shouldQueue == true)
    }

    @Test func interrupted_shouldQueue_isTrue() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.handle(.connect)
        _ = await machine.handle(.connectionEstablished)
        _ = await machine.handle(.connectionInterruption)

        let shouldQueue = await machine.shouldQueueRequests
        #expect(shouldQueue == true)
    }

    @Test func invalidated_shouldQueue_isFalse() async throws {
        let machine = IPCConnectionStateMachine(serviceId: "test.service")

        _ = await machine.handle(.connect)
        _ = await machine.handle(.connectionEstablished)
        _ = await machine.handle(.connectionInvalidation)

        let shouldQueue = await machine.shouldQueueRequests
        #expect(shouldQueue == false)
    }
}
