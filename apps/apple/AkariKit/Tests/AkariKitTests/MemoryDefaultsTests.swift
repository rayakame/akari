import Foundation
import Testing

struct MemoryDefaultsTests {
    let key = "app.akari.tests.\(UUID().uuidString)"

    @Test
    func valuesStayInTheInstance() {
        let defaults = MemoryDefaults()

        defaults.set("1", forKey: key)

        #expect(defaults.string(forKey: key) == "1")
        #expect(MemoryDefaults().object(forKey: key) == nil)
        #expect(UserDefaults.standard.object(forKey: key) == nil)
        defaults.removeObject(forKey: key)
        #expect(defaults.object(forKey: key) == nil)
    }

    @Test
    func theTypesTheAppsStoreComeBack() {
        let defaults = MemoryDefaults()

        defaults.set(["guild": "1"], forKey: "\(key).spot")
        defaults.set(["2", "3"], forKey: "\(key).collapsed")
        defaults.set(240.0, forKey: "\(key).width")

        #expect(defaults.dictionary(forKey: "\(key).spot") as? [String: String] == ["guild": "1"])
        #expect(defaults.stringArray(forKey: "\(key).collapsed") == ["2", "3"])
        #expect(defaults.double(forKey: "\(key).width") == 240)
        #expect(defaults.double(forKey: "\(key).missing") == 0)
        for suffix in ["spot", "collapsed", "width"] {
            #expect(UserDefaults.standard.object(forKey: "\(key).\(suffix)") == nil)
        }
    }

    // Every removed domain held one of cfprefsd's file descriptors for seconds; a test loop once
    // used them all up, and the real app's settings could then be neither read nor written.
    @Test
    func noTestMakesAPersistentDefaultsDomain() throws {
        let kit = URL(filePath: #filePath).deletingLastPathComponent()
        let app = kit.appending(path: "../../../../macos/AkariTests").standardized
        var offenders: [String] = []
        for directory in [kit, app] {
            let files = try #require(FileManager.default.enumerator(atPath: directory.path))
            for case let file as String in files where file.hasSuffix(".swift") {
                let path = directory.appending(path: file)
                guard path.lastPathComponent != URL(filePath: #filePath).lastPathComponent else {
                    continue
                }
                let source = try String(contentsOf: path, encoding: .utf8)
                if source.contains("UserDefaults(suiteName:")
                    || source.contains("removePersistentDomain")
                {
                    offenders.append(path.lastPathComponent)
                }
            }
        }

        #expect(FileManager.default.fileExists(atPath: app.path))
        #expect(offenders.isEmpty, "\(offenders.sorted())")
    }
}
