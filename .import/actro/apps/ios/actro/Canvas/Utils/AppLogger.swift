import Foundation
import OSLog

/// App-wide logging facade based on `os.Logger`.
///
/// - Provides a stable API surface with explicit log levels.
/// - Uses subsystem/category to make filtering in Console.app / Xcode easy.
/// - `trace`/`debug` are compiled out in non-DEBUG builds.
struct AppLogger {
    let logger: Logger
    let category: String

    init(category: String, subsystem: String = AppLogger.defaultSubsystem) {
        self.category = category
        self.logger = Logger(subsystem: subsystem, category: category)
    }

    // MARK: - Levels

    /// Finest-grained logging. Compiled out in Release by default.
    @inline(__always)
    func trace(_ message: @autoclosure @escaping () -> String, file: String = #file, line: Int = #line, function: String = #function) {
        #if DEBUG
        let msg = message()
        logger.trace("\(msg, privacy: .public)")
        AppLogger.mirrorToStderr(level: "TRACE", category: category, message: msg, file: file, line: line, function: function)
        #endif
    }

    /// Debug logging. Compiled out in Release.
    @inline(__always)
    func debug(_ message: @autoclosure @escaping () -> String, file: String = #file, line: Int = #line, function: String = #function) {
        #if DEBUG
        let msg = message()
        logger.debug("\(msg, privacy: .public)")
        AppLogger.mirrorToStderr(level: "DEBUG", category: category, message: msg, file: file, line: line, function: function)
        #endif
    }

    /// General informational messages.
    @inline(__always)
    func info(_ message: @autoclosure @escaping () -> String, file: String = #file, line: Int = #line, function: String = #function) {
        let msg = message()
        logger.info("\(msg, privacy: .public)")
        #if DEBUG
        AppLogger.mirrorToStderr(level: "INFO", category: category, message: msg, file: file, line: line, function: function)
        #endif
    }

    /// Important informational messages.
    @inline(__always)
    func notice(_ message: @autoclosure @escaping () -> String, file: String = #file, line: Int = #line, function: String = #function) {
        let msg = message()
        logger.notice("\(msg, privacy: .public)")
        #if DEBUG
        AppLogger.mirrorToStderr(level: "NOTICE", category: category, message: msg, file: file, line: line, function: function)
        #endif
    }

    /// Warnings that might require attention.
    @inline(__always)
    func warning(_ message: @autoclosure @escaping () -> String, file: String = #file, line: Int = #line, function: String = #function) {
        let msg = message()
        logger.warning("\(msg, privacy: .public)")
        #if DEBUG
        AppLogger.mirrorToStderr(level: "WARNING", category: category, message: msg, file: file, line: line, function: function)
        #endif
    }

    /// Errors that indicate an operation failed.
    @inline(__always)
    func error(_ message: @autoclosure @escaping () -> String, file: String = #file, line: Int = #line, function: String = #function) {
        let msg = message()
        logger.error("\(msg, privacy: .public)")
        #if DEBUG
        AppLogger.mirrorToStderr(level: "ERROR", category: category, message: msg, file: file, line: line, function: function)
        #endif
    }

    /// Severe/system-level failures.
    @inline(__always)
    func fault(_ message: @autoclosure @escaping () -> String, file: String = #file, line: Int = #line, function: String = #function) {
        let msg = message()
        logger.fault("\(msg, privacy: .public)")
        #if DEBUG
        AppLogger.mirrorToStderr(level: "FAULT", category: category, message: msg, file: file, line: line, function: function)
        #endif
    }

    // MARK: - Defaults

    static let defaultSubsystem: String = {
        Bundle.main.bundleIdentifier ?? "Canvas"
    }()

    #if DEBUG
    /// Mirror logs to Xcode Debug Console (stderr), since unified logging is not reliably shown there.
    private static func mirrorToStderr(level: String, category: String, message: String, file: String, line: Int, function: String) {
        // 时间格式化
        let formatter = DateFormatter()
        formatter.dateFormat = "yyyy-MM-dd HH:mm:ss.SSS"
        let timestamp = formatter.string(from: Date())
        
        // 提取文件名（去掉路径）
        let fileName = (file as NSString).lastPathComponent
        
        let meta = "[\(timestamp)] [\(level)] [\(fileName):\(line) \(function)] [\(category)]"
        fputs("\(meta) \(message)\n", stderr)
    }
    #endif
}

// MARK: - Common categories

extension AppLogger {
    static let app = AppLogger(category: "app")
    static let database = AppLogger(category: "database")
    static let reactNative = AppLogger(category: "react-native")
    static let viewModel = AppLogger(category: "view-model")
    static let ui = AppLogger(category: "ui")
}

let appLogger = AppLogger.app
let dbLogger = AppLogger.database
let rnLogger = AppLogger.reactNative
let vmLogger = AppLogger.viewModel
let uiLogger = AppLogger.ui
