import Foundation
import Testing

@testable import AkariKit

// A thread (20) under channel 10, started from 10's message 20 by user 2; its first message (31)
// points back at that one.
@MainActor @Suite(.timeLimit(.minutes(1)))
struct StarterMessageTests {
    let store = FakeStore()
    let account: FakeAccount
    let thread: ChannelId = id(20)
    let original = message(20, in: 10, by: user(2, name: "Ray"), content: "the original")

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            var thread = AkariKitTests.thread(20, in: 10)
            thread.thread?.ownerId = id(2)
            state.channels[id(20)] = thread
            state.permissions[id(20)] = [.viewChannel, .sendMessages]
            state.show(
                message(
                    31, in: 20, by: user(2, name: "Ray"), content: "",
                    kind: .threadStarterMessage),
                message(32, in: 20, by: user(3, name: "Mira")))
            state.windows[id(20)] = window([31, 32], oldest: true)
        }
    }

    func makeModel() -> MessageListModel {
        MessageListModel(channelId: thread, userId: id(1), account: account, store: store)
    }

    func starts(_ starter: StarterMessage?) {
        store.update { $0.starters[id(20)] = starter }
    }

    @Test
    func aStarterMessageShowsAsTheOriginalMessage() {
        starts(.loaded(message: original))

        let model = makeModel()

        #expect(model.rows.first?.message.kind == .threadStarterMessage)
        #expect(model.rows.first?.starter == .loaded(original))
        #expect(model.rows.first?.shown == original)
        #expect(model.rows.first?.notice == nil)
        #expect(model.rows.last?.starter == nil)
        #expect(model.rows.last?.shown == model.rows.last?.message)
    }

    @Test
    func anUnavailableStarterSaysSorryWeCouldntLoadIt() {
        starts(.unavailable)

        let model = makeModel()

        #expect(model.rows.first?.starter == .unavailable)
        #expect(
            model.rows.first?.notice
                == "Sorry, we couldn't load the first message in this thread")
        #expect(
            model.rows.first?.noticeParts
                == [.text("Sorry, we couldn't load the first message in this thread")])
    }

    @Test
    func aStarterSettlingRereadsItsRow() {
        starts(.loading)
        let model = makeModel()
        #expect(model.rows.first?.starter == .loading)
        starts(.loaded(message: original))
        store.log.forget()

        model.apply(EventBatch([.messageUpdated(channelId: thread, messageId: id(31))]))

        #expect(model.rows.first?.starter == .loaded(original))
        #expect(
            store.reads.filter { if case .starterMessage = $0 { true } else { false } }
                == [.starterMessage(thread)])
    }

    @Test
    func onlyAThreadsFirstMessageReadsItsStarter() {
        starts(.loaded(message: original))
        let model = makeModel()
        store.log.forget()

        model.apply(EventBatch([.messageUpdated(channelId: thread, messageId: id(32))]))

        #expect(!store.reads.contains(.starterMessage(thread)))
        #expect(model.rows.first?.starter == .loaded(original))
    }

    @Test
    func theThreadsOwnerIsNamedFromItsMessages() {
        starts(.loaded(message: original))

        #expect(makeModel().threadOwner == "Ray")
    }

    @Test
    func theThreadsOwnerIsNamedFromTheStoreFirst() {
        store.update { $0.users[id(2)] = user(2, name: "Ray from READY") }

        #expect(makeModel().threadOwner == "Ray from READY")
    }

    // `user(id:)` holds DM recipients only; the user may not have posted in their own thread.
    @Test
    func theUsersOwnThreadIsNamedFromTheCurrentUser() {
        store.update { state in
            state.channels[id(20)]?.thread?.ownerId = id(1)
            state.currentUser = user(1, name: "Me")
        }

        #expect(makeModel().threadOwner == "Me")
    }

    // SessionView reads it, so an equal name mustn't make the session's view render again.
    @Test
    func aRereadThatKeepsTheOwnerDoesntNotify() {
        starts(.loaded(message: original))
        let model = makeModel()
        let changed = Locked(false)
        withObservationTracking {
            _ = model.threadOwner
        } onChange: {
            changed.withLock { $0 = true }
        }

        model.apply(EventBatch([.messageUpdated(channelId: thread, messageId: id(32))]))

        #expect(!changed.current)
        #expect(model.threadOwner == "Ray")
    }

    @Test
    func aThreadWithoutAKnownOwnerHasNoName() {
        store.update { state in
            state.channels[id(20)]?.thread?.ownerId = id(9)
        }

        #expect(makeModel().threadOwner == nil)
    }
}

// The timeline's part: where the starter's row goes and how it groups.
struct StarterTimelineTests {
    let noon = berlinTime(2026, 10, 10, 12, 0)
    let ray = user(2, name: "Ray")

