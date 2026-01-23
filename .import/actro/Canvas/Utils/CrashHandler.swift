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
        // IMPORTANT: Signal handlers can only use async-signal-safe functions!
        // NSLog, malloc, and most standard library functions are NOT safe.
        // Use write() system call to stderr directly.

        // Write minimal crash info using only async-signal-safe functions
        writeSignalSafe("========================================\n")
        writeSignalSafe("CRASH - Signal Received: ")
        writeSignalNumber(signal)
        writeSignalSafe(" (")
        writeSignalName(for: signal)
        writeSignalSafe(")\n")
        writeSignalSafe("========================================\n")

        // Re-raise the signal to let the system handle it
        Darwin.signal(signal, SIG_DFL)
        Darwin.raise(signal)
    }

    /// Async-signal-safe write to stderr
    private static func writeSignalSafe(_ message: StaticString) {
        message.withUTF8Buffer { buffer in
            _ = write(STDERR_FILENO, buffer.baseAddress, buffer.count)
        }
    }

    /// Async-signal-safe write of signal number using stack buffer
    private static func writeSignalNumber(_ signal: Int32) {
        // Use fixed-size stack buffer (max 11 digits for Int32 + sign)
        var buffer: (CChar, CChar, CChar, CChar, CChar, CChar, CChar, CChar, CChar, CChar, CChar, CChar) =
            (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)

        var num = signal
        var idx = 10  // Start from end of buffer

        if num < 0 {
            num = -num
        }

        repeat {
            withUnsafeMutableBytes(of: &buffer) { ptr in
                ptr[idx] = UInt8(48 + num % 10)  // '0' = 48
            }
            num /= 10
            idx -= 1
        } while num > 0

        if signal < 0 {
            withUnsafeMutableBytes(of: &buffer) { ptr in
                ptr[idx] = UInt8(45)  // '-' = 45
            }
            idx -= 1
        }

        // Write from idx+1 to end
        withUnsafeBytes(of: &buffer) { ptr in
            _ = write(STDERR_FILENO, ptr.baseAddress!.advanced(by: idx + 1), 10 - idx)
        }
    }

    /// Write signal name using static string (no allocation)
    private static func writeSignalName(for signal: Int32) {
        switch signal {
        case SIGABRT: writeSignalSafe("SIGABRT")
        case SIGILL: writeSignalSafe("SIGILL")
        case SIGSEGV: writeSignalSafe("SIGSEGV")
        case SIGFPE: writeSignalSafe("SIGFPE")
        case SIGBUS: writeSignalSafe("SIGBUS")
        case SIGPIPE: writeSignalSafe("SIGPIPE")
        case SIGTRAP: writeSignalSafe("SIGTRAP")
        case SIGTERM: writeSignalSafe("SIGTERM")
        case SIGKILL: writeSignalSafe("SIGKILL")
        default: writeSignalSafe("UNKNOWN")
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
