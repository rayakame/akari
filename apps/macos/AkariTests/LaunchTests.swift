import Foundation
import Testing

@testable import Akari

struct LaunchTests {
    @Test
    func theAppDoesntStartUnderTests() {
        #expect(Launch.isTesting)
    }

    @Test
    func loggingIsOnlyEnabledWhenAsked() throws {
        let suite = "app.akari.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        var filters: [String] = []

        Launch.enableLoggingIfAsked(defaults) { filters.append($0) }
        defaults.set("akari_core=info", forKey: "AkariLog")
        Launch.enableLoggingIfAsked(defaults) { filters.append($0) }

        #expect(filters == ["akari_core=info"])
    }

    @Test(arguments: [
        ("scroll", true), ("akari_core=info,scroll", true), ("scroll=info", true),
        ("akari_core=info", false), ("scrollbar", false),
    ])
    func theScrollLogIsOnlyOnWhenItsCategoryIsAsked(filter: String, asked: Bool) {
        #expect(ScrollLog.isAsked(by: filter) == asked)
    }
}
