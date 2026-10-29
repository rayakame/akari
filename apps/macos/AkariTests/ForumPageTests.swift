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
            // The list card is 132 tall, centered in the 160.
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

    // A card's click opens its post in full until posts open beside the forum (G-10).
    @Test
    func aCardOpensItsPost() throws {
        var fakes = ForumFakes()
        let session = fakes.session()
        let forum = try #require(session.forum)
        let post = forum.posts[1].id

        session.go(to: post)

        #expect(session.forum == nil)
        #expect(session.messages?.channelId == post)
    }
}
