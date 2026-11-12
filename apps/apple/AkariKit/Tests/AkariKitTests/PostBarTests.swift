import AkariFFI
import Foundation
import Testing

@testable import AkariKit

// Forum 13 in guild 1 with the tags Bug (501) and Staff (502, moderated), and post 41 by user 2,
// whose first message has the post's ID.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class PostBarTests {
    let store = FakeStore()
    let account: FakeAccount
    let noon = berlinTime(2026, 10, 10, 12, 0)
    let bug: TagId = id(501)
    let staff: TagId = id(502)

    init() {
        account = FakeAccount(store: store)
    }

    func bar(following: Bool = false, invites: Bool = true) -> PostBar {
        PostBar(post: id(1), following: following, canFollow: true, invites: invites)
    }

    func timeline(
        _ rows: [MessageListModel.Row], edges: Set<MessageTimeline.Edge> = [.beginning]
    ) -> MessageTimeline {
        MessageTimeline(rows: rows, calendar: berlin, edges: edges, postBar: bar())
    }

    // MARK: The timeline

    @Test
    func theBarFollowsThePostsFirstMessageAndEndsItsGroup() {
        let shown = timeline([row(1, at: noon), row(2, at: noon + 60), row(3, at: noon + 120)])

        #expect(shown.shape == ["beginning", "day", "+1", "bar+", "+2", "3"])
    }

    @Test
    func aPostWithoutRepliesEndsWithItsBar() {
        #expect(timeline([row(1, at: noon)]).shape == ["beginning", "day", "+1", "bar"])
    }

    // Discord's bar leads the list once the start is loaded and the first message is gone.
    @Test
    func aDeletedFirstMessageLeavesTheBarAtTheStart() {
        let shown = timeline([row(2, at: noon), row(3, at: noon + 60)])

        #expect(shown.shape == ["beginning", "bar+", "day", "+2", "3"])
        #expect(timeline([]).shape == ["beginning", "bar"])
    }

    @Test
    func noBarBeforeTheStartIsLoaded() {
        let shown = timeline([row(2, at: noon), row(3, at: noon + 60)], edges: [.older])

        #expect(shown.shape == ["older", "day", "+2", "3"])
        #expect(timeline([], edges: [.older]).shape == ["older"])
    }

    @Test
    func aReplyOnTheNextDayHasItsDividerUnderTheBar() {
        let shown = timeline([row(1, at: noon), row(2, at: noon + 86_400)])

        #expect(shown.shape == ["beginning", "day", "+1", "bar+", "day", "+2"])
    }

    @Test
    func theNewMessagesLineComesUnderTheBar() {
        let shown = MessageTimeline(
            rows: [row(1, at: noon), row(2, at: noon + 60)], calendar: berlin,
            edges: [.beginning], newMessages: id(2), postBar: bar())

        #expect(shown.shape == ["beginning", "day", "+1", "bar+", "new", "+2"])
    }

    @Test
    func aChannelWithoutABarIsAsBefore() {
        let shown = MessageTimeline(
            rows: [row(1, at: noon), row(2, at: noon + 60)], calendar: berlin, edges: [.beginning])

        #expect(shown.shape == ["beginning", "day", "+1", "2"])
    }

    // MARK: The model

    func world(
        owner: UInt64 = 2, joined: Bool = false, tags: [TagId]? = nil, moderator: Bool = false,
        oldest: Bool = true, messages: [UInt64] = [41, 5]
    ) {
        var forum = channel(13, kind: .guildForum, name: "ideas")
        forum.forum = ForumSettings(
            tags: [
                ForumTag(id: bug, name: "Bug", moderated: false, emoji: nil),
                ForumTag(id: staff, name: "Staff", moderated: true, emoji: nil),
            ],
            defaultReaction: nil, sort: .latestActivity, layout: .list, tagMatching: .some,
            postSlowmode: 0, template: nil)
        var post = thread(41, in: 13, name: "A post")
        post.thread?.joined = joined
        post.thread?.ownerId = id(owner)
        post.thread?.appliedTags = tags ?? [staff, bug]
        store.update { state in
            state.connection = .online
            state.currentUser = user(1)
            state.add(guild(1))
            state.list([forum], in: 1)
            state.channels[id(41)] = post
            state.permissions[id(13)] =
                moderator ? [.viewChannel, .sendMessages, .manageThreads] : [.viewChannel, .sendMessages]
            state.permissions[id(41)] = [.viewChannel, .sendMessages]
            state.windows[id(41)] = window(messages, oldest: oldest)
            for raw in messages {
                state.show(message(raw, in: 41, by: user(2)))
            }
        }
    }

    func list() -> MessageListModel {
        MessageListModel(channelId: id(41), userId: id(1), account: account, store: store)
    }

    @Test
    func aForumPostHasItsBar() {
        world()

        let bar = list().postBar

        #expect(bar == PostBar(post: id(41), following: false, canFollow: true, invites: true))
    }

    @Test
    func aThreadOfATextChannelHasNone() {
        store.update { state in
            state.add(guild(1))
            state.list([channel(11)], in: 1)
            state.channels[id(31)] = thread(31, in: 11)
            state.windows[id(31)] = window([4], oldest: true)
            state.show(message(4, in: 31))
        }

        let list = MessageListModel(channelId: id(31), userId: id(1), account: account, store: store)

        #expect(list.postBar == nil)
        #expect(list.postStart == nil)
    }

    @Test
    func followingShowsAndFollowsTheStore() async {
        world(joined: true)
        let list = list()
        #expect(list.postBar?.following == true)

        store.update { $0.channels[id(41)]?.thread?.joined = false }
        list.apply(EventBatch([.channelUpdated(channelId: id(41), guildId: id(1))]))

        #expect(list.postBar?.following == false)
    }

    // Discord shows "Start the conversation!" to everyone who may answer but the owner.
    @Test
    func theOwnerAndThoseWhoCannotSendAreNotInvited() {
        world(owner: 1)
        #expect(list().postBar?.invites == false)

        world()
        store.update { $0.permissions[id(41)] = [.viewChannel] }
        #expect(list().postBar?.invites == false)

        world()
        store.update { $0.channels[id(41)]?.thread?.locked = true }
        #expect(list().postBar?.invites == false)
    }

    @Test
    func anArchivedPostCannotBeFollowed() {
        world()
        store.update { $0.channels[id(41)]?.thread?.archived = true }

        #expect(list().postBar?.canFollow == false)
    }

    // The post's own order, the forum's tags only, moderated ones for moderators.
    @Test
    func theStartShowsTheTagsTheUserMayApply() {
        world()
        #expect(list().postStart?.tags.map(\.name) == ["Bug"])

        world(moderator: true)
        #expect(list().postStart?.tags.map(\.name) == ["Staff", "Bug"])

        world(tags: [id(999), bug])
        #expect(list().postStart?.tags.map(\.name) == ["Bug"])
    }

    @Test
    func aFirstMessageGoneFromTheLoadedStartIsDeleted() {
        world()
        #expect(list().postStart?.firstMessageDeleted == false)

        world(messages: [5])
        #expect(list().postStart?.firstMessageDeleted == true)

        world(oldest: false, messages: [5])
        #expect(list().postStart?.firstMessageDeleted == false, "not known before the start loads")
    }

    // Review: a post opened before the store knew it (a launch on it) gets its bar once it does.
    @Test
    func aPostTheStoreLearnsOfLaterGetsItsBarAndStart() {
        let early = list()
        #expect(early.postBar == nil)

        world()
        early.apply(EventBatch([.ready]))

        #expect(early.postBar?.post == id(41))
        #expect(early.postStart?.tags.map(\.name) == ["Bug"])
    }

    @Test
    func aMediaChannelsPostSaysSo() {
        world()
        #expect(list().postStart?.isMedia == false)

        store.update { $0.channels[id(13)]?.kind = .guildMedia }
        #expect(list().postStart?.isMedia == true)
    }
}
