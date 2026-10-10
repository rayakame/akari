import Foundation
import Testing

@testable import AkariKit

@MainActor
final class CollapsedCategoriesTests {
    let suite = "app.akari.tests.\(UUID().uuidString)"
    let defaults: UserDefaults

    init() throws {
        defaults = try #require(UserDefaults(suiteName: suite))
    }

    deinit {
        UserDefaults.standard.removePersistentDomain(forName: suite)
    }

    let channels = [
        channel(1), channel(10, kind: .guildCategory), channel(11, parent: 10),
        channel(12, parent: 10), channel(20, kind: .guildCategory), channel(21, parent: 20),
    ]

    @Test
    func collapsedCategoriesKeepOnlyTheOpenChannel() {
        let collapsed = CollapsedCategories(defaults: defaults)

        collapsed.toggle(id(10), in: id(5))

        #expect(collapsed.isCollapsed(id(10), in: id(5)))
        #expect(!collapsed.isCollapsed(id(20), in: id(5)))
        #expect(
            collapsed.visible(channels, in: id(5), open: id(12)).map(\.id)
                == [id(1), id(10), id(12), id(20), id(21)])
        #expect(
            collapsed.visible(channels, in: id(5), open: nil).map(\.id)
                == [id(1), id(10), id(20), id(21)])
        collapsed.toggle(id(10), in: id(5))
        #expect(collapsed.visible(channels, in: id(5), open: nil) == channels)
    }

    @Test
    func collapseIsPerGuildAndSurvivesANewInstance() {
        CollapsedCategories(defaults: defaults).toggle(id(10), in: id(5))

        let later = CollapsedCategories(defaults: defaults)

        #expect(later.isCollapsed(id(10), in: id(5)))
        #expect(!later.isCollapsed(id(10), in: id(6)))
    }
}
