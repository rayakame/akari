import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// Discord's forum page numbers (canary 653948): the gallery's masonry columns, its card height
// and the loading cards.
@MainActor
struct ForumMeasuresTests {
    @Test
    func theGalleryHasDiscordsColumns() {
        #expect(ForumMeasures.columns(width: 300) == (1, 320))
        #expect(ForumMeasures.columns(width: 700) == (1, 700))
        #expect(ForumMeasures.columns(width: 1000) == (2, 492))
        #expect(ForumMeasures.columns(width: 1500) == (4, 363))
    }

    @Test
    func aGalleryCardIsItsHeaderMediaAndFooter() {
        // 60 + ceil(294 × 2 / 3) + 24 + 36
        #expect(ForumMeasures.galleryCardHeight(width: 320) == 316)
    }

    // A media channel's tiles are 16:9: 60 + ceil(294 × 9 / 16) + 24 + 36.
    @Test
    func aMediaChannelsCardIsLower() {
        #expect(ForumMeasures.galleryCardHeight(width: 320, media: true) == 286)
    }

    @Test
    func aMediaChannelSortsButHasNoOtherView() {
        #expect(ForumSortMenu.title(media: false) == "Sort & View")
        #expect(ForumSortMenu.title(media: true) == "Sort")
    }

    @Test
    func aMediaRowOpensItsPage() {
        let media = Channel(
            id: ChannelId(rawValue: 14), kind: .guildMedia, guildId: ForumFakes.guild, parentId: nil,
            name: "clips", position: 0, topic: nil, nsfw: false, rateLimitPerUser: 0,
            recipientIds: [], thread: nil)

        #expect(media.opensPage)
    }

    @Test
    func loadingCardsFillTheWindowBelowTheHeader() {
        #expect(ForumMeasures.placeholderCards(height: 900) == 6)
        #expect(ForumMeasures.placeholderCards(height: 250) == 1)
    }
}

@MainActor
struct ForumWordsTests {
    @Test
    func theEmptyStateFollowsTheFilter() {
        #expect(ForumEmptyState.words(channel: "ideas", tags: 0) == (
            "Be the first to start the conversation!", "What do you want to post about in #ideas?"))
        #expect(ForumEmptyState.words(channel: "ideas", tags: 1) == (
            "There are no posts with this tag yet!", "Be the first to start a conversation with this tag"))
        #expect(ForumEmptyState.words(channel: "ideas", tags: 2) == (
            "There are no posts with these tags yet!",
            "Be the first to start a conversation with these tags"))
    }

    @Test
    func thePermissionBoxesSayWhatIsMissing() {
        #expect(ForumPermissionBox.history("ideas")
            == "You do not have permission to view the post history of #ideas.")
        #expect(ForumPermissionBox.search("ideas")
            == "You do not have permission to search the post history of #ideas.")
    }

    @Test
    func thePillCountsNewPosts() {
        #expect(NewPostsPill.title(1) == "1 New Post")
        #expect(NewPostsPill.title(3) == "3 New Posts")
    }

    // The forum row's count without mentions (G-8): "N New" for new posts, else the unread ones.
    @Test
    func theForumRowCountsPosts() {
        #expect(PostCountText.text(.new(count: 3)) == "3 New")
        #expect(PostCountText.text(.unread(count: 2)) == "2")
        #expect(PostCountText.text(.new(count: 1234)) == "1k+ New")
        #expect(MentionBadge.accessibilityValue(
            Badge(unread: true, mentions: 0, posts: .new(count: 3))) == "unread, 3 new posts")
        #expect(MentionBadge.accessibilityValue(
            Badge(unread: false, mentions: 0, posts: .unread(count: 1))) == "1 unread post")
    }

    @Test
    func aCardSaysItsTitleAndCount() throws {
        var fakes = ForumFakes()
        let session = fakes.session()
        let cards = try #require(session.forum).posts

        #expect(ForumListCard.label(cards[0]) == "Post Welcome to #ideas: read this first, 2 messages")
    }
}

