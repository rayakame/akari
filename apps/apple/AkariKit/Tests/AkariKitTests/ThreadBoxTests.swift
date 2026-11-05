import Foundation
import Testing

@testable import AkariKit

// Channel 10's message 20 started thread 20; Mira's 25 is its newest message.
@MainActor @Suite(.timeLimit(.minutes(1)))
struct ThreadBoxTests {
    let store = FakeStore()
    let account: FakeAccount
    let mira = user(3, name: "Mira")
    let lastMessage = Date(timeIntervalSince1970: 1_700_000_000)

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.channels[id(10)] = channel(10)
            state.show(
                message(19, in: 10),
                message(20, in: 10, by: user(2, name: "Ray"), content: "test", hasThread: true),
                message(21, in: 10))
            state.windows[id(10)] = window([19, 20, 21])
        }
        startThread(count: 16)
        store.update {
            $0.latestMessages[id(20)] = message(25, in: 20, by: mira, content: "the newest")
        }
    }

    func startThread(
        _ raw: UInt64 = 20, name: String = "t1", count: UInt32?, archived: Bool = false,
        lastMessage: UInt64? = 25, createdAt: Date? = nil
    ) {
        store.update { state in
            var started = thread(raw, in: 10, name: name)
            started.thread?.messageCount = count
            started.thread?.archived = archived
            started.thread?.lastMessageId = lastMessage.map(id)
            started.thread?.createdAt = createdAt
            state.channels[id(raw)] = started
        }
    }

    func makeModel() -> MessageListModel {
        MessageListModel(channelId: id(10), userId: id(1), account: account, store: store)
    }

    func box(_ model: MessageListModel, of raw: UInt64 = 20) -> ThreadBox? {
        model.rows.first { $0.id == id(raw) }?.threadBox
    }

    @Test
    func aMessageWithAThreadShowsItsBox() {
        let model = makeModel()

        let box = box(model)
        #expect(box?.threadId == id(20))
        #expect(box?.name == "t1")
        #expect(box?.link == "16 Messages ›")
        #expect(
            box?.bottom
                == .latest(message(25, in: 20, by: mira, content: "the newest"), authorName: "Mira"))
        #expect(box?.time == Self.time(of: 25))
        #expect(model.rows.filter { $0.threadBox != nil }.map(\.id) == [id(20)])
    }

    @Test
    func aMessageWithoutTheThreadsFlagHasNoBox() {
        store.update { $0.messages[id(20)]?.hasThread = false }

        #expect(box(makeModel()) == nil)
    }

    @Test
    func aThreadTheStoreDoesntHaveHasNoBox() {
        store.update { $0.channels[id(20)] = nil }

        #expect(box(makeModel()) == nil)
    }

    // A thread started without a message has the ID of its "started a thread" line.
    @Test
    func theLineAboutAThreadStartedWithoutAMessageHasItsBox() {
        store.update { state in
            state.show(
                message(
                    22, in: 10, by: user(2, name: "Ray"), content: "t2", kind: .threadCreated,
                    reference: MessageReference(messageId: nil, channelId: id(22), guildId: id(1)),
                    hasThread: true))
            state.windows[id(10)] = window([19, 20, 21, 22])
        }
        startThread(22, name: "t2", count: 1, lastMessage: 23)

        let box = box(makeModel(), of: 22)

        #expect(box?.name == "t2")
        #expect(box?.link == "1 Message ›")
    }

    @Test
    func theCountsLinkReadsAsDiscordsDoes() {
        #expect(ThreadBox.link(count: 1, threadId: id(20)) == "1 Message ›")
        #expect(ThreadBox.link(count: 2, threadId: id(20)) == "2 Messages ›")
        #expect(ThreadBox.link(count: 0, threadId: id(20)) == "See Thread ›")
        #expect(ThreadBox.link(count: nil, threadId: id(20)) == "See Thread ›")
        // Created before 2022-07-01, when Discord stopped counting at 50.
        let old: ChannelId = id(992_549_565_104_128_000)
        let new: ChannelId = id(992_549_565_104_128_001)
        #expect(ThreadBox.link(count: 100_000, threadId: new) == "100k+ Messages ›")
        #expect(ThreadBox.link(count: 50, threadId: old) == "50+ Messages ›")
        #expect(ThreadBox.link(count: 49, threadId: old) == "49 Messages ›")
        #expect(ThreadBox.link(count: 50, threadId: new) == "50 Messages ›")
    }

    @Test
    func anArchivedThreadHasNoRecentMessagesAndNoTime() {
        startThread(count: 16, archived: true)

        let box = box(makeModel())

        #expect(box?.bottom == .noRecentMessages)
        #expect(box?.time == nil)
    }

    @Test
    func aThreadWithNoMessagesSaysSoWithoutATime() {
        startThread(count: 0)

        let box = box(makeModel())

        #expect(box?.bottom == .noMessages)
        #expect(box?.time == nil)
    }

    @Test
    func aThreadWithNoRecentMessagesSaysSoWithItsTime() {
        store.update { $0.latestMessages[id(20)] = nil }

        let box = box(makeModel())

        #expect(box?.bottom == .noRecentMessages)
        #expect(box?.time == Self.time(of: 25))
    }

    @Test
    func theTimeFallsBackToTheThreadsCreationThenItsId() {
        let created = Date(timeIntervalSince1970: 1_800_000_000)
        startThread(count: 16, lastMessage: nil, createdAt: created)
        #expect(box(makeModel())?.time == created)

        startThread(count: 16, lastMessage: nil)
        #expect(box(makeModel())?.time == Self.time(of: 20))
    }

    @Test
    func theRelativeTimeIsShort() {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        func time(_ seconds: TimeInterval) -> ThreadBox.RelativeTime {
            ThreadBox.relativeTime(
                since: now - seconds, now: now, locale: Locale(identifier: "en_US"),
                timeZone: .gmt)
        }

        #expect(time(0).text == "1m ago")
        #expect(time(59).text == "1m ago")
        #expect(time(60).text == "1m ago")
        #expect(time(25 * 60).text == "25m ago")
        #expect(time(3600).text == "1h ago")
        #expect(time(23 * 3600 + 3599).text == "23h ago")
        #expect(time(86_400).text == "1d ago")
        #expect(time(29 * 86_400).text == "29d ago")
        #expect(time(30 * 86_400).text == ">30d ago")
        #expect(time(359 * 86_400).text == ">30d ago")
        #expect(time(360 * 86_400).text == "January 20, 2026")
    }

    @Test
    func theRelativeTimeSaysWhenItChanges() {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        func next(_ seconds: TimeInterval) -> Date? {
            ThreadBox.relativeTime(
                since: now - seconds, now: now, locale: Locale(identifier: "en_US"),
                timeZone: .gmt
            ).nextChange
        }

        #expect(next(30) == now + 90)
        #expect(next(25 * 60 + 10) == now + 50)
        #expect(next(3600) == now + 3600)
        #expect(next(86_400 + 5) == now + 86_400 - 5)
        #expect(next(360 * 86_400) == nil)
    }

    @Test
    func aBoxFollowsItsMessagesUpdate() {
        let model = makeModel()
        startThread(count: 17, lastMessage: 26)
        store.update {
            $0.latestMessages[id(20)] = message(26, in: 20, by: mira, content: "newer")
        }

        model.apply(EventBatch([.messageUpdated(channelId: id(10), messageId: id(20))]))

        #expect(box(model)?.link == "17 Messages ›")
        #expect(
            box(model)?.bottom
                == .latest(message(26, in: 20, by: mira, content: "newer"), authorName: "Mira"))
    }

    @Test
    func aThreadArrivingGivesItsMessageTheBox() {
        store.update { $0.channels[id(20)] = nil }
        let model = makeModel()
        #expect(box(model) == nil)
        startThread(count: 16)

        model.apply(EventBatch([.channelAdded(channelId: id(20), guildId: id(1))]))

        #expect(box(model)?.name == "t1")
    }

    // Inside the thread, the message it started from shows no box of its own.
    @Test
    func theStartersOriginalShowsNoBox() {
        store.update { state in
            state.show(message(31, in: 20, content: "", kind: .threadStarterMessage))
            state.windows[id(20)] = window([31], oldest: true)
            state.starters[id(20)] = .loaded(
                message: message(20, in: 10, content: "test", hasThread: true))
        }

        let model = MessageListModel(channelId: id(20), userId: id(1), account: account, store: store)

        #expect(model.rows.first?.threadBox == nil)
    }

    @Test
    func anEmptyLatestMessageShowsWhatItHolds() {
        func placeholder(_ edit: (inout Message) -> Void) -> String? {
            var latest = message(25, in: 20, content: "")
            edit(&latest)
            return ThreadBox.placeholder(for: latest)
        }

        #expect(placeholder { _ in } == "Click to see attachment")
        #expect(
            placeholder {
                $0.attachments = [
                    Attachment(
                        id: id(1), filename: "a.png", title: nil, description: nil,
                        contentType: nil, size: 1, kind: .image, width: 4, height: 4,
                        placeholder: nil, spoiler: false, animated: false,
                        key: .attachment(channelId: id(20), messageId: id(25), attachmentId: id(1)),
                        display: .media, cover: nil)
                ]
            } == "Click to see attachment")
        let wave = Sticker(
            id: id(7), name: "wave", format: .png, key: .sticker(stickerId: id(7), format: .png))
        #expect(placeholder { $0.stickers = [wave] } == "Click to see sticker")
        #expect(placeholder { $0.kind = .chatInputCommand } == "Click to see command")
        #expect(placeholder { $0.kind = .contextMenuCommand } == "Click to see command")
        #expect(placeholder { $0.componentsV2 = true } == "Click to see message")
        #expect(placeholder { $0.kind = .channelPinnedMessage } == "Message pinned.")
        #expect(placeholder { $0.content = "hi" } == nil)
    }

    private static func time(of raw: UInt64) -> Date {
        Date(timeIntervalSince1970: Double((raw >> 22) + 1_420_070_400_000) / 1000)
    }
}
