import Foundation
import Testing

@testable import Akari

struct CacheDirectoryTests {
    let base = FileManager.default.temporaryDirectory.appending(
        path: "akari-cache-directory-\(UUID().uuidString)", directoryHint: .isDirectory)

    @Test
    func theCacheDirectoryIsInApplicationSupport() throws {
        defer { try? FileManager.default.removeItem(at: base) }

        let directory = try #require(CacheDirectory.make(base: base))

        #expect(
            directory.standardizedFileURL == base.appending(path: "Akari/Cache").standardizedFileURL
        )
        var isDirectory: ObjCBool = false
        #expect(FileManager.default.fileExists(atPath: directory.path, isDirectory: &isDirectory))
        #expect(isDirectory.boolValue)
    }

    @Test
    func theCacheDirectoryIsExcludedFromBackups() throws {
        defer { try? FileManager.default.removeItem(at: base) }

        let directory = try #require(CacheDirectory.make(base: base))

        let values = try directory.resourceValues(forKeys: [.isExcludedFromBackupKey])
        #expect(values.isExcludedFromBackup == true)
    }

    @Test
    func anUnusableBaseMeansNoCache() throws {
        try Data("not a directory".utf8).write(to: base)
        defer { try? FileManager.default.removeItem(at: base) }

        #expect(CacheDirectory.make(base: base) == nil)
    }
}