@MainActor
struct ForumPageTests {
    func card(_ index: Int) throws -> (ForumModel.Card, SessionModel) {
        var fakes = ForumFakes()
        let session = fakes.session()
        return (try #require(session.forum).posts[index], session)
    }

    // The first media is 72 square at the card's right, inside its border, whatever its shape.
    @Test
    func theThumbnailStaysInsideItsCard() throws {
        let (card, session) = try card(1)
        #expect(card.media.count == 1)
        let image = try Rendered(
            ForumListCard(card: card, filter: [], now: ForumFakes.now) {}
                .environment(\.placeholders, PlaceholderCache())
                .frame(width: 600)
                .padding(40)
                .frame(width: 680, height: 220, alignment: .topLeading)
                .background(Color.black)
                .ignoresSafeArea(),
            size: NSSize(width: 680, height: 220))
        let beside = stride(from: CGFloat(642), to: 678, by: 1).map { image.color($0, 100) }

        let black = NSColor(srgbRed: 0, green: 0, blue: 0, alpha: 1)
        #expect(beside.allSatisfy { same($0, black) }, "\(beside)")
        // The thumbnail ends 10 inside the card's right edge: 72 wide, then the background.
        #expect(!same(image.color(640 - 10 - 36, 100), image.color(640 - 10 - 80, 100)))
    }

    func handAreas<Content: View>(in view: Content, size: NSSize) -> [NSRect] {
        let host = NSHostingView(rootView: view.frame(width: size.width, height: size.height))
        let window = TestWindow.make(size: size)
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        func areas(_ view: NSView) -> [CursorArea.Area] {
            ((view as? CursorArea.Area).map { [$0] } ?? []) + view.subviews.flatMap(areas)
        }
        return areas(host).filter { $0.cursor == .pointingHand }.map {
            $0.convert($0.bounds, to: nil)
        }
    }

    // Cards are buttons: the hand over all of them, as over Discord's.
    @Test
    func cardsShowTheHandEdgeToEdge() throws {
        let (card, session) = try card(0)
        let size = NSSize(width: 600, height: 160)

        for (name, view) in [
            ("list", AnyView(ForumListCard(card: card, filter: [], now: ForumFakes.now) {})),
            (
                "gallery",
                AnyView(
                    ForumGalleryCard(
                        card: card, filter: [], size: CGSize(width: 600, height: 160),
                        now: ForumFakes.now
                    ) {})
            ),
        ] {
            let areas = handAreas(in: view, size: size)
            // The list card is 134 tall (its tags row too), centered in the 160.
            for point in [NSPoint(x: 4, y: 80), NSPoint(x: 596, y: 80), NSPoint(x: 300, y: 20)] {
                #expect(areas.contains { $0.contains(point) }, "\(name) \(point): \(areas)")
            }
        }
    }

    @Test
    func theBarsControlsShowTheHand() throws {
        var fakes = ForumFakes()
        let session = fakes.session()
        let forum = try #require(session.forum)
        let size = NSSize(width: 900, height: 60)

        let areas = handAreas(in: ForumTagsBar(forum: forum), size: size)

        // Sort & View, four tags and All.
        #expect(areas.count == 6, "\(areas)")
    }

    // The bar holds the pills that fit before "All", the others not at all.
    @Test
    func theBarHoldsOnlyTheTagsThatFit() throws {
        let tags = ForumFakes.tags
        let two = AllTagsButton.width + ForumTagPill.width(tags[0]) + ForumTagPill.width(tags[1]) + 16
        #expect(ForumTagsBar.fitting(tags, in: two).map(\.name) == ["Bug", "Feature"])
        #expect(ForumTagsBar.fitting(tags, in: two - 1).map(\.name) == ["Bug"])
        #expect(ForumTagsBar.fitting(tags, in: 2000).count == 4)

        var fakes = ForumFakes()
        let session = fakes.session()
        let forum = try #require(session.forum)
        // Sort & View (laid out up to a point wider than it measures), 34 for the divider: the
        // rest holds two pills.
        let sort = NSHostingView(rootView: ForumSortMenu(forum: forum)).fittingSize.width + 2
        let areas = handAreas(
            in: ForumTagsBar(forum: forum), size: NSSize(width: sort + 34 + two, height: 60))
        #expect(areas.count == 4, "Sort & View, two tags and All: \(areas)")
    }

    @Test
    func aCardOpensItsPostBesideTheForum() async throws {
        var fakes = ForumFakes()
        let session = fakes.session()
        let forum = try #require(session.forum)
        let post = forum.posts[1].id

        await ForumPage.open(post, in: forum.forumId, full: false, session)?.value

        #expect(session.forum === forum)
        #expect(session.split?.channelId == post)
    }

    @Test
    func aShiftClickOpensThePostAlone() throws {
        var fakes = ForumFakes()
        let session = fakes.session()
        let forum = try #require(session.forum)
        let post = forum.posts[1].id

        #expect(ForumPage.open(post, in: forum.forumId, full: true, session) == nil)

        #expect(session.forum == nil)
        #expect(session.messages?.channelId == post)
        #expect(session.split == nil)
    }

    // Discord's card of the post open in the split: surface-higher, hovered or not.
    @Test
    func theOpenPostsCardHasTheSelectedFill() throws {
        var fakes = ForumFakes()
        fakes.untagged = true
        let session = fakes.session()
        let card = try #require(session.forum?.posts[1])
        let size = NSSize(width: 600, height: 110)
        func fill(opened: Bool) throws -> NSColor? {
            let view = ForumListCard(card: card, filter: [], opened: opened, now: ForumFakes.now) {}
                .frame(width: size.width, height: size.height, alignment: .top)
            return try Rendered(view, size: size, scale: 2).color(300, 9)
        }
        func drawn(_ color: NSColor) throws -> NSColor? {
            try Rendered(Color(nsColor: color), size: size, scale: 2).color(300, 9)
        }
        func near(_ color: NSColor?, _ other: NSColor?) -> Bool {
            guard let color, let other else {
                return false
            }
            return abs(color.redComponent - other.redComponent) < 0.02
                && abs(color.blueComponent - other.blueComponent) < 0.02
        }

        let opened = try fill(opened: true)
        let closed = try fill(opened: false)
        #expect(near(opened, try drawn(Palette.surfaceHigher)), "\(String(describing: opened))")
        #expect(near(closed, try drawn(Palette.panel)), "\(String(describing: closed))")
        #expect(!near(opened, closed))
    }
}

// A card's one reaction: its chip, its click, and where it stands on the card.
@MainActor
struct ForumCardReactionTests {
    let plain = ForumModel.Card.Reaction(emoji: .unicode("👍"), count: 2, mine: false, toggles: true)
    let mine = ForumModel.Card.Reaction(emoji: .unicode("🎉"), count: 3, mine: true, toggles: true)
    let offered = ForumModel.Card.Reaction(emoji: .unicode("🔥"), count: 0, mine: false, toggles: true)

