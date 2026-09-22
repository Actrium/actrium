//! actr-cabi Swift wrapper — bridges the C ABI callback model to Swift
//! async/await via `withCheckedThrowingContinuation` and a thread-safe,
//! single-shot continuation registry.

import Foundation
import ActrCabiLib

// MARK: - Errors

public enum ActrCabiError: Error, @unchecked Sendable {
    case runtimeInit(Int32)
    case nullPointer
    case handleInvalid
    case invalidState(expected: String)
    case generic(code: Int32, message: String)
    case panic
}

// MARK: - Continuation registry

/// Thread-safe map from a non-zero integer key to a single-shot continuation.
/// The key is encoded into the C `user_data` pointer so stateless C trampolines
/// can recover the awaiting Swift task.
private final class ContinuationMap: @unchecked Sendable {
    static let shared = ContinuationMap()

    private var map: [Int: Any] = [:]
    private let lock = NSLock()
    private var counter = 0

    func register<T>(_ continuation: CheckedContinuation<T, Error>) -> Int {
        lock.lock()
        defer { lock.unlock() }

        repeat {
            counter = counter == Int.max ? 1 : counter + 1
        } while map[counter] != nil
        map[counter] = continuation
        return counter
    }

    func take<T>(_ key: Int) -> CheckedContinuation<T, Error>? {
        lock.lock()
        defer { lock.unlock() }
        return map.removeValue(forKey: key) as? CheckedContinuation<T, Error>
    }
}

/// Decode an advertised UTF-8 buffer without reading past its explicit length.
/// Native callback buffers are valid only until the callback returns, so this
/// function always copies them immediately.
private func decodeUtf8(_ ptr: UnsafeRawPointer?, _ len: Int) -> String {
    guard len > 0, let ptr else { return "" }
    let bytes = UnsafeBufferPointer(
        start: ptr.assumingMemoryBound(to: UInt8.self),
        count: len
    )
    return String(decoding: bytes, as: UTF8.self)
}

private func callbackError(
    status: Int32,
    errorPtr: UnsafePointer<CChar>?,
    errorLen: Int
) -> ActrCabiError {
    switch status {
    case -10:
        return .nullPointer
    case -21:
        return .handleInvalid
    case -99:
        return .panic
    default:
        return .generic(
            code: status,
            message: decodeUtf8(errorPtr.map { UnsafeRawPointer($0) }, errorLen)
        )
    }
}

private func userDataKey(_ userData: UnsafeMutableRawPointer?) -> Int? {
    guard let userData else { return nil }
    return Int(bitPattern: OpaquePointer(userData))
}

/// Encode a non-zero integer key as a `user_data` pointer.
private func keyToUserData(_ key: Int) -> UnsafeMutableRawPointer {
    precondition(key != 0)
    return UnsafeMutableRawPointer(bitPattern: key)!
}

// MARK: - C trampolines

@_cdecl("actr_cabi_handle_trampoline")
fileprivate func handleTrampoline(
    _ status: Int32,
    _ handle: UnsafeMutableRawPointer?,
    _ errorPtr: UnsafePointer<CChar>?,
    _ errorLen: Int,
    _ userData: UnsafeMutableRawPointer?
) {
    guard
        let key = userDataKey(userData),
        let continuation: CheckedContinuation<HandleBox, Error> =
            ContinuationMap.shared.take(key)
    else {
        // A late or duplicate successful callback must not leak its newly
        // created native handle. The generic release is type-aware.
        if let handle { actr_release_handle(handle) }
        return
    }

    if status == 0, let handle {
        continuation.resume(returning: HandleBox(ptr: handle))
    } else if status == 0 {
        continuation.resume(throwing: ActrCabiError.handleInvalid)
    } else {
        continuation.resume(throwing: callbackError(
            status: status,
            errorPtr: errorPtr,
            errorLen: errorLen
        ))
    }
}

