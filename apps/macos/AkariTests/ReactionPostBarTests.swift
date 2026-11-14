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
        toggles: Bool = true, post: UInt64 = 1
    ) -> PostBar {
        PostBar(
            post: MessageId(rawValue: post), following: false, canFollow: true, invites: false,
            reactions: pills, defaultReaction: offer, reacts: reacts, toggles: toggles)
    }

    func show(_ bar: PostBar, images: ImageCache? = nil) throws -> PostBarCell {
        var state = MessageTableState(rows: [
            row(1, by: author(2, "Ray"), at: list.noon, content: "the post"),
            row(2, by: author(3), at: list.noon + 600, content: "an answer"),
        ])
        state.reachedOldest = true
        state.postBar = bar
        state.images = ImageCacheRef(images, placeholders: nil)
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
        #expect(offer.accessibilityLabel() == "fire, press to react")
        #expect(offer.frame.width == 34)
        #expect(cell.convert(offer.bounds, from: offer).minX == 24)
        #expect(abs(cell.react.frame.minX - (24 + 34 + 4)) < 0.01)
        list.actions.calls = []
        offer.performClick(nil)
        #expect(list.actions.calls == ["toggle :fire: on 1 from forumToolbar"])
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
        #expect(list.actions.calls == ["toggle :tada: on 1 from forumToolbar"])
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

    // The official count takes a pill for 58 wide; a count of five digits makes one 100.
    @Test
    func pillsWithLongCountsLeaveFollowItsPlace() throws {
        let many = (0..<9).map { pill(String(UnicodeScalar(UInt8(65 + $0))), 12345) }

        let cell = try show(bar(many))

        let official = PostBar.pillsShown(width: Double(cell.bounds.width))
        #expect(cell.reactions.pills.count < official)
        #expect(!cell.reactions.pills.isEmpty)
        #expect(cell.reactions.more.count == 9 - cell.reactions.pills.count)
        #expect(cell.react.frame.maxX <= cell.follow.frame.minX - 4)
        #expect(cell.reactions.frame.maxX <= cell.react.frame.minX)
    }

    @Test
    func whoMayNotReactHasNoButtonAndPillsThatOnlyShow() throws {
        let cell = try show(bar([pill("👍")], reacts: false, toggles: false))

        #expect(cell.react.isHidden)
        #expect(cell.reactions.pills.map(\.isEnabled) == [false])
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
        #expect(thumbs.toolTip == ":thumbsup: reacted by 2 people")

        list.move(to: list.table.convert(NSPoint(x: thumbs.bounds.midX, y: thumbs.bounds.midY), from: thumbs))

        #expect(thumbs.isHovered)
        #expect(list.actions.rested.last == ":thumbsup: of 1")
        list.actions.names[":thumbsup:"] = ":thumbsup: reacted by Ann and Bo"
        list.controller.reactionNamesArrived(for: MessageId(rawValue: 1))
        #expect(thumbs.toolTip == ":thumbsup: reacted by Ann and Bo")
    }
}

// A forum card's reaction chip as VoiceOver reads it.
@MainActor
struct ForumCardReactionTests {
    @Test
    func voiceOverReadsTheEmojisShortNameWithoutColons() {
        let offer = ForumModel.Card.Reaction(emoji: .unicode("🔥"), count: 0, mine: false, toggles: true)
        let counted = ForumModel.Card.Reaction(emoji: .unicode("👍"), count: 12, mine: false, toggles: true)

        #expect(ForumCardReaction.label(of: offer) == "fire, press to react")
        #expect(ForumCardReaction.label(of: counted) == "thumbsup, 12 reactions")
    }

    @Test
    func theSamePostsCountRollsAndAnotherPostsReactionsComeWithout() throws {
        list.controller.reducesMotion = { false }
        let cell = try show(bar([pill("👍", 2)]))
        let key = list.keyWindow()
        defer { key.orderOut(nil) }
        let thumbs = cell.reactions.pills[0]

        _ = try show(bar([pill("👍", 3)]))
        #expect(thumbs.isRolling)

        thumbs.countLayer.removeAllAnimations()
        let other = try show(bar([pill("👍", 7)], post: 9))
        #expect(other.reactions.pills.map(\.countText) == ["7"])
        #expect(!other.reactions.pills[0].isRolling)
    }

    @Test
    func reactToPostIsReadAsItsWordsAndTheSymbolAloneAsAddReaction() throws {
        let cell = try show(bar())
        #expect(cell.react.accessibilityLabel() == "React to Post")

        let reacted = try show(bar([pill("👍", 2)]))
        #expect(reacted.react.accessibilityLabel() == "Add Reaction")
        #expect(reacted.react.frame.width == PostBarButton.height)
    }

    @Test
    func theQuickReactionsOpenUnderTheBarsButton() throws {
        list.actions.quick[1] = [QuickReaction(emoji: .unicode("💯"), mine: false)]
        let cell = try show(bar([pill("👍", 2)]))
        var views: [NSView] = []
        list.controller.popUp = { _, _, view in views.append(view) }

        cell.react.performClick(nil)

        #expect(views.count == 1)
        #expect(views.first === cell.react)
    }

    @Test
    func theBarsPillsTakeTheListsImages() throws {
        let cache = ImageCache(source: FakeImageSource.stalling())

        let cell = try show(bar([customPill(7)]), images: cache)

        #expect(cell.reactions.pills.map { $0.images === cache } == [true])
    }
}
