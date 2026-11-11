import Foundation

public enum ApplePersistentStoragePaths {
    public static func applicationDirectory(in applicationSupport: URL) -> URL {
        applicationSupport.appendingPathComponent("Actrium", isDirectory: true)
    }

    public static func appDatabasePath(in directory: URL) -> String {
        directory.appendingPathComponent("actrium.db").path
    }

    public static func browserDatabasePath(in directory: URL, currentName: String) -> String {
        directory.appendingPathComponent(currentName).path
    }
}
