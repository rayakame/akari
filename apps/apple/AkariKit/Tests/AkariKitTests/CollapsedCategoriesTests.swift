import Foundation
import Testing

@testable import AkariKit

@MainActor
final class CollapsedCategoriesTests {
    let defaults = MemoryDefaults()

    let channels = [
        channel(1), channel(10, kind: .guildCategory), channel(11, parent: 10),
        channel(12, parent: 10), channel(20, kind: .guildCategory), channel(21, parent: 20),
    ]

    @Test
    func collapsedCategoriesKeepOnlyWhatTheListKeeps() {
        let collapsed = CollapsedCategories(defaults: defaults)

        collapsed.toggle(id(10), in: id(5))

        #expect(collapsed.isCollapsed(id(10), in: id(5)))
        #expect(!collapsed.isCollapsed(id(20), in: id(5)))
        #expect(
            collapsed.visible(channels, in: id(5)) { $0.id == id(12) }.map(\.id)
                == [id(1), id(10), id(12), id(20), id(21)])
        #expect(
            collapsed.visible(channels, in: id(5)) { _ in false }.map(\.id)
                == [id(1), id(10), id(20), id(21)])
        collapsed.toggle(id(10), in: id(5))
        #expect(collapsed.visible(channels, in: id(5)) { _ in false } == channels)
    }

    @Test
    func aThreadGoesWithItsParentsCategory() {
        let collapsed = CollapsedCategories(defaults: defaults)
        let list = [
            channel(10, kind: .guildCategory), channel(11, parent: 10), thread(32, in: 11),
            thread(31, in: 11), channel(12, parent: 10), channel(20, kind: .guildCategory),
            channel(21, parent: 20), thread(41, in: 21),
        ]
        collapsed.toggle(id(10), in: id(5))

        #expect(
            collapsed.visible(list, in: id(5)) { $0.id == id(32) }.map(\.id)
                == [id(10), id(11), id(32), id(20), id(21), id(41)],
            "the parent stays for its thread")
        #expect(
            collapsed.visible(list, in: id(5)) { $0.id == id(11) }.map(\.id)
                == [id(10), id(11), id(20), id(21), id(41)],
            "a parent that stays doesn't keep its threads")
    }

    @Test
    func collapseIsPerGuildAndSurvivesANewInstance() {
        CollapsedCategories(defaults: defaults).toggle(id(10), in: id(5))

        let later = CollapsedCategories(defaults: defaults)

        #expect(later.isCollapsed(id(10), in: id(5)))
        #expect(!later.isCollapsed(id(10), in: id(6)))
    }
}
