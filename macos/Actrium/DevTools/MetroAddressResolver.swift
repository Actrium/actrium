import Foundation
import Network

/// Metro bundler address utilities
/// Finds an available port to avoid conflicts between Actrium instances
struct MetroAddressResolver {

    /// Always use localhost
    static let address: String = "127.0.0.1"

    /// Find an available port by binding to port 0 and checking the assigned port
    static func findAvailablePort() -> UInt16 {
        var port: UInt16 = 8081  // Default fallback

        // Create a socket and bind to port 0 to get an available port
        let socketFD = socket(AF_INET, SOCK_STREAM, 0)
        guard socketFD >= 0 else { return port }
        defer { close(socketFD) }

        var addr = sockaddr_in()
        addr.sin_family = sa_family_t(AF_INET)
        addr.sin_port = 0  // Let kernel assign
        addr.sin_addr.s_addr = inet_addr("127.0.0.1")

        let bindResult = withUnsafePointer(to: &addr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPtr in
                bind(socketFD, sockaddrPtr, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }

        guard bindResult == 0 else { return port }

        // Get the assigned port
        var assignedAddr = sockaddr_in()
        var addrLen = socklen_t(MemoryLayout<sockaddr_in>.size)

        let getsocknameResult = withUnsafeMutablePointer(to: &assignedAddr) { ptr in
            ptr.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPtr in
                getsockname(socketFD, sockaddrPtr, &addrLen)
            }
        }

        if getsocknameResult == 0 {
            port = UInt16(bigEndian: assignedAddr.sin_port)
        }

        return port
    }

    /// Build bundle URL for a given port
    static func bundleURL(port: UInt16) -> URL {
        URL(string: "http://\(address):\(port)/index.bundle?platform=macos&dev=true")!
    }

    /// Build base URL for a given port
    static func baseURL(port: UInt16) -> URL {
        URL(string: "http://\(address):\(port)")!
    }

    /// Build host string for a given port
    static func host(port: UInt16) -> String {
        "\(address):\(port)"
    }
}
