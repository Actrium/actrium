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

        // Load js-beautify library
        guard let beautifyURL = Bundle.main.url(forResource: "beautify.min", withExtension: "js"),
              let beautifyScript = try? String(contentsOf: beautifyURL, encoding: .utf8) else {
            print("[JSBeautifier] Failed to load beautify.min.js")
            return
        }

        // Evaluate the library
        context.evaluateScript(beautifyScript)

        // Check if js_beautify function is available
        if let jsBeautify = context.objectForKeyedSubscript("js_beautify"),
           !jsBeautify.isUndefined {
            isReady = true
        } else {
            print("[JSBeautifier] js_beautify function not found")
        }
    }

    /// Beautify JavaScript source code
    /// - Parameter source: The JavaScript source code to beautify
    /// - Returns: Beautified code, or original if beautification fails
    func beautify(_ source: String) -> String {
        guard isReady, let context = context else {
            return source
        }

        // Escape the source for JavaScript string
        let escaped = source
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
            .replacingOccurrences(of: "\n", with: "\\n")
            .replacingOccurrences(of: "\r", with: "\\r")
            .replacingOccurrences(of: "\t", with: "\\t")

        // Call js_beautify with options
        let script = """
        js_beautify("\(escaped)", {
            indent_size: 2,
            indent_char: " ",
            max_preserve_newlines: 2,
            preserve_newlines: true,
            keep_array_indentation: false,
            break_chained_methods: false,
            brace_style: "collapse",
            space_before_conditional: true,
            unescape_strings: false,
            jslint_happy: false,
            end_with_newline: false,
            wrap_line_length: 0,
            e4x: false,
            comma_first: false,
            operator_position: "before-newline"
        })
        """

        if let result = context.evaluateScript(script),
           !result.isUndefined,
           let beautified = result.toString() {
            return beautified
        }

        return source
    }
}