@_cdecl("actr_cabi_call_trampoline")
fileprivate func callTrampoline(
    _ status: Int32,
    _ respPtr: UnsafePointer<UInt8>?,
    _ respLen: Int,
    _ errorPtr: UnsafePointer<CChar>?,
    _ errorLen: Int,
    _ userData: UnsafeMutableRawPointer?
) {
    guard
        let key = userDataKey(userData),
        let continuation: CheckedContinuation<Data, Error> = ContinuationMap.shared.take(key)
    else { return }

    if status != 0 {
        continuation.resume(throwing: callbackError(
            status: status,
            errorPtr: errorPtr,
            errorLen: errorLen
        ))
    } else if respLen == 0 {
        continuation.resume(returning: Data())
    } else if let respPtr {
        continuation.resume(returning: Data(bytes: respPtr, count: respLen))
    } else {
        continuation.resume(throwing: ActrCabiError.nullPointer)
    }
}

@_cdecl("actr_cabi_discover_trampoline")
fileprivate func discoverTrampoline(
    _ status: Int32,
    _ actorIdPtr: UnsafePointer<CChar>?,
    _ actorIdLen: Int,
    _ errorPtr: UnsafePointer<CChar>?,
    _ errorLen: Int,
    _ userData: UnsafeMutableRawPointer?
) {
    guard
        let key = userDataKey(userData),
        let continuation: CheckedContinuation<String, Error> = ContinuationMap.shared.take(key)
    else { return }

    if status != 0 {
        continuation.resume(throwing: callbackError(
            status: status,
            errorPtr: errorPtr,
            errorLen: errorLen
        ))
    } else if actorIdLen > 0, let actorIdPtr {
        continuation.resume(returning: decodeUtf8(UnsafeRawPointer(actorIdPtr), actorIdLen))
    } else {
        continuation.resume(throwing: ActrCabiError.nullPointer)
    }
}

@_cdecl("actr_cabi_void_trampoline")
fileprivate func voidTrampoline(
    _ status: Int32,
    _ errorPtr: UnsafePointer<CChar>?,
    _ errorLen: Int,
    _ userData: UnsafeMutableRawPointer?
) {
    guard
        let key = userDataKey(userData),
        let continuation: CheckedContinuation<Void, Error> = ContinuationMap.shared.take(key)
    else { return }

    if status == 0 {
        continuation.resume()
    } else {
        continuation.resume(throwing: callbackError(
            status: status,
            errorPtr: errorPtr,
            errorLen: errorLen
        ))
    }
}

// MARK: - Internal handle box

/// `UnsafeMutableRawPointer` is not `Sendable`; native handle tokens are
/// immutable values, so this wrapper may cross the continuation boundary.
private struct HandleBox: @unchecked Sendable {
    let ptr: UnsafeMutableRawPointer
}

/// Keeps observer context owners alive when a node is deinitialized without an
/// explicit `waitShutdown()`. The retained box is released only after native
/// shutdown completion, when observer callbacks can no longer fire.
private final class ObserverLifetimeCleanup: @unchecked Sendable {
    let owners: [AnyObject]

    init(owners: [AnyObject]) {
        self.owners = owners
    }
}

@_cdecl("actr_cabi_observer_cleanup_trampoline")
fileprivate func observerCleanupTrampoline(
    _ status: Int32,
    _ errorPtr: UnsafePointer<CChar>?,
    _ errorLen: Int,
    _ userData: UnsafeMutableRawPointer?
) {
    guard let userData else { return }
    Unmanaged<ObserverLifetimeCleanup>.fromOpaque(userData).release()
}

// MARK: - Observer callbacks

/// Swift-facing observer callback table.
///
/// Callbacks execute synchronously on actr runtime threads. They must not
/// block, throw Objective-C exceptions, or retain callback buffer pointers.
/// If `userData` points into an object, pass that object as `lifetimeOwner` so
/// the node retains it until its native observer is dropped.
public struct ActrCabiObserverCallbacks {
    public var userData: UnsafeMutableRawPointer?
    public var onSignalingConnecting: (@convention(c) (UnsafeMutableRawPointer?) -> Void)?
    public var onSignalingConnected: (@convention(c) (UnsafeMutableRawPointer?) -> Void)?
    public var onSignalingDisconnected: (@convention(c) (UnsafeMutableRawPointer?) -> Void)?
    public var onError: (@convention(c) (UnsafePointer<UInt8>?, Int, UnsafeMutableRawPointer?) -> Void)?
    fileprivate var lifetimeOwner: AnyObject?

