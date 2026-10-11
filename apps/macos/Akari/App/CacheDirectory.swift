import Foundation

enum CacheDirectory {
    // Excluded from backups, so message text doesn't spread into Time Machine or iCloud.
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
