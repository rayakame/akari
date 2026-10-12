import Foundation
import Testing

@testable import Akari

struct CacheDirectoryTests {
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

        // Time Machine reads this attribute; isExcludedFromBackup reads false on CI's macOS 26
        // runner although the attribute is set.
        let marked = getxattr(
            directory.path, "com.apple.metadata:com_apple_backup_excludeItem", nil, 0, 0, 0)
        #expect(marked >= 0, "errno \(errno)")
    }

    @Test
    func anUnusableBaseMeansNoCache() throws {
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data("not a directory".utf8).write(to: base)

        #expect(CacheDirectory.make(base: base) == nil)
    }
}
