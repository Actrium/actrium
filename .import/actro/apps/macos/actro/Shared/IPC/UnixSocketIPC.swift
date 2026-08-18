//
//  UnixSocketIPC.swift
//  Actro
//
//  Unix Domain Socket IPC layer for Actro <-> Helper communication.
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
    static func forTab(_ tabId: String) -> String {
        let processId = ProcessInfo.processInfo.processIdentifier
        return String(
            format: "/tmp/actro-%d-%016llx.sock",
            processId,
            stableHash(tabId)
        )
    }

    /// Clean up socket file
    static func cleanup(_ path: String) {
        try? FileManager.default.removeItem(atPath: path)
    }

    private static func stableHash(_ value: String) -> UInt64 {
        var hash: UInt64 = 14_695_981_039_346_656_037
        for byte in value.utf8 {
            hash ^= UInt64(byte)
            hash &*= 1_099_511_628_211
        }
        return hash
    }
}

// MARK: - IPC Server (Actro side)

/// Unix Socket server for receiving connections from Helper processes
class IPCServer {
    private var serverSocket: Int32 = -1
    private var clientSocket: Int32 = -1
    private let socketPath: String?
    private let connectedSocket: Int32?
    private let queue = DispatchQueue(label: "io.actrium.actro.ipc.server")
    private let sendLock = NSLock()
    private var isRunning = false

    var onMessage: ((Data) -> Void)?
    var onConnected: (() -> Void)?
    var onDisconnected: (() -> Void)?

    init(socketPath: String) {
        self.socketPath = socketPath
        self.connectedSocket = nil
    }

    /// Create the Broker endpoint around one side of an already-connected
    /// `socketpair`. The descriptor is owned by this instance after `start()`.
    init(connectedSocket: Int32) {
        self.socketPath = nil
        self.connectedSocket = connectedSocket
    }

    deinit {
        stop()
    }

    /// Start listening for connections
    func start() throws {
        if let connectedSocket {
            guard connectedSocket >= 0 else {
                throw IPCError.socketCreationFailed(EBADF)
            }
            try configureConnectedSocket(connectedSocket)
            clientSocket = connectedSocket
            isRunning = true
            queue.async { [weak self] in
                self?.readLoop()
            }
            DispatchQueue.main.async { [weak self] in
                self?.onConnected?()
            }
            NSLog("IPCServer: Using inherited socketpair endpoint")
            return
        }

        guard let socketPath else {
            throw IPCError.socketCreationFailed(EINVAL)
        }

        // Clean up any existing socket file
        IPCSocketPath.cleanup(socketPath)

        // Create socket
        serverSocket = socket(AF_UNIX, SOCK_STREAM, 0)
        guard serverSocket >= 0 else {
            throw IPCError.socketCreationFailed(errno)
        }
        try configureConnectedSocket(serverSocket)

        // Bind to path
        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)

        let pathBytes = socketPath.utf8CString
        guard pathBytes.count <= MemoryLayout.size(ofValue: addr.sun_path) else {
            throw IPCError.pathTooLong
        }

        withUnsafeMutablePointer(to: &addr.sun_path) { ptr in
            ptr.withMemoryRebound(to: CChar.self, capacity: pathBytes.count) { dest in
                for (i, byte) in pathBytes.enumerated() {
                    dest[i] = byte
                }
            }
        }

        let bindResult = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPtr in
                bind(serverSocket, sockaddrPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }

        guard bindResult >= 0 else {
            close(serverSocket)
            throw IPCError.bindFailed(errno)
        }

        // Listen
        guard listen(serverSocket, 1) >= 0 else {
            close(serverSocket)
            throw IPCError.listenFailed(errno)
        }

        isRunning = true

        // Accept connections in background
        queue.async { [weak self] in
            self?.acceptLoop()
        }

        NSLog("IPCServer: Listening on \(socketPath)")
    }

    /// Stop the server
    func stop() {
        isRunning = false

        if clientSocket >= 0 {
            close(clientSocket)
            clientSocket = -1
        }

        if serverSocket >= 0 {
            close(serverSocket)
            serverSocket = -1
        }

        if let socketPath {
            IPCSocketPath.cleanup(socketPath)
        }
    }

    /// Send a message to the connected client
    func send(_ data: Data) throws {
        guard clientSocket >= 0 else {
            throw IPCError.notConnected
        }

        sendLock.lock()
        defer { sendLock.unlock() }
        try sendFrame(to: clientSocket, data: data)
    }

    // MARK: - Private

    private func acceptLoop() {
        while isRunning && serverSocket >= 0 {
            var clientAddr = sockaddr_un()
            var clientAddrLen = socklen_t(MemoryLayout<sockaddr_un>.size)

            let newClient = withUnsafeMutablePointer(to: &clientAddr) { ptr in
                ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPtr in
                    accept(serverSocket, sockaddrPtr, &clientAddrLen)
                }
            }

            guard newClient >= 0 else {
                if isRunning {
                    NSLog("IPCServer: Accept failed: \(errno)")
                }
                continue
            }
            do {
                try configureConnectedSocket(newClient)
            } catch {
                close(newClient)
                NSLog("IPCServer: Failed to configure client socket: \(error)")
                continue
            }

            NSLog("IPCServer: Client connected")
            clientSocket = newClient

            DispatchQueue.main.async { [weak self] in
                self?.onConnected?()
            }

            readLoop()
        }
    }

    private func readLoop() {
        while isRunning && clientSocket >= 0 {
            do {
                let data = try readFrame(from: clientSocket)
                DispatchQueue.main.async { [weak self] in
                    self?.onMessage?(data)
                }
            } catch {
                NSLog("IPCServer: Read error: \(error)")
                break
            }
        }

        if clientSocket >= 0 {
            close(clientSocket)
            clientSocket = -1
        }

        DispatchQueue.main.async { [weak self] in
            self?.onDisconnected?()
        }
    }
}

