import Foundation

/// Resolves unique Metro bundler address based on process ID
/// Uses 127.x.x.x address space to avoid port conflicts between Canvas instances
struct MetroAddressResolver {

    /// Fixed Metro port
    static let port: UInt16 = 8081

    /// Generate unique 127.x.x.x address from process ID
    /// Uses Knuth multiplicative hash for uniform distribution
    static func address(for pid: pid_t) -> String {
        // Knuth multiplicative hash for uniform distribution
        var hash = UInt32(bitPattern: Int32(pid)) &* 2654435761
        hash ^= hash >> 16  // Additional mixing

        // Map to 127.1-254.0-255.1-254 (avoiding edge cases)
        let b1 = UInt8(clamping: ((hash >> 16) & 0xFF) % 253 + 1)  // 1-254
        let b2 = UInt8((hash >> 8) & 0xFF)                         // 0-255
        let b3 = UInt8(clamping: (hash & 0xFF) % 253 + 1)          // 1-254

        return "127.\(b1).\(b2).\(b3)"
    }

    /// Current process's Metro address
    static var currentAddress: String {
        address(for: getpid())
    }

    /// Current process's Metro host string (address:port)
    static var currentHost: String {
        "\(currentAddress):\(port)"
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
