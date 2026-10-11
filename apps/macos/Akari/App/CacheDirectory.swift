import Foundation

enum CacheDirectory {
    /// `Akari/Cache` in Application Support, created and excluded from backups, so message
    /// text doesn't spread into Time Machine or iCloud; `nil` if it can't be made.
    static func make(base: URL = .applicationSupportDirectory) -> URL? {
        var directory = base.appending(path: "Akari/Cache", directoryHint: .isDirectory)
        do {
            try FileManager.default.createDirectory(
                at: directory, withIntermediateDirectories: true)
            var values = URLResourceValues()
            values.isExcludedFromBackup = true
            try directory.setResourceValues(values)
            return directory
        } catch {
            return nil
        }
    }
}
