import Foundation
import OSLog

/// App-wide logging facade based on `os.Logger`.
///
/// - Provides a stable API surface with explicit log levels.
/// - Uses subsystem/category to make filtering in Console.app / Xcode easy.
/// - `trace`/`debug` are compiled out in non-DEBUG builds.
struct AppLogger {
    let logger: Logger

    init(category: String, subsystem: String = AppLogger.defaultSubsystem) {
        self.logger = Logger(subsystem: subsystem, category: category)
    }

    // MARK: - Levels

    /// Finest-grained logging. Compiled out in Release by default.
    @inline(__always)
    func trace(_ message: @autoclosure @escaping () -> String) {
        #if DEBUG
        logger.trace("\(message(), privacy: .public)")
        #endif
    }

    /// Debug logging. Compiled out in Release.
    @inline(__always)
    func debug(_ message: @autoclosure @escaping () -> String) {
        #if DEBUG
        logger.debug("\(message(), privacy: .public)")
        #endif
    }

    /// General informational messages.
    @inline(__always)
    func info(_ message: @autoclosure @escaping () -> String) {
        logger.info("\(message(), privacy: .public)")
    }

    /// Important informational messages.
    @inline(__always)
    func notice(_ message: @autoclosure @escaping () -> String) {
        logger.notice("\(message(), privacy: .public)")
    }

    /// Warnings that might require attention.
    @inline(__always)
    func warning(_ message: @autoclosure @escaping () -> String) {
        logger.warning("\(message(), privacy: .public)")
    }

    /// Errors that indicate an operation failed.
    @inline(__always)
    func error(_ message: @autoclosure @escaping () -> String) {
        logger.error("\(message(), privacy: .public)")
    }

    /// Severe/system-level failures.
    @inline(__always)
    func fault(_ message: @autoclosure @escaping () -> String) {
        logger.fault("\(message(), privacy: .public)")
    }

    // MARK: - Defaults

    static let defaultSubsystem: String = {
        Bundle.main.bundleIdentifier ?? "Canvas"
    }()
}

// MARK: - Common categories

extension AppLogger {
    static let app = AppLogger(category: "app")
    static let database = AppLogger(category: "database")
    static let reactNative = AppLogger(category: "react-native")
    static let viewModel = AppLogger(category: "view-model")
    static let ui = AppLogger(category: "ui")
}