    func starter(_ starter: MessageListModel.Row.Starter) -> MessageListModel.Row {
        let row = row(31, by: ray, at: noon, kind: .threadStarterMessage)
        return MessageListModel.Row(id: row.id, message: row.message, starter: starter)
    }

    func original() -> Message {
        row(20, by: ray, at: noon - 60).message
    }

    @Test
    func aLoadingStarterHasNoRow() {
        let shown = MessageTimeline(
            rows: [starter(.loading), row(32, by: ray, at: noon + 60)], calendar: berlin,
            newMessages: id(31))

        #expect(shown.shape == ["day", "new", "+32"])
        #expect(shown.items.map(\.id).contains(.newMessages(id(31))))
    }

    // A new thread from a message: the unread bar counts its starter, so the line stays, after
    // the thread's start until the starter shows.
    @Test
    func aLoadingStarterInTheLastRowKeepsTheNewMessagesLine() {
        let shown = MessageTimeline(
            rows: [starter(.loading)], calendar: berlin, edges: [.beginning],
            newMessages: id(31))

        #expect(shown.shape == ["beginning", "new"])
    }

    @Test
    func theMessageAfterTheStarterStartsItsOwnGroup() {
        let shown = MessageTimeline(
            rows: [starter(.loaded(original())), row(32, by: ray, at: noon + 60)],
            calendar: berlin)

        #expect(shown.shape == ["day", "+31", "+32"])
    }

    @Test
    func aLoadedStarterTakesItsOriginalsDay() {
        let yesterday = row(20, by: ray, at: noon - 86_400).message
        let shown = MessageTimeline(
            rows: [starter(.loaded(yesterday)), row(32, by: ray, at: noon)], calendar: berlin)

        #expect(shown.shape == ["day", "+31", "day", "+32"])
    }

    @Test
    func anUnavailableStarterIsANotice() {
        let shown = MessageTimeline(
            rows: [starter(.unavailable), row(32, by: ray, at: noon + 60)], calendar: berlin)

        #expect(shown.shape == ["day", "+31", "+32"])
    }
}

// "started a thread" lines in channel 10 for thread 21: the name opens the thread, fetched first
// when the store doesn't have it, unless Discord refused it or it's gone.
@MainActor @Suite(.timeLimit(.minutes(1)))
struct ThreadLineTests {
    let store = FakeStore()
    let account: FakeAccount

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.channels[id(10)] = channel(10, guild: 1)
            state.permissions[id(10)] = [.viewChannel, .sendMessages]
            state.show(
                message(
                    41, by: user(2, name: "Ray"), content: "t2", kind: .threadCreated,
                    reference: MessageReference(messageId: nil, channelId: id(21), guildId: id(1))))
            state.windows[id(10)] = window([41])
        }
    }

    func makeModel() -> MessageListModel {
        MessageListModel(channelId: id(10), userId: id(1), account: account, store: store)
    }

    func thread(of model: MessageListModel) -> Message.NoticePart? {
        model.rows.first?.noticeParts?.first { if case .thread = $0 { true } else { false } }
    }

    @Test
    func aLineForAThreadTheStoreKnowsLinksIt() {
        store.update { $0.channels[id(21)] = AkariKitTests.thread(21, in: 10) }

        #expect(thread(of: makeModel()) == .thread(name: "t2", id: id(21)))
    }

    @Test
    func aLineForAThreadTheStoreDoesntHaveLinksIt() {
        #expect(thread(of: makeModel()) == .thread(name: "t2", id: id(21)))
    }

    @Test
    func aLineForAnUnavailableThreadDoesntLinkIt() {
        store.update { $0.unavailableThreads.insert(id(21)) }

        let model = makeModel()

        #expect(thread(of: model) == .thread(name: "t2", id: nil))
        #expect(model.rows.first?.notice == "Ray started a thread: t2. See all threads.")
    }

    @Test
    func aThreadBecomingUnavailableUnlinksItsLine() {
        let model = makeModel()
        store.update { $0.unavailableThreads.insert(id(21)) }

        model.apply(EventBatch([.threadUnavailable(guildId: id(1), threadId: id(21))]))

        #expect(thread(of: model) == .thread(name: "t2", id: nil))
    }

    @Test
    func aThreadArrivingLinksItsLineAgain() {
        store.update { $0.unavailableThreads.insert(id(21)) }
        let model = makeModel()
        store.update { $0.channels[id(21)] = AkariKitTests.thread(21, in: 10) }

        model.apply(EventBatch([.channelAdded(channelId: id(21), guildId: id(1))]))

        #expect(thread(of: model) == .thread(name: "t2", id: id(21)))
    }
}
