import Foundation
import Testing

@testable import Akari

struct LaunchTests {
    @Test
    func theAppDoesntStartUnderTests() {
        #expect(Launch.isTesting)
    }

    @Test
    func testsRunInTheDevelopmentAppsOwnContainer() {
        #expect(Bundle.main.bundleIdentifier == "app.akari.Akari.dev")
        #expect(NSHomeDirectory().hasSuffix("/Library/Containers/app.akari.Akari.dev/Data"))
        #expect(Launch.tokenService(for: Bundle.main.bundleIdentifier) == "app.akari.dev.tokens")
    }

    @Test
    func onlyTheReleaseAppUsesItsTokens() {
        #expect(Launch.tokenService(for: "app.akari.Akari") == "app.akari.tokens")
        #expect(Launch.tokenService(for: "app.akari.Akari.dev") == "app.akari.dev.tokens")
        #expect(Launch.tokenService(for: nil) == "app.akari.dev.tokens")
    }

    @Test
    func loggingIsOnlyEnabledWhenAsked() {
        let defaults = MemoryDefaults()
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
