import Foundation

/// Stable semantic names for the application's top-level window entities.
///
/// Platform chrome belongs inside `tabs`; it must not introduce another
/// application-owned level above `tabs` or beside `workspace`.
public enum MainWindowEntityName: String, CaseIterable, Codable, Sendable {
    case mainWindow = "MainWindow"
    case tabs = "Tabs"
    case workspace = "Workspace"

    public var accessibilityIdentifier: String {
        switch self {
        case .mainWindow:
            "actro.main-window"
        case .tabs:
            "actro.main-window.tabs"
        case .workspace:
            "actro.main-window.workspace"
        }
    }
}

public enum MainWindowHostPlatform: String, CaseIterable, Codable, Sendable {
    case macOS
    case iPadOS
}

public enum MainWindowPresentation: String, CaseIterable, Codable, Sendable {
    case windowed
    case fullScreen
}

/// Canonical application-owned hierarchy for every Apple main window.
///
/// The host platform and presentation mode intentionally do not participate
/// in this value. They may affect safe-area or system-chrome details inside an
/// entity, but never the entity names, order, or ownership.
public struct MainWindowStructure: Equatable, Codable, Sendable {
    public let root: MainWindowEntityName
    public let topLevelElements: [MainWindowEntityName]

    public static let canonical = MainWindowStructure(
        root: .mainWindow,
        topLevelElements: [.tabs, .workspace]
    )

    private init(
        root: MainWindowEntityName,
        topLevelElements: [MainWindowEntityName]
    ) {
        self.root = root
        self.topLevelElements = topLevelElements
    }

    public static func resolve(
        platform: MainWindowHostPlatform,
        presentation: MainWindowPresentation
    ) -> MainWindowStructure {
        _ = platform
        _ = presentation
        return .canonical
    }
}
