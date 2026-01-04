import Foundation

/// Resolves unique Metro bundler port based on process ID
/// Uses dynamic port allocation to avoid conflicts between Canvas instances
struct MetroAddressResolver {

    /// Base port range for Metro (49152-65535 is dynamic/private range)
    private static let basePort: UInt16 = 49152
    private static let portRange: UInt16 = 16383  // 49152 to 65535

    /// Generate unique port from process ID
    /// Uses hash to distribute ports across the dynamic range
    static func port(for pid: pid_t) -> UInt16 {
        // Knuth multiplicative hash for uniform distribution
        var hash = UInt32(bitPattern: Int32(pid)) &* 2654435761
        hash ^= hash >> 16  // Additional mixing

        // Map to port range 49152-65535
        let offset = UInt16(hash % UInt32(portRange))
        return basePort + offset
    }

    /// Always use localhost
    static let address: String = "127.0.0.1"

    /// Current process's Metro port
    static var currentPort: UInt16 {
        port(for: getpid())
    }

    /// Current process's Metro host string (address:port)
    static var currentHost: String {
        "\(address):\(currentPort)"
    }

    /// Current process's Metro base URL
    static var currentBaseURL: URL {
        URL(string: "http://\(currentHost)")!
    }

    /// Bundle URL for the main entry point
    static var mainBundleURL: URL {
        URL(string: "http://\(currentHost)/index.bundle?platform=macos&dev=true")!
    }

    /// Bundle URL for a specific guest app entry point
    static func guestBundleURL(appId: String) -> URL {
        URL(string: "http://\(currentHost)/guest/\(appId)/index.bundle?platform=macos&dev=true")!
    }
}
