import SwiftUI
import AppKit

// MARK: - ShortcutRecorderView

struct ShortcutRecorderView: NSViewRepresentable {
    let currentBinding: ShortcutBinding?
    let onRecord: (String, NSEvent.ModifierFlags) -> Void

    func makeNSView(context: Context) -> ShortcutRecorderNSView {
        let view = ShortcutRecorderNSView()
        view.currentBinding = currentBinding
        view.onRecord = onRecord
        return view
    }

    func updateNSView(_ nsView: ShortcutRecorderNSView, context: Context) {
        nsView.currentBinding = currentBinding
        nsView.needsDisplay = true
    }
}

// MARK: - ShortcutRecorderNSView

class ShortcutRecorderNSView: NSView {
    var isRecording = false
    var currentBinding: ShortcutBinding?
    var onRecord: ((String, NSEvent.ModifierFlags) -> Void)?

    private let cornerRadius: CGFloat = 6
    private let viewHeight: CGFloat = 24
    private let minWidth: CGFloat = 80

    override var acceptsFirstResponder: Bool { true }

    override var intrinsicContentSize: NSSize {
        NSSize(width: minWidth, height: viewHeight)
    }

    override func mouseDown(with event: NSEvent) {
        if isRecording {
            // Click outside while recording cancels
            isRecording = false
            window?.makeFirstResponder(nil)
        } else {
            isRecording = true
            window?.makeFirstResponder(self)
        }
        needsDisplay = true
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard isRecording else {
            return super.performKeyEquivalent(with: event)
        }
        keyDown(with: event)
        return true
    }

    override func keyDown(with event: NSEvent) {
        guard isRecording else {
            super.keyDown(with: event)
            return
        }

        let keyCode = event.keyCode

        // Escape cancels recording
        if keyCode == 53 {
            isRecording = false
            window?.makeFirstResponder(nil)
            needsDisplay = true
            return
        }

        // Delete/Backspace clears the binding
        if keyCode == 51 || keyCode == 117 {
            isRecording = false
            window?.makeFirstResponder(nil)
            needsDisplay = true
            // Signal clear with empty key
            onRecord?("", NSEvent.ModifierFlags())
            return
        }

        // Require at least Command or Control modifier
        let mods = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        guard mods.contains(.command) || mods.contains(.control) else {
            return
        }

        guard let chars = event.charactersIgnoringModifiers?.lowercased(), !chars.isEmpty else {
            return
        }

        isRecording = false
        window?.makeFirstResponder(nil)
        needsDisplay = true
        onRecord?(chars, mods)
    }

    override func resignFirstResponder() -> Bool {
        isRecording = false
        needsDisplay = true
        return super.resignFirstResponder()
    }

    override func draw(_ dirtyRect: NSRect) {
        let bounds = self.bounds

        // Background
        let bgColor: NSColor = isRecording
            ? .controlAccentColor.withAlphaComponent(0.15)
            : .controlBackgroundColor
        let borderColor: NSColor = isRecording
            ? .controlAccentColor
            : .separatorColor

        let path = NSBezierPath(roundedRect: bounds.insetBy(dx: 0.5, dy: 0.5), xRadius: cornerRadius, yRadius: cornerRadius)
        bgColor.setFill()
        path.fill()
        borderColor.setStroke()
        path.lineWidth = 1
        path.stroke()

        // Text
        let text: String
        let textColor: NSColor
        if isRecording {
            text = "Type shortcut..."
            textColor = .secondaryLabelColor
        } else if let binding = currentBinding {
            text = binding.displayString
            textColor = .labelColor
        } else {
            text = "--"
            textColor = .tertiaryLabelColor
        }

        let font = NSFont.systemFont(ofSize: 12, weight: .medium)
        let attrs: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: textColor,
        ]
        let attrStr = NSAttributedString(string: text, attributes: attrs)
        let textSize = attrStr.size()
        let textRect = NSRect(
            x: (bounds.width - textSize.width) / 2,
            y: (bounds.height - textSize.height) / 2,
            width: textSize.width,
            height: textSize.height
        )
        attrStr.draw(in: textRect)
    }
}
