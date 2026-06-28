import Foundation
import AppKit
import Combine

// MARK: - Shortcut Category

enum ShortcutCategory: String, CaseIterable {
    case navigation = "Navigation"
    case tabs = "Tabs"
    case view = "View"
    case developer = "Developer"
}

// MARK: - Shortcut Action

enum ShortcutAction: String, CaseIterable, Codable {
    case newWindow
    case newTab
    case closeTab
    case reload
    case focusAddressBar
    case printPage
    case showHistory
    case showBookmarks
    case addBookmark
    case zoomIn
    case zoomOut
    case actualSize
    case toggleFullscreen
    case toggleDevTools
    case openDevToolsWindow
    case clearConsole
    case appManager

    var displayName: String {
        switch self {
        case .newWindow: return "New Window"
        case .newTab: return "New Tab"
        case .closeTab: return "Close Tab"
        case .reload: return "Reload"
        case .focusAddressBar: return "Focus Address Bar"
        case .printPage: return "Print"
        case .showHistory: return "Show All History"
        case .showBookmarks: return "Show All Bookmarks"
        case .addBookmark: return "Add Bookmark"
        case .zoomIn: return "Zoom In"
        case .zoomOut: return "Zoom Out"
        case .actualSize: return "Actual Size"
        case .toggleFullscreen: return "Toggle Fullscreen"
        case .toggleDevTools: return "Toggle DevTools"
        case .openDevToolsWindow: return "Open ACTR DevTools in New Window"
        case .clearConsole: return "Clear Console"
        case .appManager: return "App Manager"
        }
    }

    var category: ShortcutCategory {
        switch self {
        case .newWindow, .newTab, .closeTab, .reload, .focusAddressBar, .printPage:
            return .navigation
        case .showHistory, .showBookmarks, .addBookmark:
            return .tabs
        case .zoomIn, .zoomOut, .actualSize, .toggleFullscreen:
            return .view
        case .toggleDevTools, .openDevToolsWindow, .clearConsole, .appManager:
            return .developer
        }
    }
}

// MARK: - Shortcut Binding

struct ShortcutBinding: Codable, Equatable {
    var key: String
    var modifiers: UInt

    var displayString: String {
        let flags = NSEvent.ModifierFlags(rawValue: modifiers)
        var parts: [String] = []
        if flags.contains(.control) { parts.append("\u{2303}") }
        if flags.contains(.option) { parts.append("\u{2325}") }
        if flags.contains(.shift) { parts.append("\u{21E7}") }
        if flags.contains(.command) { parts.append("\u{2318}") }

        let keyDisplay: String
        switch key {
        case "+": keyDisplay = "+"
        case "-": keyDisplay = "-"
        default: keyDisplay = key.uppercased()
        }
        parts.append(keyDisplay)
        return parts.joined()
    }
}

// MARK: - Shortcut Store

final class ShortcutStore: ObservableObject {
    static let shared = ShortcutStore()

    private static let userDefaultsKey = "customShortcutBindings"

    @Published var bindings: [ShortcutAction: ShortcutBinding]

    static let defaults: [ShortcutAction: ShortcutBinding] = [
        .newWindow: ShortcutBinding(key: "n", modifiers: NSEvent.ModifierFlags([.command, .shift]).rawValue),
        .newTab: ShortcutBinding(key: "t", modifiers: NSEvent.ModifierFlags.command.rawValue),
        .closeTab: ShortcutBinding(key: "w", modifiers: NSEvent.ModifierFlags.command.rawValue),
        .reload: ShortcutBinding(key: "r", modifiers: NSEvent.ModifierFlags.command.rawValue),
        .focusAddressBar: ShortcutBinding(key: "l", modifiers: NSEvent.ModifierFlags.command.rawValue),
        .printPage: ShortcutBinding(key: "p", modifiers: NSEvent.ModifierFlags.command.rawValue),
        .showHistory: ShortcutBinding(key: "y", modifiers: NSEvent.ModifierFlags.command.rawValue),
        .showBookmarks: ShortcutBinding(key: "b", modifiers: NSEvent.ModifierFlags([.command, .option]).rawValue),
        .addBookmark: ShortcutBinding(key: "d", modifiers: NSEvent.ModifierFlags.command.rawValue),
        .zoomIn: ShortcutBinding(key: "+", modifiers: NSEvent.ModifierFlags.command.rawValue),
        .zoomOut: ShortcutBinding(key: "-", modifiers: NSEvent.ModifierFlags.command.rawValue),
        .actualSize: ShortcutBinding(key: "0", modifiers: NSEvent.ModifierFlags.command.rawValue),
        .toggleFullscreen: ShortcutBinding(key: "f", modifiers: NSEvent.ModifierFlags([.command, .control]).rawValue),
        .toggleDevTools: ShortcutBinding(key: "i", modifiers: NSEvent.ModifierFlags([.command, .option]).rawValue),
        .openDevToolsWindow: ShortcutBinding(key: "i", modifiers: NSEvent.ModifierFlags([.command, .option, .shift]).rawValue),
        .clearConsole: ShortcutBinding(key: "k", modifiers: NSEvent.ModifierFlags([.command, .option]).rawValue),
        .appManager: ShortcutBinding(key: "m", modifiers: NSEvent.ModifierFlags([.command, .shift]).rawValue),
    ]

    private init() {
        bindings = Self.defaults
        loadCustomBindings()
    }

    // MARK: - Public API

    func defaultBinding(for action: ShortcutAction) -> ShortcutBinding {
        Self.defaults[action]!
    }

    func updateBinding(for action: ShortcutAction, key: String, modifiers: NSEvent.ModifierFlags) {
        bindings[action] = ShortcutBinding(key: key, modifiers: modifiers.rawValue)
        saveCustomBindings()
    }

    func resetBinding(for action: ShortcutAction) {
        bindings[action] = Self.defaults[action]
        saveCustomBindings()
    }

    func resetAllToDefaults() {
        bindings = Self.defaults
        UserDefaults.standard.removeObject(forKey: Self.userDefaultsKey)
    }

    func hasConflict(key: String, modifiers: NSEvent.ModifierFlags, excluding: ShortcutAction?) -> ShortcutAction? {
        for (action, binding) in bindings {
            if action == excluding { continue }
            if binding.key == key && binding.modifiers == modifiers.rawValue {
                return action
            }
        }
        return nil
    }

    // MARK: - Persistence

    private func loadCustomBindings() {
        guard let data = UserDefaults.standard.data(forKey: Self.userDefaultsKey),
              let custom = try? JSONDecoder().decode([String: ShortcutBinding].self, from: data) else {
            return
        }
        for (rawAction, binding) in custom {
            if let action = ShortcutAction(rawValue: rawAction) {
                bindings[action] = binding
            }
        }
    }

    private func saveCustomBindings() {
        var custom: [String: ShortcutBinding] = [:]
        for (action, binding) in bindings {
            if binding != Self.defaults[action] {
                custom[action.rawValue] = binding
            }
        }
        if custom.isEmpty {
            UserDefaults.standard.removeObject(forKey: Self.userDefaultsKey)
        } else if let data = try? JSONEncoder().encode(custom) {
            UserDefaults.standard.set(data, forKey: Self.userDefaultsKey)
        }
    }
}
