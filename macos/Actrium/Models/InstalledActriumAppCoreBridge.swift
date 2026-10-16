import Foundation
import ActriumCore

// MARK: - Core-backed Source Type Rules

extension AppSourceType {
    init?(coreRawValue: String) {
        guard let normalized = parseAppSourceType(raw: coreRawValue) else { return nil }
        self.init(rawValue: normalized)
    }

    var displayName: String {
        (try? appSourceTypeInfo(raw: rawValue).displayName) ?? rawValue
    }
}

extension InstalledActriumApp {
    /// Human-readable source type.
    var sourceTypeDisplay: String {
        sourceType.displayName
    }
}
