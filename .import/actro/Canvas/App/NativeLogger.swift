import Foundation
import React

private let logger = AppLogger.reactNative

/// 原生日志模块 - 将 JavaScript 日志输出到 Xcode 控制台 + DevTools Console + 远程日志服务器
@objc(NativeLogger)
class NativeLogger: NSObject {

  // 远程日志服务器地址
  private static let logServerURL = "http://localhost:9999/log"

  // 发送日志到远程服务器 (异步，不阻塞)
  private static func sendToRemote(level: String, message: String) {
    #if !DEBUG
    // Avoid remote logging in Release to reduce noise and privacy risk.
    return
    #else
    guard let url = URL(string: logServerURL) else { return }

    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.timeoutInterval = 2.0 // 快速超时，避免阻塞

    let body: [String: Any] = [
      "level": level,
      "msg": "[Swift] \(message)",
      "source": "native"
    ]

    do {
      request.httpBody = try JSONSerialization.data(withJSONObject: body)

      // 异步发送，忽略结果
      URLSession.shared.dataTask(with: request) { _, _, _ in }.resume()
    } catch {
      // 静默失败
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

  // 新增：原生代码直接调用的静态日志方法
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
