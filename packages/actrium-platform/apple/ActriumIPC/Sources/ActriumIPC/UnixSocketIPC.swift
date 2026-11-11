//
//  UnixSocketIPC.swift
//  Actrium
//
//  Unix Domain Socket IPC layer for Actrium <-> Helper communication.
//  Protocol: Length-prefixed Protobuf messages
//
//  Frame format:
//  +----------------+------------------+
//  | Length (4 bytes, big-endian) | Protobuf data |
//  +----------------+------------------+
//

import Foundation
import Darwin

// MARK: - Socket Path Helper

enum IPCSocketPath {
    /// Generate a deterministic, short socket path for a logical connection.
    ///
    /// Darwin's `sockaddr_un.sun_path` is only 104 bytes. Using
    /// `FileManager.temporaryDirectory` is unsafe because its expanded sandbox
    /// path can already consume most of that budget. Both Actrium and the
    /// Renderer are entitled to `/tmp`, so keep the filesystem prefix fixed and
    /// hash the caller-provided identifier.
    public static func forTab(_ tabId: String) -> String {
        let processId = ProcessInfo.processInfo.processIdentifier
        return String(
            format: "/tmp/actrium-%d-%016llx.sock",
            processId,
            stableHash(tabId)
        )
    }

    /// Clean up socket file
    static func cleanup(_ path: String) {
        try? FileManager.default.removeItem(atPath: path)
    }

    /// FNV-1a gives a stable cross-process value without pulling a crypto
    /// framework into the small IPC package.
    private static func stableHash(_ value: String) -> UInt64 {
        var hash: UInt64 = 14_695_981_039_346_656_037
        for byte in value.utf8 {
            hash ^= UInt64(byte)
            hash &*= 1_099_511_628_211
        }
        return hash
    }
}

// MARK: - Socket ownership

/// The descriptor stays open until all in-flight I/O releases this object.
/// Only `stopped` is mutable, protected by `stateLock`; complete writes are
/// serialized by `sendLock`. Shutdown wakes I/O without recycling its descriptor.
private final class IPCSocket: @unchecked Sendable {
    let descriptor: Int32
    private let stateLock = NSLock()
    private let sendLock = NSLock()
    private var stopped = false

    init(_ descriptor: Int32) {
        self.descriptor = descriptor
    }

    deinit {
        Darwin.close(descriptor)
    }

    var isStopped: Bool {
        stateLock.withLock { stopped }
    }

    func stop() {
        stateLock.withLock {
            guard !stopped else { return }
            stopped = true
            // The read/write owner closes the descriptor after its syscall exits.
            _ = Darwin.shutdown(descriptor, SHUT_RDWR)
        }
    }

    func send(_ data: Data) throws {
        try sendLock.withLock {
            guard !isStopped else { throw IPCError.notConnected }
            try sendFrame(to: descriptor, data: data)
        }
    }

    func acceptClient() throws -> IPCSocket {
        // A bounded poll also handles platforms where shutdown does not wake a
        // listening socket. Nonblocking accept cannot outlive that cancellation.
        while !isStopped {
            var event = pollfd(fd: descriptor, events: Int16(POLLIN), revents: 0)
            let ready = Darwin.poll(&event, 1, 100)
            if ready == 0 || (ready < 0 && errno == EINTR) { continue }
            guard ready > 0, !isStopped else { throw IPCError.connectionClosed }
            let accepted = Darwin.accept(descriptor, nil, nil)
            if accepted < 0 && (errno == EAGAIN || errno == EINTR) { continue }
            guard accepted >= 0 else { throw IPCError.connectionClosed }
            let socket = IPCSocket(accepted)
            let flags = fcntl(accepted, F_GETFL)
            guard flags >= 0, fcntl(accepted, F_SETFL, flags & ~O_NONBLOCK) >= 0 else {
                throw IPCError.socketCreationFailed(errno)
            }
            try configureConnectedSocket(accepted)
            return socket
        }
        throw IPCError.connectionClosed
    }
}

/// Bridges synchronous sends from any thread to the main-actor lifecycle.
/// Every access to the current socket is protected by the lock. I/O keeps a
/// strong snapshot so replacement cannot close a descriptor in use.
private final class IPCSocketSlot: @unchecked Sendable {
    private let lock = NSLock()
    private var socket: IPCSocket?

    deinit { replace(with: nil) }

    func replace(with newSocket: IPCSocket?) {
        let previous = lock.withLock {
            let previous = socket
            socket = newSocket
            return previous
        }
        previous?.stop()
    }

    func send(_ data: Data) throws {
        guard let current = lock.withLock({ socket }) else { throw IPCError.notConnected }
        try current.send(data)
    }
}

// MARK: - IPC Server (Actrium side)

/// Lifecycle and callbacks are main-actor isolated; blocking I/O owns a socket snapshot.
@MainActor
final class IPCServer {
    private let socketPath: String?
    private var inheritedSocket: IPCSocket?
    private var listener: IPCSocket?
    private var client: IPCSocket?
    private nonisolated let outgoing = IPCSocketSlot()
    private let queue = DispatchQueue(label: "io.actrium.ipc.server")