// MARK: - IPC Client (Helper side)

/// Unix Socket client for connecting to Actro
class IPCClient {
    private var socket: Int32 = -1
    private let socketPath: String?
    private let connectedSocket: Int32?
    private let queue = DispatchQueue(label: "io.actrium.actro.ipc.client")
    private let sendLock = NSLock()
    private var isConnected = false

    var onMessage: ((Data) -> Void)?
    var onDisconnected: (() -> Void)?

    init(socketPath: String) {
        self.socketPath = socketPath
        self.connectedSocket = nil
    }

    /// Create the Helper endpoint around an inherited, already-connected file
    /// descriptor (normally standard input supplied by the Broker).
    init(connectedSocket: Int32) {
        self.socketPath = nil
        self.connectedSocket = connectedSocket
    }

    deinit {
        disconnect()
    }

    /// Connect to the server
    func connect() throws {
        if let connectedSocket {
            guard connectedSocket >= 0 else {
                throw IPCError.socketCreationFailed(EBADF)
            }
            try configureConnectedSocket(connectedSocket)
            socket = connectedSocket
            isConnected = true
            NSLog("IPCClient: Using inherited socketpair endpoint")
            queue.async { [weak self] in
                self?.readLoop()
            }
            return
        }

        guard let socketPath else {
            throw IPCError.socketCreationFailed(EINVAL)
        }

        socket = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard socket >= 0 else {
            throw IPCError.socketCreationFailed(errno)
        }
        try configureConnectedSocket(socket)

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)

        let pathBytes = socketPath.utf8CString
        guard pathBytes.count <= MemoryLayout.size(ofValue: addr.sun_path) else {
            close(socket)
            socket = -1
            throw IPCError.pathTooLong
        }

        withUnsafeMutablePointer(to: &addr.sun_path) { ptr in
            ptr.withMemoryRebound(to: CChar.self, capacity: pathBytes.count) { dest in
                for (i, byte) in pathBytes.enumerated() {
                    dest[i] = byte
                }
            }
        }

        let connectResult = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPtr in
                Darwin.connect(socket, sockaddrPtr, socklen_t(MemoryLayout<sockaddr_un>.size))
            }
        }

        guard connectResult >= 0 else {
            close(socket)
            socket = -1
            throw IPCError.connectFailed(errno)
        }

        isConnected = true
        NSLog("IPCClient: Connected to \(socketPath)")

        // Start reading in background
        queue.async { [weak self] in
            self?.readLoop()
        }
    }

    /// Disconnect from the server
    func disconnect() {
        isConnected = false

        if socket >= 0 {
            close(socket)
            socket = -1
        }
    }

    /// Send a message to the server
    func send(_ data: Data) throws {
        guard socket >= 0 && isConnected else {
            throw IPCError.notConnected
        }

        sendLock.lock()
        defer { sendLock.unlock() }
        try sendFrame(to: socket, data: data)
    }

    // MARK: - Private

    private func readLoop() {
        while isConnected && socket >= 0 {
            do {
                let data = try readFrame(from: socket)
                DispatchQueue.main.async { [weak self] in
                    self?.onMessage?(data)
                }
            } catch {
                NSLog("IPCClient: Read error: \(error)")
                break
            }
        }

        isConnected = false

        DispatchQueue.main.async { [weak self] in
            self?.onDisconnected?()
        }
    }
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

/// Read a length-prefixed frame
private func readFrame(from fd: Int32) throws -> Data {
    // Read length (4 bytes, big-endian)
    var lengthBE: UInt32 = 0
    let lengthRead = withUnsafeMutableBytes(of: &lengthBE) { ptr in
        read(fd, ptr.baseAddress!, 4)
    }
    guard lengthRead == 4 else {
        throw IPCError.connectionClosed
    }

    let length = Int(UInt32(bigEndian: lengthBE))
    guard length > 0 && length < 10_000_000 else {  // Max 10MB
        throw IPCError.invalidFrameLength
    }

    // Read data
    var data = Data(count: length)
    let dataRead = data.withUnsafeMutableBytes { ptr in
        read(fd, ptr.baseAddress!, length)
    }
    guard dataRead == length else {
        throw IPCError.connectionClosed
    }

    return data
}

// MARK: - Errors

enum IPCError: Error, LocalizedError {
    case socketCreationFailed(Int32)
    case pathTooLong
    case bindFailed(Int32)
    case listenFailed(Int32)
    case connectFailed(Int32)
    case notConnected
    case writeFailed(Int32)
    case connectionClosed
    case invalidFrameLength

    var errorDescription: String? {
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
