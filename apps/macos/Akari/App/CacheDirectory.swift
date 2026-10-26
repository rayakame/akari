import Foundation

enum CacheDirectory {
    static func location(base: URL = .applicationSupportDirectory) -> URL {
        base.appending(path: "Akari/Cache", directoryHint: .isDirectory)
    }

    /// Where images go: the caches directory, which the system may purge; the core keeps each
    /// account under `<bundle>/Images/<user id>/`.
    static func imagesLocation(base: URL = .cachesDirectory, bundle: String) -> URL {
        base.appending(path: bundle, directoryHint: .isDirectory)
            .appending(path: "Images", directoryHint: .isDirectory)
    }

    static func makeImages(base: URL = .cachesDirectory, bundle: String) -> URL? {
        let directory = imagesLocation(base: base, bundle: bundle)
        do {
            try FileManager.default.createDirectory(
                at: directory, withIntermediateDirectories: true)
            return directory
        } catch {
            return nil
        }
    }

    // Excluded from backups, so message text doesn't spread into Time Machine or iCloud.
    static func make(base: URL = .applicationSupportDirectory) -> URL? {
        var directory = location(base: base)
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