    var onMessage: ((Data) -> Void)?
    var onConnected: (() -> Void)?
    var onDisconnected: (() -> Void)?

    init(socketPath: String) { self.socketPath = socketPath }

    init(connectedSocket: Int32) {
        self.socketPath = nil
        self.inheritedSocket = connectedSocket >= 0 ? IPCSocket(connectedSocket) : nil
    }

    deinit {
        listener?.stop()
        client?.stop()
        if listener != nil, let socketPath { IPCSocketPath.cleanup(socketPath) }
    }

    func start() throws {
        guard listener == nil, client == nil else { return }
        if let inheritedSocket {
            self.inheritedSocket = nil
            try configureConnectedSocket(inheritedSocket.descriptor)
            activate(inheritedSocket)
            return
        }
        guard let socketPath else { throw IPCError.notConnected }
        IPCSocketPath.cleanup(socketPath)
        let descriptor = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard descriptor >= 0 else { throw IPCError.socketCreationFailed(errno) }
        let socket = IPCSocket(descriptor)
        try configureConnectedSocket(descriptor)
        var address = try socketAddress(socketPath)
        let result = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.bind(descriptor, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }
        guard result >= 0 else { throw IPCError.bindFailed(errno) }
        guard Darwin.listen(descriptor, 1) >= 0 else { throw IPCError.listenFailed(errno) }
        let flags = fcntl(descriptor, F_GETFL)
        guard flags >= 0, fcntl(descriptor, F_SETFL, flags | O_NONBLOCK) >= 0 else {
            throw IPCError.socketCreationFailed(errno)
        }
        listener = socket
        acceptNext(socket)
    }

    func stop() {
        listener?.stop()
        listener = nil
        outgoing.replace(with: nil)
        client = nil
        inheritedSocket = nil
        if let socketPath { IPCSocketPath.cleanup(socketPath) }
    }

    nonisolated func send(_ data: Data) throws { try outgoing.send(data) }

    private func acceptNext(_ socket: IPCSocket) {
        queue.async { [weak self] in
            do {
                let accepted = try socket.acceptClient()
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.listener === socket else { return }
                    self.activate(accepted)
                }
            } catch {
                DispatchQueue.main.async { [weak self] in
                    guard let self, self.listener === socket else { return }
                    self.stop()
                    self.onDisconnected?()
                }
            }
        }
    }

    private func activate(_ socket: IPCSocket) {
        client = socket
        outgoing.replace(with: socket)
        queue.async { [weak self] in
            do {
                while !socket.isStopped {
                    let data = try readFrame(from: socket.descriptor)
                    DispatchQueue.main.async { [weak self] in
                        guard let self, self.client === socket else { return }
                        self.onMessage?(data)
                    }
                }
            } catch {
                // EOF and local shutdown both finish this socket generation.
            }
            socket.stop()
            DispatchQueue.main.async { [weak self] in
                guard let self, self.client === socket else { return }
                self.outgoing.replace(with: nil)
                self.client = nil
                let listener = self.listener
                self.onDisconnected?()
                // A callback may stop or restart the server; do not start a
                // second accept loop for the replacement listener.
                if let listener, self.listener === listener, self.client == nil {
                    self.acceptNext(listener)
                }
            }
        }
        onConnected?()
    }
}

// MARK: - IPC Client (Helper side)

@MainActor
final class IPCClient {
    private let socketPath: String?
    private var inheritedSocket: IPCSocket?
    private var connection: IPCSocket?
    private nonisolated let outgoing = IPCSocketSlot()
    private let queue = DispatchQueue(label: "io.actrium.ipc.client")

    var onMessage: ((Data) -> Void)?
    var onDisconnected: (() -> Void)?

    init(socketPath: String) { self.socketPath = socketPath }

    init(connectedSocket: Int32) {
        self.socketPath = nil
        self.inheritedSocket = connectedSocket >= 0 ? IPCSocket(connectedSocket) : nil
    }

    deinit { connection?.stop() }

    func connect() throws {
        guard connection == nil else { return }
        let socket: IPCSocket
        if let inheritedSocket {
            self.inheritedSocket = nil
            socket = inheritedSocket
        } else {
            guard let socketPath else { throw IPCError.notConnected }
            let descriptor = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
            guard descriptor >= 0 else { throw IPCError.socketCreationFailed(errno) }
            socket = IPCSocket(descriptor)
            var address = try socketAddress(socketPath)
            let result = withUnsafePointer(to: &address) {
                $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                    Darwin.connect(descriptor, $0, socklen_t(MemoryLayout<sockaddr_un>.size))
                }
            }
            guard result >= 0 else { throw IPCError.connectFailed(errno) }
        }
        try configureConnectedSocket(socket.descriptor)
        connection = socket
        outgoing.replace(with: socket)
        queue.async { [weak self] in
            do {
                while !socket.isStopped {
                    let data = try readFrame(from: socket.descriptor)
                    DispatchQueue.main.async { [weak self] in
                        guard let self, self.connection === socket else { return }
                        self.onMessage?(data)
                    }
                }
            } catch {
                // EOF and local shutdown both finish this socket generation.
            }
            socket.stop()
            DispatchQueue.main.async { [weak self] in
                guard let self, self.connection === socket else { return }
                self.outgoing.replace(with: nil)
                self.connection = nil
                self.onDisconnected?()
            }
        }
    }

    func disconnect() {
        outgoing.replace(with: nil)
        connection = nil
        inheritedSocket = nil
    }

    nonisolated func send(_ data: Data) throws { try outgoing.send(data) }
}

