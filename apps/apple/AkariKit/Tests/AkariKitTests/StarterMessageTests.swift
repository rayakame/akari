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