    public init(
        userData: UnsafeMutableRawPointer? = nil,
        lifetimeOwner: AnyObject? = nil,
        onSignalingConnecting: (@convention(c) (UnsafeMutableRawPointer?) -> Void)? = nil,
        onSignalingConnected: (@convention(c) (UnsafeMutableRawPointer?) -> Void)? = nil,
        onSignalingDisconnected: (@convention(c) (UnsafeMutableRawPointer?) -> Void)? = nil,
        onError: (@convention(c) (UnsafePointer<UInt8>?, Int, UnsafeMutableRawPointer?) -> Void)? = nil
    ) {
        self.userData = userData
        self.lifetimeOwner = lifetimeOwner
        self.onSignalingConnecting = onSignalingConnecting
        self.onSignalingConnected = onSignalingConnected
        self.onSignalingDisconnected = onSignalingDisconnected
        self.onError = onError
    }
}

// MARK: - ActrCabiNode

public final class ActrCabiNode: @unchecked Sendable {
    private enum State {
        case empty
        case initializing
        case initialized(UnsafeMutableRawPointer)
        case linking
        case attached(UnsafeMutableRawPointer)
        case registering
        case registered(UnsafeMutableRawPointer)
        case starting
        case started(UnsafeMutableRawPointer)
        case waitingForShutdown(UnsafeMutableRawPointer)
    }

    private let stateLock = NSLock()
    private var state: State = .empty
    private var observerOwners: [AnyObject] = []

    public init() {}

    public static func initRuntime() throws {
        let code = actr_init_runtime()
        if code != 0 { throw ActrCabiError.runtimeInit(code) }
    }

    public static func shutdownRuntime() throws {
        let code = actr_shutdown_runtime()
        if code != 0 { throw ActrCabiError.runtimeInit(code) }
    }

    /// Build a node from a config file.
    ///
    /// The node carries the placeholder identity `local/Client@0.0.0` until
    /// `withActorType(_:)` overrides it. Linked apps that rely on
    /// ACL-filtered discovery must call `withActorType(_:)` between this
    /// method and `linkWorkload(name:)`.
    public func fromConfig(path: String) async throws {
        try withLockedState { state in
            guard case .empty = state else {
                throw ActrCabiError.invalidState(expected: "empty")
            }
            state = .initializing
        }

        do {
            let handle = try await withHandle { callback, userData in
                path.withCString { pathPtr in
                    _ = actr_node_from_config(
                        pathPtr,
                        path.utf8.count,
                        callback,
                        userData
                    )
                }
            }
            setState(.initialized(handle))
        } catch {
            resetState()
            throw error
        }
    }

    /// Set the identity the linked workload registers with.
    ///
    /// `actorType` must use the `publisher/program@version` representation.
    /// Call after `fromConfig(path:)` and before `linkWorkload(name:)`;
    /// without this call the node registers with the placeholder identity
    /// `local/Client@0.0.0`, which actrix ACL filtering rejects. On failure
    /// the node is reset and must be rebuilt with `fromConfig(path:)`.
    public func withActorType(_ actorType: String) throws {
        let initHandle = try withLockedState { state -> UnsafeMutableRawPointer in
            guard case let .initialized(handle) = state else {
                throw ActrCabiError.invalidState(expected: "initialized")
            }
            state = .initializing
            return handle
        }
        defer { actr_release_handle(initHandle) }
        let newHandle = actorType.withCString { typePtr in
            actr_node_with_actor_type(initHandle, typePtr, actorType.utf8.count)
        }
        guard let newHandle else {
            resetState()
            throw ActrCabiError.generic(
                code: -50,
                message: "invalid actor type or init handle: \(actorType)"
            )
        }
        setState(.initialized(newHandle))
    }

