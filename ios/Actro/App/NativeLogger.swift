import Foundation
import React

private let logger = AppLogger.reactNative

/// Original Birthday Module - Will JavaScript Log Output To Xcode Console + DevTools Console + Remote Log Server
@objc(NativeLogger)
class NativeLogger: NSObject {

  // Remote log server address
  private static let logServerURL = "http://localhost:9999/log"

  // Send Log to Remote Server (Step forward, no jamming.)
  private static func sendToRemote(level: String, message: String) {
    #if !DEBUG
    // Avoid remote logging in Release to reduce noise and privacy risk.
    return
    #else
    guard let url = URL(string: logServerURL) else { return }

    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.timeoutInterval = 2.0 // Quick timeout. Avoid jamming.

    let body: [String: Any] = [
      "level": level,
      "msg": "[Swift] \(message)",
      "source": "native"
    ]

    do {
      request.httpBody = try JSONSerialization.data(withJSONObject: body)

      // Step-off, ignore the result
      URLSession.shared.dataTask(with: request) { _, _, _ in }.resume()
    } catch {
      // Silence fails.
    }
    #endif
  }

  @objc
  func log(_ message: String) {
    logger.info("[JS] \(message)")
    NativeLogger.sendToRemote(level: "log", message: message)
  }

  @objc
  func warn(_ message: String) {
    logger.warning("[JS WARN] \(message)")
    NativeLogger.sendToRemote(level: "warn", message: message)
  }

  @objc
  func error(_ message: String) {
    logger.error("[JS ERROR] \(message)")
    NativeLogger.sendToRemote(level: "error", message: message)
  }

  // Add: Static log method directly called by original code
  @objc
  static func nativeLog(_ message: String) {
    logger.info("[Native] \(message)")
    sendToRemote(level: "log", message: "[Native] \(message)")
  }

  @objc
  static func nativeWarn(_ message: String) {
    logger.warning("[Native WARN] \(message)")
    sendToRemote(level: "warn", message: "[Native] \(message)")
  }

  @objc
  static func nativeError(_ message: String) {
    logger.error("[Native ERROR] \(message)")
    sendToRemote(level: "error", message: "[Native] \(message)")
  }

  @objc
  static func requiresMainQueueSetup() -> Bool {
    return false
  }

  @objc
  static func moduleName() -> String! {
    return "NativeLogger"
  }
}
