import Foundation

/// Metro bundler address utilities
/// Port is dynamically assigned by kernel (--port 0) and parsed from Metro output
struct MetroAddressResolver {

    /// Always use localhost
    static let address: String = "127.0.0.1"

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
