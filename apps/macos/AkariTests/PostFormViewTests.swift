import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// The forum's header row as search and as the form for a new post, and "(N New)" on cards.
@MainActor
struct PostFormViewTests {
    let page = ForumPageTests()

    func session(_ change: (inout ForumFakes) -> Void = { _ in }) -> SessionModel {
        var fakes = ForumFakes()
        change(&fakes)
        return fakes.session()
    }

    func row(_ forum: ForumModel) -> ForumHeaderRow {
        ForumHeaderRow(forum: forum, guidelines: {})
    }

    func height(_ forum: ForumModel, width: CGFloat = 700) -> CGFloat {
        let host = NSHostingView(rootView: row(forum).frame(width: width))
        host.layoutSubtreeIfNeeded()
        return host.fittingSize.height
    }

    @Test
    func theFieldSaysWhatItDoes() throws {
        let forum = try #require(session().forum)
        #expect(ForumHeaderRow.placeholder(forum.form) == "Search or create a post...")

        forum.openForm()
        #expect(ForumHeaderRow.placeholder(forum.form) == "Title")

        let closed = try #require(session { $0.canPost = false }.forum)
        #expect(ForumHeaderRow.placeholder(closed.form) == "Search")
    }

    @Test
    func theClosedRowHasNewPostAndTheGuidelines() throws {
        let forum = try #require(session().forum)

        let areas = page.handAreas(in: row(forum), size: NSSize(width: 700, height: 60))

        #expect(areas.count == 2, "\(areas)")
        #expect(height(forum) == 48, "the 32 controls and 8 around")
    }

    @Test
    func newPostIsOffWithoutThePermission() throws {
        let forum = try #require(session { $0.canPost = false }.forum)

        let areas = page.handAreas(in: row(forum), size: NSSize(width: 700, height: 60))

        #expect(areas.count == 1, "the guidelines only: \(areas)")
    }

    // The ×, the three tags the user may apply, the guidelines; Post is off without a title.
    @Test
    func theOpenFormHasItsControls() throws {
        let forum = try #require(session().forum)
        forum.openForm()

        let size = NSSize(width: 700, height: 320)
        #expect(page.handAreas(in: row(forum), size: size).count == 5)

        forum.form.title = "A title"
        forum.form.message = "Some text"
        #expect(page.handAreas(in: row(forum), size: size).count == 6, "and Post")
    }

    // 8, the 32 title row, the message's 80 under 4, the tags (16, 24, 12, the hairline), the
    // controls (12, 40), 8.
    @Test
    func theOpenFormIsAsTallAsItsParts() throws {
        let forum = try #require(session().forum)
        forum.openForm()

        #expect(abs(height(forum) - (8 + 32 + 4 + 80 + 16 + 24 + 12 + 1 + 12 + 40 + 8)) <= 1)
    }

    @Test
    func aMissingTagIsSaidUnderTheControls() throws {
        let forum = try #require(session { $0.requiresTag = true }.forum)
        forum.openForm()
        let plain = height(forum)

        forum.form.title = "A title"
        forum.form.message = "Some text"

        #expect(forum.form.tagMissing)
        #expect(height(forum) > plain + 8)
        forum.form.toggle(tag: ForumFakes.tags[0].id)
        #expect(abs(height(forum) - plain) <= 1)
    }

    @Test
    func tagsWrapOntoMoreRows() {
        let layout = FlowLayout(spacing: 4)
        let host = NSHostingView(
            rootView: layout {
                ForEach(0..<5, id: \.self) { _ in
                    Color.red.frame(width: 100, height: 24)
                }
            }
            .frame(width: 320, alignment: .leading))
        host.layoutSubtreeIfNeeded()

        // Three fit in 320 (100 + 4 + 100 + 4 + 100), two go below.
        #expect(abs(host.fittingSize.height - (24 + 4 + 24)) < 0.5)
    }

    @Test
    func aForumsSlowmodeIsAboutPosts() {
        let slowmode = Slowmode(interval: 30, exempt: false, until: nil)

        #expect(
            PostFormSlowmode.help(slowmode)
                == "Slowmode is enabled. Members can create one post every 30 seconds.")
        #expect(
            PostFormSlowmode.help(Slowmode(interval: 3600, exempt: false, until: nil))
                == "Slowmode is enabled. Members can create one post every 1 hour.")
    }

    // MARK: "(N New)"

    @Test
    func theCountIsWordedAsDiscords() {
        #expect(ForumCardCount.new("3") == "(3 New)")
        #expect(ForumCardCount.new("1+") == "(1+ New)")
        #expect(ForumCardCount.new("25+") == "(25+ New)")
    }

    @Test
    func aCardWithNewMessagesSaysSoAfterItsCount() throws {
        let counted = try #require(session { $0.newCounts = [1: 3] }.forum)
        let plain = try #require(session().forum)
        #expect(counted.posts[1].newCount == "3")
        #expect(plain.posts[1].newCount == nil)

        func width(_ card: ForumModel.Card) -> CGFloat {
            NSHostingView(rootView: ForumCardFooter(card: card, now: ForumFakes.now)).fittingSize.width
        }
        let words = ("(3 New)" as NSString).size(
            withAttributes: [.font: TypeScale.forumNewCount.nsFont]
        ).width
        #expect(abs(width(counted.posts[1]) - width(plain.posts[1]) - (4 + words)) <= 1.5)
    }

    @Test
    func theEmptyStateOffersAPostWhereTheUserMayPost() {
        let with = page.handAreas(
            in: ForumEmptyState(channel: "ideas", tags: 0, create: {}),
            size: NSSize(width: 600, height: 300))
        let without = page.handAreas(
            in: ForumEmptyState(channel: "ideas", tags: 0), size: NSSize(width: 600, height: 300))

        #expect(with.count == 1)
        #expect(without.isEmpty)
        #expect(ForumEmptyState.createTitle == "Create a post")
    }
}
