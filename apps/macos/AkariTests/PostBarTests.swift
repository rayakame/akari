import AkariKit
import AppKit
import Testing

@testable import Akari

// A forum post's start, its action bar under the first message, and what stands under the bar.
@MainActor
struct PostBarTests {
    let list = MessageTableTests()
    let noon = berlinTime(2026, 10, 10, 12, 0)

    func bar(following: Bool = false, canFollow: Bool = true, invites: Bool = true) -> PostBar {
        PostBar(
            post: MessageId(rawValue: 1), following: following, canFollow: canFollow,
            invites: invites)
    }

    func state(_ count: Int, bar: PostBar) -> MessageTableState {
        var state = MessageTableState(rows: list.messages(count))
        state.reachedOldest = true
        state.postBar = bar
        return state
    }

    func barCell() throws -> PostBarCell {
        let index = try #require(
            list.controller.timeline.items.firstIndex { $0.id == .postBar })
        return try #require(
            list.table.view(atColumn: 0, row: index, makeIfNecessary: true) as? PostBarCell)
    }

    @Test
    func theBarIsTheRowUnderTheFirstMessage() throws {
        list.show(state(3, bar: bar()))

        let items = list.controller.timeline.items.map(\.id)
        let first = try #require(items.firstIndex(of: .message(MessageId(rawValue: 1))))
        #expect(items[first + 1] == .postBar)
        let cell = try barCell()
        #expect(cell.under == .divider)
        #expect(list.table.rect(ofRow: first + 1).height == PostBarCell.height(bar(), hasReplies: true))
    }

    @Test
    func theRowIsAsTallAsWhatStandsUnderTheBar() {
        let plain = PostBarCell.height(bar(invites: false), hasReplies: false)
        #expect(plain == 53, "8, the border, 6, the 32 buttons, 6")
        #expect(PostBarCell.height(bar(), hasReplies: true) == plain + 8 + 16)
        #expect(PostBarCell.height(bar(), hasReplies: false) == plain + 16 + 64 + 16)
    }

    @Test
    func aPostWithoutRepliesInvitesThoseWhoMayAnswer() throws {
        list.show(state(1, bar: bar()))
        #expect(try barCell().under == .invite)

        list.show(state(1, bar: bar(invites: false)))
        #expect(try barCell().under == .nothing)
    }

    @Test
    func followSaysWhatAClickDoes() throws {
        let actions = MessageTableTests.RecordingActions()
        list.controller.actions = actions
        list.show(state(2, bar: bar()))
        var cell = try barCell()
        #expect(cell.follow.label == "Follow")
        #expect(cell.follow.symbol == "bell.fill")
        #expect(cell.follow.toolTip == "Add this post to your channel list and receive notifications about it.")

        cell.follow.performClick(nil)
        #expect(actions.calls.last == "follow true")

        list.show(state(2, bar: bar(following: true)))
        cell = try barCell()
        #expect(cell.follow.label == "Following")
        #expect(cell.follow.symbol == "checkmark")
        cell.follow.performClick(nil)
        #expect(actions.calls.last == "follow false")
    }

    @Test
    func anArchivedPostsFollowIsOff() throws {
        list.show(state(2, bar: bar(canFollow: false)))

        #expect(try !barCell().follow.isEnabled)
    }

    @Test
    func copyLinkSaysCopiedUntilItsTimeIsUp() async throws {
        let actions = MessageTableTests.RecordingActions()
        list.controller.actions = actions
        list.show(state(2, bar: bar()))
        let cell = try barCell()
        let gate = AsyncStream<Void>.makeStream()
        var waited: [Duration] = []
        cell.wait = { duration in
            waited.append(duration)
            for await _ in gate.stream { break }
        }
        #expect(cell.copy.label.isEmpty)
        #expect(cell.copy.toolTip == "Copy Link")

        cell.copy.performClick(nil)

        #expect(actions.calls.last == "copy link")
        #expect(cell.copy.label == "Copied")
        #expect(cell.copy.symbol == "checkmark")
        gate.continuation.yield()
        for _ in 0..<100 where cell.copy.label == "Copied" {
            await Task.yield()
        }
        #expect(cell.copy.label.isEmpty)
        #expect(cell.copy.symbol == "link")
        #expect(waited == [.seconds(2)])
    }

    @Test
    func theButtonsShowTheHand() throws {
        list.show(state(2, bar: bar()))
        let cell = try barCell()
        list.window.layoutIfNeeded()

        for button in [cell.follow, cell.copy] {
            let middle = button.convert(NSPoint(x: button.bounds.midX, y: button.bounds.midY), to: nil)
            #expect(list.controller.cursor(at: middle) == .pointingHand)
        }
    }

    // MARK: The start

    func tag(_ raw: UInt64, _ name: String) -> ForumTag {
        ForumTag(id: TagId(rawValue: raw), name: name, moderated: false, emoji: nil)
    }

    @Test
    func aPostsStartIsItsNameWithTheForumsSymbol() throws {
        let start = try #require(ChannelName(kind: .post(nil), name: "A post").start)

        #expect(start.title == "A post")
        #expect(start.icon == .symbol(ChannelSymbols.name(for: .guildForum)))
        #expect(start.subtitle == nil)
        #expect(start.tags.isEmpty)
    }

    @Test
    func aDeletedFirstMessageIsSaidUnderTheName() throws {
        let post = PostStart(tags: [tag(501, "Bug")], firstMessageDeleted: true)

        let start = try #require(ChannelName(kind: .post(post), name: "A post").start)

        #expect(start.subtitle == "Original message was deleted")
        #expect(start.tags.map(\.name) == ["Bug"])
    }

    @Test
    func tagsAddTheirRowToTheStart() throws {
        let plain = try #require(ChannelName(kind: .post(nil), name: "A post").start)
        let tagged = try #require(
            ChannelName(
                kind: .post(PostStart(tags: [tag(501, "Bug")], firstMessageDeleted: false)),
                name: "A post"
            ).start)

        let difference =
            ChannelStartCell.height(tagged, width: 500) - ChannelStartCell.height(plain, width: 500)

        #expect(abs(difference - (14 + 24)) <= 1, "\(difference)")
        var state = MessageTableState(rows: list.messages(1))
        state.reachedOldest = true
        state.start = tagged
        list.show(state)
        #expect(abs(list.fittingHeight(ofRow: 0) - ChannelStartCell.height(tagged, width: list.table.tableColumns[0].width)) <= 1)
    }
}