    @Test
    func aChipIsTwentyHighAndTheForumsDefaultShowsNoNumber() {
        let size = { (reaction: ForumModel.Card.Reaction) in
            NSHostingView(rootView: ForumCardReaction(reaction: reaction)).fittingSize
        }

        #expect(size(plain).height == 20)
        #expect(size(offered) == NSSize(width: 6 + 14 + 6, height: 20))
        #expect(size(plain).width > 26 + 4)
    }

    @Test
    func voiceOverReadsTheEmojiAndItsCountOrThatAPressReacts() {
        #expect(ForumCardReaction.label(of: offered) == "fire, press to react")
        #expect(ForumCardReaction.label(of: .init(emoji: .unicode("👍"), count: 1, mine: false, toggles: true))
            == "thumbsup, 1 reaction")
        #expect(ForumCardReaction.label(of: plain) == "thumbsup, 2 reactions")
    }

    // The middle of a row of pixels: its brightest pixel and how blue it is.
    func blueness(_ color: NSColor?) -> CGFloat {
        guard let color else {
            return 0
        }
        return color.blueComponent - (color.redComponent + color.greenComponent) / 2
    }

    // At 2x on the dark surface: the user's own chip in the brand's blue with its border, the
    // number in the light blue; another one grey without a border.
    @Test
    func theUsersOwnChipHasTheBrandsFillBorderAndNumber() throws {
        let image = try Rendered(
            HStack(spacing: 20) {
                ForumCardReaction(reaction: plain)
                ForumCardReaction(reaction: mine)
            }
            .padding(10)
            .frame(width: 200, height: 40, alignment: .topLeading)
            .background(Color(nsColor: Palette.chat))
            .ignoresSafeArea(),
            size: NSSize(width: 200, height: 40))
        let width = NSHostingView(rootView: ForumCardReaction(reaction: plain)).fittingSize.width
        let second = 10 + width + 20

        // The left border's column, then the fill beside the emoji, at the chip's middle.
        #expect(blueness(image.color(10.25, 20)) < 0.05)
        #expect(blueness(image.color(second + 0.25, 20)) > 0.3, "\(String(describing: image.color(second + 0.25, 20)))")
        #expect(blueness(image.color(11.5, 20)) < 0.05)
        #expect(blueness(image.color(second + 1.5, 20)) > 0.1)
        let digits = stride(from: second + 6 + 14 + 4, to: second + 40, by: 0.5).flatMap { x in
            stride(from: CGFloat(13), to: 27, by: 0.5).compactMap { image.color(x, $0) }
        }
        let brightest = try #require(digits.max { $0.brightnessComponent < $1.brightnessComponent })
        #expect(blueness(brightest) > 0.05, "the number in #dfe4ff: \(brightest)")
    }