private func socketAddress(_ path: String) throws -> sockaddr_un {
    var address = sockaddr_un()
    address.sun_family = sa_family_t(AF_UNIX)
    let bytes = path.utf8CString
    guard bytes.count <= MemoryLayout.size(ofValue: address.sun_path) else {
        throw IPCError.pathTooLong
    }
    withUnsafeMutablePointer(to: &address.sun_path) {
        $0.withMemoryRebound(to: CChar.self, capacity: bytes.count) { destination in
            for (index, byte) in bytes.enumerated() { destination[index] = byte }
        }
    }
    return address
}

// MARK: - Frame I/O Helpers

/// Send a length-prefixed frame
private func sendFrame(to fd: Int32, data: Data) throws {
    // Write length (4 bytes, big-endian)
    var length = UInt32(data.count).bigEndian
    try withUnsafeBytes(of: &length) { ptr in
        try writeExactly(to: fd, buffer: ptr.baseAddress!, count: 4)
    }

    // Write data
    try data.withUnsafeBytes { ptr in
        try writeExactly(to: fd, buffer: ptr.baseAddress!, count: data.count)
    }
}

/// Prevent process-wide SIGPIPE termination and handle short writes correctly.
private func configureConnectedSocket(_ fd: Int32) throws {
    var enabled: Int32 = 1
    let result = withUnsafePointer(to: &enabled) {
        setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, $0, socklen_t(MemoryLayout<Int32>.size))
    }
    guard result == 0 else {
        throw IPCError.socketCreationFailed(errno)
    }
}

private func writeExactly(to fd: Int32, buffer: UnsafeRawPointer, count: Int) throws {
    var totalWritten = 0
    while totalWritten < count {
        let written = Darwin.write(
            fd,
            buffer.advanced(by: totalWritten),
            count - totalWritten
        )
        if written < 0, errno == EINTR {
            continue
        }
        guard written > 0 else {
            throw IPCError.writeFailed(errno)
        }
        totalWritten += written
    }
}

/// Read exactly `count` bytes from fd, handling short reads
private func readExactly(from fd: Int32, buffer: UnsafeMutableRawPointer, count: Int) throws {
    var totalRead = 0
    while totalRead < count {
        let bytesRead = read(fd, buffer.advanced(by: totalRead), count - totalRead)
        if bytesRead < 0 && errno == EINTR { continue }
        if bytesRead <= 0 {
            // 0 = EOF, -1 = error
            throw IPCError.connectionClosed
        }
        totalRead += bytesRead
    }
}

/// Read a length-prefixed frame
private func readFrame(from fd: Int32) throws -> Data {
    // Read length (4 bytes, big-endian)
    var lengthBE: UInt32 = 0
    try withUnsafeMutableBytes(of: &lengthBE) { ptr in
        try readExactly(from: fd, buffer: ptr.baseAddress!, count: 4)
    }

    let length = Int(UInt32(bigEndian: lengthBE))
    guard length > 0 && length < 10_000_000 else {  // Max 10MB
        throw IPCError.invalidFrameLength
    }

    // Read data
    var data = Data(count: length)
    try data.withUnsafeMutableBytes { ptr in
        try readExactly(from: fd, buffer: ptr.baseAddress!, count: length)
    }

    return data
}

// MARK: - Errors

public enum IPCError: Error, LocalizedError {
    case socketCreationFailed(Int32)
    case pathTooLong
    case bindFailed(Int32)
    case listenFailed(Int32)
    case connectFailed(Int32)
    case notConnected
    case writeFailed(Int32)
    case connectionClosed
    case invalidFrameLength

    public var errorDescription: String? {
        switch self {
        case .socketCreationFailed(let errno):
            return "Socket creation failed: \(errno)"
        case .pathTooLong:
            return "Socket path too long"
        case .bindFailed(let errno):
            return "Bind failed: \(errno)"
        case .listenFailed(let errno):
            return "Listen failed: \(errno)"
        case .connectFailed(let errno):
            return "Connect failed: \(errno)"
        case .notConnected:
            return "Not connected"
        case .writeFailed(let errno):
            return "Write failed: \(errno)"
        case .connectionClosed:
            return "Connection closed"
        case .invalidFrameLength:
            return "Invalid frame length"
        }
    }
}
