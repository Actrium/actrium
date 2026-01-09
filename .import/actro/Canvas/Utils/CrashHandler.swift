import Foundation

/// Crash handler to log crash information before the app terminates
final class CrashHandler {
    private static var isSetup = false

    static func setup() {
        guard !isSetup else { return }
        isSetup = true

        NSLog("🛡️ [CrashHandler] Setting up crash handlers")

        // Handle uncaught Objective-C exceptions
        NSSetUncaughtExceptionHandler { exception in
            CrashHandler.logException(exception)
        }

        // Handle Unix signals
        setupSignalHandlers()
    }

    private static func setupSignalHandlers() {
        // Common crash signals
        let signals: [Int32] = [
            SIGABRT,  // Abort
            SIGILL,   // Illegal instruction
            SIGSEGV,  // Segmentation fault
            SIGFPE,   // Floating point exception
            SIGBUS,   // Bus error
            SIGPIPE,  // Broken pipe
            SIGTRAP   // Trap
        ]

        for sig in signals {
            signal(sig) { signalNumber in
                CrashHandler.logSignal(signalNumber)
            }
        }
    }

    private static func logException(_ exception: NSException) {
        let timestamp = ISO8601DateFormatter().string(from: Date())
        let name = exception.name.rawValue
        let reason = exception.reason ?? "Unknown"
        let callStack = exception.callStackSymbols.joined(separator: "\n")

        let message = """
        ========================================
        💥 CRASH - Uncaught Exception
        ========================================
        Time: \(timestamp)
        Name: \(name)
        Reason: \(reason)

        Call Stack:
        \(callStack)
        ========================================
        """

        NSLog("%@", message)

        // Also write to a crash log file
        writeCrashLog(message)
    }

    private static func logSignal(_ signal: Int32) {
        let timestamp = ISO8601DateFormatter().string(from: Date())
        let signalName = signalName(for: signal)

        // Get current thread's call stack
        let callStack = Thread.callStackSymbols.joined(separator: "\n")

        let message = """
        ========================================
        💥 CRASH - Signal Received
        ========================================
        Time: \(timestamp)
        Signal: \(signal) (\(signalName))

        Call Stack:
        \(callStack)
        ========================================
        """

        NSLog("%@", message)

        // Also write to a crash log file
        writeCrashLog(message)

        // Re-raise the signal to let the system handle it
        Darwin.signal(signal, SIG_DFL)
        Darwin.raise(signal)
    }

    private static func signalName(for signal: Int32) -> String {
        switch signal {
        case SIGABRT: return "SIGABRT (Abort)"
        case SIGILL: return "SIGILL (Illegal instruction)"
        case SIGSEGV: return "SIGSEGV (Segmentation fault)"
        case SIGFPE: return "SIGFPE (Floating point exception)"
        case SIGBUS: return "SIGBUS (Bus error)"
        case SIGPIPE: return "SIGPIPE (Broken pipe)"
        case SIGTRAP: return "SIGTRAP (Trap)"
        case SIGTERM: return "SIGTERM (Termination)"
        case SIGKILL: return "SIGKILL (Kill)"
        default: return "Unknown signal"
        }
    }

    private static func writeCrashLog(_ message: String) {
        let fileManager = FileManager.default
        let logsDir = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
            .appendingPathComponent("Canvas", isDirectory: true)
            .appendingPathComponent("CrashLogs", isDirectory: true)

        guard let logsDir = logsDir else { return }

        do {
            try fileManager.createDirectory(at: logsDir, withIntermediateDirectories: true)

            let timestamp = ISO8601DateFormatter().string(from: Date())
                .replacingOccurrences(of: ":", with: "-")
            let logFile = logsDir.appendingPathComponent("crash-\(timestamp).log")

            try message.write(to: logFile, atomically: true, encoding: .utf8)
            NSLog("📝 [CrashHandler] Crash log written to: \(logFile.path)")
        } catch {
            NSLog("⚠️ [CrashHandler] Failed to write crash log: \(error)")
        }
    }
}
