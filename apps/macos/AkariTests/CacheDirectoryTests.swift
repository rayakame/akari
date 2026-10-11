import Foundation
import Testing

@testable import Akari

struct CacheDirectoryTests {
    // Beside the app's own directory: backup exclusion may read differently in a temporary one.
    let root = URL.applicationSupportDirectory.appending(
        path: "akari-cache-directory-\(UUID().uuidString)", directoryHint: .isDirectory)

    var base: URL {
        root.appending(path: "base", directoryHint: .isDirectory)
    }

    @Test
    func theCacheDirectoryIsInApplicationSupport() {
        #expect(
            CacheDirectory.location().pathComponents.suffix(4) == [
                "Library", "Application Support", "Akari", "Cache",
            ])
    }

    @Test
    func theCacheDirectoryIsCreated() throws {
        defer { try? FileManager.default.removeItem(at: root) }

        let directory = try #require(CacheDirectory.make(base: base))

        #expect(directory.pathComponents == base.pathComponents + ["Akari", "Cache"])
        var isDirectory: ObjCBool = false
        #expect(FileManager.default.fileExists(atPath: directory.path, isDirectory: &isDirectory))
        #expect(isDirectory.boolValue)
    }

    @Test
    func theCacheDirectoryIsExcludedFromBackups() throws {
        defer { try? FileManager.default.removeItem(at: root) }

        let directory = try #require(CacheDirectory.make(base: base))

        var read = URL(filePath: directory.path(percentEncoded: false), directoryHint: .isDirectory)
        read.removeAllCachedResourceValues()
        let values = try read.resourceValues(forKeys: [.isExcludedFromBackupKey])
        let marked = getxattr(
            directory.path, "com.apple.metadata:com_apple_backup_excludeItem", nil, 0, 0, 0)
        #expect(
            values.isExcludedFromBackup == true,
            "backup attribute: \(marked >= 0 ? "set" : "missing, errno \(errno)")")
    }

    @Test
    func anUnusableBaseMeansNoCache() throws {
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data("not a directory".utf8).write(to: base)

        #expect(CacheDirectory.make(base: base) == nil)
    }
}
