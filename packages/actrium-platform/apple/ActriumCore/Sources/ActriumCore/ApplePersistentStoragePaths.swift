import Foundation

/// Keep existing stores in place, including SQLite journals and absolute paths in saved records.
/// Legacy names here are durable storage identifiers, not product branding.
public enum ApplePersistentStoragePaths {
    public static func applicationDirectory(in applicationSupport: URL) -> URL {
        let legacy = applicationSupport.appendingPathComponent("Actro", isDirectory: true)
        if FileManager.default.fileExists(atPath: legacy.path) {
            return legacy
        }
        return applicationSupport.appendingPathComponent("Actrium", isDirectory: true)
    }

    public static func appDatabasePath(in directory: URL) -> String {
        databasePath(in: directory, currentName: "actrium.db", legacyName: "actro.db")
    }

    public static func browserDatabasePath(in directory: URL, currentName: String) -> String {
        databasePath(in: directory, currentName: currentName, legacyName: "actro_browser.db")
    }

    private static func databasePath(in directory: URL, currentName: String, legacyName: String) -> String {
        let legacy = directory.appendingPathComponent(legacyName)
        if FileManager.default.fileExists(atPath: legacy.path) {
            return legacy.path
        }
        return directory.appendingPathComponent(currentName).path
    }
}