    @Test
    func aChipThatOnlyShowsTakesNoClickAndShowsTheNotAllowedCursor() async throws {
        final class Clicks {
            var count = 0
        }
        let clicks = Clicks()
        let shows = ForumModel.Card.Reaction(emoji: .unicode("👍"), count: 2, mine: false, toggles: false)
        let view = ShownView(
            HStack(spacing: 20) {
                ForumCardReaction(reaction: shows) { clicks.count += 100 }
                ForumCardReaction(reaction: plain) { clicks.count += 1 }
            }
            .frame(width: 200, height: 40),
            size: NSSize(width: 200, height: 40))
        let refused = view.hands(.operationNotAllowed)
        let reacts = view.hands()
        try #require(refused.count == 1 && reacts.count == 1)
        #expect(refused[0].maxX < reacts[0].minX)

        try await view.click(at: NSPoint(x: refused[0].midX, y: refused[0].midY))
        try await view.click(at: NSPoint(x: reacts[0].midX, y: reacts[0].midY))

        try await view.waitUntil("the click") { clicks.count > 0 }
        #expect(clicks.count == 1)
    }

    func forum(_ layout: ForumLayout) throws -> (SessionModel, ForumAccount, ForumModel) {
        var fakes = ForumFakes()
        fakes.untagged = true
        fakes.posts = 1
        fakes.layout = layout
        fakes.reactions = [reaction("👍", 2)]
        fakes.reacts = true
        let session = fakes.session()
        return (session, try #require(fakes.account), try #require(session.forum))
    }

    // The chip lies over the card: the footer keeps its place, and its count starts after it.
    @Test
    func aListCardsFooterKeepsThePlaceOfItsChip() throws {
        let (_, _, forum) = try forum(.list)
        let reacted = try #require(forum.posts.first)
        let reaction = try #require(reacted.reaction)
        var fakes = ForumFakes()
        fakes.untagged = true
        fakes.posts = 1
        let plain = try #require(fakes.session().forum?.posts.first)
        #expect(plain.reaction == nil)
        let width = { (card: ForumModel.Card) in
            NSHostingView(rootView: ForumCardFooter(card: card, now: ForumFakes.now)).fittingSize.width
        }
        let chip = NSHostingView(rootView: ForumCardReaction(reaction: reaction)).fittingSize.width

        #expect(abs(width(reacted) - width(plain) - (chip + 8)) < 0.5)
    }

    // Over the card at its footer's left; the footer starts after it.
    @Test
    func aListCardsChipStandsAtTheFootersStartAndReacts() async throws {
        let (session, account, forum) = try forum(.list)
        let view = ShownView(
            ForumPage(forum: forum, session: session, now: ForumFakes.now), size: NSSize(width: 700, height: 500))
        let card = try #require(view.hands().max { $0.width < $1.width }, "the card")
        let chip = try #require(view.hands().first { $0.height == ForumCardReaction.height })
        #expect(abs(chip.minX - (card.minX + 10)) < 0.5)
        #expect(abs(chip.maxY - (card.maxY - ForumMeasures.cardPadding)) < 0.5)

        try await view.click(at: NSPoint(x: chip.midX, y: chip.midY))

        try await view.waitUntil("the reaction") { !account.reacted.withLock { $0 }.isEmpty }
        #expect(account.reacted.withLock { $0 } == ["👍 on \(ForumFakes.post(0).rawValue) from forumToolbar"])
    }

    @Test
    func aGalleryCardsChipReacts() async throws {
        let (session, account, forum) = try forum(.gallery)
        let view = ShownView(
            ForumPage(forum: forum, session: session, now: ForumFakes.now), size: NSSize(width: 700, height: 900))
        let chips = view.hands().filter { $0.height == ForumCardReaction.height }
        let chip = try #require(chips.first, "\(view.hands())")

        try await view.click(at: NSPoint(x: chip.midX, y: chip.midY))

        try await view.waitUntil("the reaction: \(chips)") { !account.reacted.withLock { $0 }.isEmpty }
        #expect(account.reacted.withLock { $0 } == ["👍 on \(ForumFakes.post(0).rawValue) from forumToolbar"])
    }
}
