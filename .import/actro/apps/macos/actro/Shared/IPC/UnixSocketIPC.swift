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

// MARK: - Socket Path Helper

enum IPCSocketPath {
    /// Generate a unique socket path for a tab
    static func forTab(_ tabId: String) -> String {
        let tmpDir = FileManager.default.temporaryDirectory.path
        return "\(tmpDir)/actro-\(tabId).sock"
    }

    /// Clean up socket file
    static func cleanup(_ path: String) {
        try? FileManager.default.removeItem(atPath: path)
    }
}

// MARK: - IPC Server (Actro side)

/// Unix Socket server for receiving connections from Helper processes
class IPCServer {
    private var serverSocket: Int32 = -1
    private var clientSocket: Int32 = -1
    private let socketPath: String
    private let queue = DispatchQueue(label: "chat.askaway.actro.ipc.server")
    private var isRunning = false

    var onMessage: ((Data) -> Void)?
    var onConnected: (() -> Void)?
    var onDisconnected: (() -> Void)?

    init(socketPath: String) {
        self.socketPath = socketPath
    }

    deinit {
        stop()
    }

    /// Start listening for connections
    func start() throws {
        // Clean up any existing socket file
        IPCSocketPath.cleanup(socketPath)

        // Create socket
        serverSocket = socket(AF_UNIX, SOCK_STREAM, 0)
        guard serverSocket >= 0 else {
            throw IPCError.socketCreationFailed(errno)
        }

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

        IPCSocketPath.cleanup(socketPath)
    }

    /// Send a message to the connected client
    func send(_ data: Data) throws {
        guard clientSocket >= 0 else {
            throw IPCError.notConnected
        }

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
    private let socketPath: String
    private let queue = DispatchQueue(label: "chat.askaway.actro.ipc.client")
    private var isConnected = false

    var onMessage: ((Data) -> Void)?
    var onDisconnected: (() -> Void)?

    init(socketPath: String) {
        self.socketPath = socketPath
    }

    deinit {
        disconnect()
    }

    /// Connect to the server
    func connect() throws {
        socket = Darwin.socket(AF_UNIX, SOCK_STREAM, 0)
        guard socket >= 0 else {
            throw IPCError.socketCreationFailed(errno)
        }

        var addr = sockaddr_un()
        addr.sun_family = sa_family_t(AF_UNIX)

        let pathBytes = socketPath.utf8CString
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
    let lengthWritten = withUnsafeBytes(of: &length) { ptr in
        write(fd, ptr.baseAddress!, 4)
    }
    guard lengthWritten == 4 else {
        throw IPCError.writeFailed(errno)
    }

    // Write data
    let dataWritten = data.withUnsafeBytes { ptr in
        write(fd, ptr.baseAddress!, data.count)
    }
    guard dataWritten == data.count else {
        throw IPCError.writeFailed(errno)
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
