import Foundation
import JavaScriptCore

/// JavaScript code beautifier using js-beautify library via JavaScriptCore
final class JSBeautifier {
    static let shared = JSBeautifier()

    private let context: JSContext?
    private var isReady = false

    private init() {
        context = JSContext()
        setupContext()
    }

    private func setupContext() {
        guard let context = context else { return }

        // Create global/window objects for js-beautify to attach to
        context.evaluateScript("var global = this; var window = this;")

        // Load js-beautify library
        guard let beautifyURL = Bundle.main.url(forResource: "beautify.min", withExtension: "js"),
              let beautifyScript = try? String(contentsOf: beautifyURL, encoding: .utf8) else {
            NSLog("[JSBeautifier] Failed to load beautify.min.js")
            return
        }

        // Evaluate the library
        context.evaluateScript(beautifyScript)

        // Check if js_beautify function is available
        if let jsBeautify = context.objectForKeyedSubscript("js_beautify"),
           !jsBeautify.isUndefined {
            isReady = true
            NSLog("[JSBeautifier] Ready")
        } else {
            NSLog("[JSBeautifier] js_beautify function not found after loading")
        }
    }

    /// Beautify JavaScript source code
    /// - Parameter source: The JavaScript source code to beautify
    /// - Returns: Beautified code, or original if beautification fails
    func beautify(_ source: String) -> String {
        guard isReady, let context = context else {
            NSLog("[JSBeautifier] Not ready, returning original")
            return source
        }

        // Use JSValue call to pass source directly, avoiding escaping issues
        guard let jsBeautify = context.objectForKeyedSubscript("js_beautify") else {
            NSLog("[JSBeautifier] js_beautify not found")
            return source
        }

        // Create options object
        let options: [String: Any] = [
            "indent_size": 2,
            "indent_char": " ",
            "max_preserve_newlines": 2,
            "preserve_newlines": true,
            "break_chained_methods": false,
            "brace_style": "collapse",
            "space_before_conditional": true,
            "end_with_newline": false
        ]

        // Call js_beautify(source, options)
        let result = jsBeautify.call(withArguments: [source, options])

        NSLog("[JSBeautifier] Input length: %d", source.count)
        NSLog("[JSBeautifier] Result isUndefined: %d, isNull: %d", result?.isUndefined ?? true, result?.isNull ?? true)

        if let result = result,
           !result.isUndefined,
           !result.isNull,
           let beautified = result.toString(),
           beautified != "undefined" {
            NSLog("[JSBeautifier] Output length: %d", beautified.count)
            NSLog("[JSBeautifier] Output preview: %@", String(beautified.prefix(100)))
            return beautified
        }

        NSLog("[JSBeautifier] Beautify failed, returning original")
        return source
    }
}