    public func linkWorkload(name: String) async throws {
        let initHandle = try withLockedState { state -> UnsafeMutableRawPointer in
            guard case let .initialized(handle) = state else {
                throw ActrCabiError.invalidState(expected: "initialized")
            }
            state = .linking
            return handle
        }

        let workloadHandle = name.withCString { namePtr in
            actr_workload_create(namePtr, name.utf8.count)
        }
        guard let workloadHandle else {
            setState(.initialized(initHandle))
            throw ActrCabiError.generic(code: -20, message: "workload not found: \(name)")
        }
        defer {
            // Native transitions consume these tokens. Release is idempotent,
            // so this also cleans them up if validation rejected either input.
            actr_release_handle(initHandle)
            actr_release_handle(workloadHandle)
        }

        do {
            let attachedHandle = try await withHandle { callback, userData in
                _ = actr_node_link(initHandle, workloadHandle, callback, userData)
            }
            setState(.attached(attachedHandle))
        } catch {
            // A valid transition call consumes both native input tokens even
            // when the asynchronous link operation fails.
            resetState()
            throw error
        }
    }

    public func withObserver(_ callbacks: ActrCabiObserverCallbacks) throws {
        let attachedHandle = try withLockedState { state -> UnsafeMutableRawPointer in
            guard case let .attached(handle) = state else {
                throw ActrCabiError.invalidState(expected: "attached")
            }
            state = .linking
            return handle
        }
        defer { actr_release_handle(attachedHandle) }
        let nativeCallbacks = actr_observer_callbacks(
            user_data: callbacks.userData,
            on_signaling_connecting: callbacks.onSignalingConnecting,
            on_signaling_connected: callbacks.onSignalingConnected,
            on_signaling_disconnected: callbacks.onSignalingDisconnected,
            on_error: callbacks.onError
        )
        guard let newHandle = actr_node_with_observer(attachedHandle, nativeCallbacks) else {
            resetState()
            throw ActrCabiError.handleInvalid
        }
        withLockedState { state in
            state = .attached(newHandle)
            if let owner = callbacks.lifetimeOwner {
                observerOwners.append(owner)
            }
        }
    }

    public func register(issuerEndpoint: String? = nil) async throws {
        let attachedHandle = try withLockedState { state -> UnsafeMutableRawPointer in
            guard case let .attached(handle) = state else {
                throw ActrCabiError.invalidState(expected: "attached")
            }
            state = .registering
            return handle
        }
        defer { actr_release_handle(attachedHandle) }

        do {
            let registeredHandle = try await withHandle { callback, userData in
                if let issuerEndpoint {
                    issuerEndpoint.withCString { endpointPtr in
                        _ = actr_node_register(
                            attachedHandle,
                            endpointPtr,
                            issuerEndpoint.utf8.count,
                            callback,
                            userData
                        )
                    }
                } else {
                    _ = actr_node_register(attachedHandle, nil, 0, callback, userData)
                }
            }
            setState(.registered(registeredHandle))
        } catch {
            resetState()
            throw error
        }
    }

    public func start() async throws {
        let registeredHandle = try withLockedState { state -> UnsafeMutableRawPointer in
            guard case let .registered(handle) = state else {
                throw ActrCabiError.invalidState(expected: "registered")
            }
            state = .starting
            return handle
        }
        defer { actr_release_handle(registeredHandle) }

        do {
            let refHandle = try await withHandle { callback, userData in
                _ = actr_node_start(registeredHandle, callback, userData)
            }
            setState(.started(refHandle))
        } catch {
            resetState()
            throw error
        }
    }

