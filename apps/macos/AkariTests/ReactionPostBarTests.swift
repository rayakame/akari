import AkariKit
import AppKit
import Testing

@testable import Akari

// A forum post's reactions at the left of its bar.
@MainActor
struct ReactionPostBarTests {
    let list = BarList()

    func bar(
        _ pills: [ReactionPill] = [], offer: ReactionEmoji? = nil, reacts: Bool = true,
        toggles: Bool = true
    ) -> PostBar {
        PostBar(
            post: MessageId(rawValue: 1), following: false, canFollow: true, invites: false,
            reactions: pills, defaultReaction: offer, reacts: reacts, toggles: toggles)
    }

    func show(_ bar: PostBar) throws -> PostBarCell {
        var state = MessageTableState(rows: [
            row(1, by: author(2, "Ray"), at: list.noon, content: "the post"),
            row(2, by: author(3), at: list.noon + 600, content: "an answer"),
        ])
        state.reachedOldest = true
        state.postBar = bar
        list.controller.show(state)
        list.window.layoutIfNeeded()
        let index = try #require(list.controller.timeline.items.firstIndex { $0.id == .postBar })
        let cell = try #require(
            list.table.view(atColumn: 0, row: index, makeIfNecessary: true) as? PostBarCell)
        cell.layoutSubtreeIfNeeded()
        cell.reactions.layoutSubtreeIfNeeded()
        return cell
    }

    @Test
    func aPostWithoutReactionsOffersReactToPost() throws {
        let cell = try show(bar())

        #expect(cell.reactions.pills.isEmpty)
        #expect(cell.reactions.offer.isHidden)
        #expect(!cell.react.isHidden)
        #expect(cell.react.label == "React to Post")
        #expect(cell.react.frame.minX == 24)
        #expect(cell.react.frame.height == 24)
        #expect(cell.react.frame.midY == cell.follow.frame.midY)
    }

    @Test
    func theForumsDefaultReactionStandsFirstWithoutANumber() throws {
        let cell = try show(bar(offer: .unicode("🔥")))

        let offer = cell.reactions.offer
        #expect(!offer.isHidden)
        #expect(offer.countText.isEmpty)
        #expect(offer.frame.width == 34)
        #expect(cell.convert(offer.bounds, from: offer).minX == 24)
        #expect(abs(cell.react.frame.minX - (24 + 34 + 4)) < 0.01)
        list.actions.calls = []
        offer.performClick(nil)
        #expect(list.actions.calls == ["toggle 🔥 on 1 from forumToolbar"])
    }

    @Test
    func aPostsReactionsStandInTheBarWithTheSymbolAloneAfterThem() throws {
        let cell = try show(bar([pill("👍", 2), pill("🎉", mine: true)], offer: .unicode("🔥")))

        #expect(cell.reactions.offer.isHidden, "the default goes once the post has reactions")
        #expect(cell.reactions.pills.map(\.countText) == ["2", "1"])
        #expect(cell.reactions.add.isHidden)
        #expect(cell.react.label.isEmpty)
        #expect(cell.react.toolTip == "Add Reaction")
        #expect(abs(cell.react.frame.minX - (24 + 52 + 4 + 52 + 4)) < 0.01)
        let first = cell.convert(cell.reactions.pills[0].bounds, from: cell.reactions.pills[0])
        #expect(first.midY == cell.follow.frame.midY)
        list.actions.calls = []
        cell.reactions.pills[1].performClick(nil)
        #expect(list.actions.calls == ["toggle 🎉 on 1 from forumToolbar"])
    }

    @Test
    func theBarShowsAsManyPillsAsItsWidthAllowsAndCountsTheRest() throws {
        let many = (0..<9).map { pill(String(UnicodeScalar(UInt8(65 + $0))), 2) }

        let cell = try show(bar(many))

        let fits = PostBar.pillsShown(width: Double(cell.bounds.width))
        #expect(fits > 0 && fits < 9)
        #expect(cell.reactions.pills.count == fits)
        #expect(!cell.reactions.more.isHidden)
        #expect(cell.reactions.more.count == 9 - fits)
        #expect(cell.reactions.more.attributedTitle.string == "+\(9 - fits)")
        #expect(cell.react.frame.maxX < cell.follow.frame.minX)
        list.actions.calls = []
        cell.reactions.more.performClick(nil)
        #expect(list.actions.calls == ["view reactions of 1"])
    }

    @Test
    func whoMayNotReactHasNoButtonAndPillsThatOnlyShow() throws {
        let cell = try show(bar([pill("👍")], reacts: false, toggles: false))

        #expect(cell.react.isHidden)
        #expect(cell.reactions.pills.map(\.isEnabled) == [false])
    }

    @Test
    func reactToPostOpensTheQuickReactionsFromTheForumsToolbar() throws {
        list.actions.quick[1] = [QuickReaction(emoji: .unicode("💯"), mine: false)]
        let cell = try show(bar())
        var menus: [NSMenu] = []
        list.controller.popUp = { menu, _, _ in menus.append(menu) }

        cell.react.performClick(nil)

        let menu = try #require(menus.first)
        list.actions.calls = []
        menu.performActionForItem(at: 0)
        #expect(list.actions.calls == ["react 💯 on 1 from forumToolbar"])
    }

    @Test
    func aChangeOfTheReactionsKeepsThePillsViews() throws {
        let cell = try show(bar([pill("👍", 2)]))
        let thumbs = cell.reactions.pills[0]

        let again = try show(bar([pill("👍", 3, mine: true)]))

        #expect(again === cell)
        #expect(again.reactions.pills[0] === thumbs)
        #expect(thumbs.countText == "3")
    }

    @Test
    func thePointerLightsABarsPillAndItsTooltipIsItsMessages() throws {
        let cell = try show(bar([pill("👍", 2)]))
        let thumbs = cell.reactions.pills[0]
        #expect(thumbs.toolTip == "👍 reacted by 2 people")

        list.move(to: list.table.convert(NSPoint(x: thumbs.bounds.midX, y: thumbs.bounds.midY), from: thumbs))

        #expect(thumbs.isHovered)
        #expect(list.actions.rested.last == "👍 of 1")
        list.actions.names["👍"] = "👍 reacted by Ann and Bo"
        list.controller.reactionNamesArrived(for: MessageId(rawValue: 1))
        #expect(thumbs.toolTip == "👍 reacted by Ann and Bo")
    }
}
