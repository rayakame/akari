import Foundation
import Testing

@testable import AkariKit

// Channel 10 in guild 1 with messages 1 to 3, each allowing reactions.
@MainActor @Suite(.timeLimit(.minutes(1)))
struct ReactionRowsTests {
    let store = FakeStore()
    let account: FakeAccount
    let here: ChannelId = id(10)
    let noon = berlinTime(2026, 10, 10, 12, 0)

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.add(guild(1))
            state.list([channel(10)], in: 1)
            state.show(message(1), message(2), message(3))
            state.windows[id(10)] = window([1, 2, 3])
            for raw in UInt64(1)...3 {
                state.allow(reacting(), message: raw, in: 10)
            }
            state.reactions[id(2)] = [reaction("👍", 2), reaction("🎉", 1, me: true)]
        }
    }

    func list(_ channel: UInt64 = 10) -> MessageListModel {
        MessageListModel(channelId: id(channel), userId: id(1), account: account, store: store)
    }

    func pills(_ list: MessageListModel) -> [[String]?] {
        list.rows.map { $0.reactions?.pills.map { "\($0.name) \($0.count)\($0.mine ? " mine" : "")" } }
    }

    @Test
    func rowsCarryTheirReactions() {
        let list = list()

        #expect(pills(list) == [nil, ["👍 2", "🎉 1 mine"], nil])
        #expect(list.rows[1].reactions?.adds == true)
        #expect(list.rows[1].reactions?.toggles == true)
    }

    @Test
    func aReactionChangeRereadsTheRowsPillsAndNothingElseOfIt() {
        let list = list()
        store.update { $0.reactions[id(2)] = [reaction("👍", 3, me: true)] }
        store.log.forget()

        list.apply(EventBatch([.messageReactionsChanged(channelId: here, messageId: id(2))]))

        #expect(pills(list) == [nil, ["👍 3 mine"], nil])
        #expect(
            store.reads == [
                .messageReactions(here, [id(2)]), .messageActions(here, id(2)),
            ])
    }

    @Test
    func theLastReactionGoneLeavesNoRow() {
        let list = list()
        store.update { $0.reactions[id(2)] = [] }

        list.apply(EventBatch([.messageReactionsChanged(channelId: here, messageId: id(2))]))

        #expect(pills(list) == [nil, nil, nil])
    }

    @Test
    func aChangeForAMessageThatIsntLoadedReadsNothing() {
        let list = list()
        store.log.forget()

        list.apply(EventBatch([.messageReactionsChanged(channelId: here, messageId: id(9))]))

        #expect(store.reads.isEmpty)
    }

    @Test
    func anEditedMessageKeepsReadingItsReactions() {
        let list = list()
        store.update { state in
            state.show(message(2, content: "edited"))
            state.reactions[id(2)] = [reaction("👍", 5)]
        }

        list.apply(EventBatch([.messageUpdated(channelId: here, messageId: id(2))]))

        #expect(pills(list) == [nil, ["👍 5"], nil])
    }

    @Test
    func whatTheUserMayDoComesFromTheCore() {
        store.update { $0.allow(reacting(react: false, toggle: false), message: 2, in: 10) }

        let row = list().rows[1].reactions

        #expect(row?.adds == false)
        #expect(row?.toggles == false)
    }

    @Test
    func withTheSettingOffRowsHaveNoReactionsUntilItIsTurnedOn() {
        store.update { $0.displaySettings.renderReactions = false }
        let list = list()
        #expect(pills(list) == [nil, nil, nil])

        store.update { $0.displaySettings.renderReactions = true }
        list.apply(EventBatch([.displaySettingsChanged]))
        #expect(pills(list) == [nil, ["👍 2", "🎉 1 mine"], nil])

        store.update { $0.displaySettings.renderReactions = false }
        list.apply(EventBatch([.displaySettingsChanged]))
        #expect(pills(list) == [nil, nil, nil])
    }

    @Test
    func aStarterRowShowsTheReactionsOfTheMessageItShows() {
        let original = message(7, in: 10)
        store.update { state in
            state.channels[id(31)] = thread(31, in: 10)
            state.show(message(31, in: 31, kind: .threadStarterMessage))
            state.windows[id(31)] = window([31], oldest: true)
            state.starters[id(31)] = .loaded(message: original)
            // As the core answers for the row's own IDs.
            state.reactions[id(31)] = [reaction("👀", 1)]
            state.allow(reacting(), message: 31, in: 31)
        }
        let list = list(31)
        #expect(pills(list) == [["👀 1"]])

        store.update { $0.reactions[id(31)] = [reaction("👀", 2)] }
        list.apply(EventBatch([.messageReactionsChanged(channelId: here, messageId: id(7))]))

        #expect(pills(list) == [["👀 2"]])
    }

    // MARK: A forum post

    func post(defaultReaction: TagEmoji? = nil, first: MessageActions? = reacting()) {
        var forum = channel(13, kind: .guildForum, name: "ideas")
        forum.forum = ForumSettings(
            tags: [], defaultReaction: defaultReaction, sort: .latestActivity, layout: .list,
            tagMatching: .some, postSlowmode: 0, template: nil)
        store.update { state in
            state.list([forum], in: 1)
            state.channels[id(41)] = thread(41, in: 13, name: "A post")
            state.permissions[id(41)] = [.viewChannel, .sendMessages]
            state.windows[id(41)] = window([41, 50], oldest: true)
            state.show(message(41, in: 41, by: user(2)), message(50, in: 41, by: user(2)))
            state.allow(first, message: 41, in: 41)
            state.allow(reacting(), message: 50, in: 41)
        }
    }

    @Test
    func aPostsFirstMessageHasItsReactionsInTheBar() {
        post()
        store.update { state in
            state.reactions[id(41)] = [reaction("👍", 2), reaction("🎉", 4, me: true)]
            state.reactions[id(50)] = [reaction("👀", 1)]
        }

        let list = list(41)

        #expect(pills(list) == [nil, ["👀 1"]])
        #expect(list.postBar?.reactions.map(\.name) == ["👍", "🎉"])
        #expect(list.postBar?.reacts == true)
        #expect(list.postBar?.toggles == true)
        #expect(list.postBar?.defaultReaction == nil)
    }

    @Test
    func theBarFollowsItsMessagesReactions() {
        post()
        let list = list(41)
        #expect(list.postBar?.reactions.isEmpty == true)

        store.update { $0.reactions[id(41)] = [reaction("👍", 1)] }
        list.apply(EventBatch([.messageReactionsChanged(channelId: id(41), messageId: id(41))]))

        #expect(list.postBar?.reactions.map(\.count) == [1])
        #expect(pills(list) == [nil, nil])
    }

    @Test
    func theForumsDefaultReactionIsOfferedWhereTheUserMayReact() {
        post(defaultReaction: .unicode(emoji: "🔥"))
        #expect(list(41).postBar?.defaultReaction == .unicode("🔥"))

        post(defaultReaction: .unicode(emoji: "🔥"), first: reacting(react: false))
        let readOnly = list(41).postBar
        #expect(readOnly?.defaultReaction == nil)
        #expect(readOnly?.reacts == false)
    }

    @Test
    func aBarWithoutItsFirstMessageOrWithTheSettingOffHasNoReactions() {
        post(defaultReaction: .unicode(emoji: "🔥"))
        store.update { state in
            state.reactions[id(41)] = [reaction("👍", 2)]
            state.windows[id(41)] = window([50], oldest: true)
        }
        let gone = list(41).postBar
        #expect(gone?.reactions.isEmpty == true)
        #expect(gone?.defaultReaction == nil)

        store.update { state in
            state.windows[id(41)] = window([41, 50], oldest: true)
            state.displaySettings.renderReactions = false
        }
        let hidden = list(41).postBar
        #expect(hidden?.reactions.isEmpty == true)
        #expect(hidden?.reacts == false)
    }

    // MARK: The timeline's changes

    func items(_ rows: [MessageListModel.Row]) -> [MessageTimeline.Item] {
        MessageTimeline(rows: rows, calendar: berlin).items
    }

    @Test
    func aRowWhoseReactionsAloneChangedIsntReloaded() {
        let rows = [row(1, at: noon), row(2, at: noon + 60)]
        let reacted = ReactionRow(
            pills: [ReactionPill(reaction("👍", 1))], adds: true, toggles: true)

        let changes = TimelineChanges(
            from: items(rows), to: items([rows[0], rows[1].reacting(reacted)]))

        #expect(changes.reacted == [2])
        #expect(changes.reloaded.isEmpty)
        #expect(!changes.isEmpty)
    }

    @Test
    func aRowThatChangedBesidesItsReactionsIsReloaded() {
        let before = row(2, at: noon)
        let reacted = ReactionRow(
            pills: [ReactionPill(reaction("👍", 1))], adds: true, toggles: true)
        let after = MessageListModel.Row(
            id: before.id, message: before.message, authorName: "Renamed", reactions: reacted)

        let changes = TimelineChanges(from: items([before]), to: items([after]))

        #expect(changes.reloaded == [1])
        #expect(changes.reacted.isEmpty)
    }

    @Test
    func aRowThatMovedUnderTheLineIsReloadedWhateverItsReactions() {
        let rows = [row(1, at: noon), row(2, at: noon + 60)]
        let reacted = ReactionRow(
            pills: [ReactionPill(reaction("👍", 1))], adds: true, toggles: true)
        let old = MessageTimeline(rows: rows, calendar: berlin).items
        let new = MessageTimeline(
            rows: [rows[0], rows[1].reacting(reacted)], calendar: berlin, newMessages: id(2)
        ).items

        let changes = TimelineChanges(from: old, to: new)

        #expect(changes.reacted.isEmpty)
        #expect(changes.reloaded.contains(3))
    }
}