    public func call(
        routeKey: String,
        payload: Data,
        timeoutMs: Int64 = 30_000
    ) async throws -> Data {
        let refHandle = try currentRefHandle()
        return try await withCheckedThrowingContinuation { continuation in
            let key = ContinuationMap.shared.register(continuation)
            let userData = keyToUserData(key)
            routeKey.withCString { routeKeyPtr in
                payload.withUnsafeBytes { buffer in
                    let payloadPtr = buffer.baseAddress?.assumingMemoryBound(to: UInt8.self)
                    _ = actr_ref_call(
                        refHandle,
                        routeKeyPtr,
                        routeKey.utf8.count,
                        payloadPtr,
                        payload.count,
                        timeoutMs,
                        callTrampoline,
                        userData
                    )
                }
            }
        }
    }

    public func discover(targetType: String) async throws -> String {
        let refHandle = try currentRefHandle()
        return try await withCheckedThrowingContinuation { continuation in
            let key = ContinuationMap.shared.register(continuation)
            let userData = keyToUserData(key)
            targetType.withCString { targetTypePtr in
                _ = actr_ref_discover(
                    refHandle,
                    targetTypePtr,
                    targetType.utf8.count,
                    discoverTrampoline,
                    userData
                )
            }
        }
    }

    public func shutdown() throws {
        let refHandle = try currentRefHandle()
        let code = actr_ref_shutdown(refHandle)
        if code != 0 {
            throw ActrCabiError.generic(code: code, message: "actr_ref_shutdown failed")
        }
    }

    public func waitShutdown() async throws {
        let refHandle = try withLockedState { state -> UnsafeMutableRawPointer in
            guard case let .started(handle) = state else {
                throw ActrCabiError.invalidState(expected: "started")
            }
            state = .waitingForShutdown(handle)
            return handle
        }

        do {
            try await withCheckedThrowingContinuation {
                (continuation: CheckedContinuation<Void, Error>) in
                let key = ContinuationMap.shared.register(continuation)
                _ = actr_ref_wait_shutdown(
                    refHandle,
                    voidTrampoline,
                    keyToUserData(key)
                )
            }
            actr_release_actr_ref(refHandle)
            resetState()
        } catch {
            setState(.started(refHandle))
            throw error
        }
    }

    deinit {
        let handle: (UnsafeMutableRawPointer, Bool)? = withLockedState { state in
            switch state {
            case let .initialized(handle), let .attached(handle), let .registered(handle):
                return (handle, false)
            case let .started(handle), let .waitingForShutdown(handle):
                return (handle, true)
            case .empty, .initializing, .linking, .registering, .starting:
                return nil
            }
        }
        if let (handle, isRef) = handle {
            if isRef {
                let cleanup = ObserverLifetimeCleanup(owners: observerOwners)
                let cleanupData = Unmanaged.passRetained(cleanup).toOpaque()
                _ = actr_ref_shutdown(handle)
                _ = actr_ref_wait_shutdown(
                    handle,
                    observerCleanupTrampoline,
                    cleanupData
                )
                actr_release_actr_ref(handle)
            } else {
                actr_release_handle(handle)
            }
        }
    }

    private func currentRefHandle() throws -> UnsafeMutableRawPointer {
        try withLockedState { state in
            guard case let .started(handle) = state else {
                throw ActrCabiError.invalidState(expected: "started")
            }
            return handle
        }
    }

    private func setState(_ newState: State) {
        withLockedState { state in state = newState }
    }

    private func resetState() {
        withLockedState { state in
            state = .empty
            observerOwners.removeAll()
        }
    }

    @discardableResult
    private func withLockedState<T>(_ body: (inout State) throws -> T) rethrows -> T {
        stateLock.lock()
        defer { stateLock.unlock() }
        return try body(&state)
    }

    private func withHandle(
        _ body: (actr_handle_callback, UnsafeMutableRawPointer) -> Void
    ) async throws -> UnsafeMutableRawPointer {
        let box: HandleBox = try await withCheckedThrowingContinuation { continuation in
            let key = ContinuationMap.shared.register(continuation)
            body(handleTrampoline, keyToUserData(key))
        }
        return box.ptr
    }
}
