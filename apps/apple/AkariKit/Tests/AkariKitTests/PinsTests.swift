import Foundation
import Testing

@testable import AkariKit

func pinned(_ messages: [Message], more: Bool = false, state: PinsState = .loaded) -> PinsPage {
    PinsPage(
        items: messages.map { PinnedMessage(message: $0, pinnedAt: $0.timestamp) }, hasMore: more,
        state: state)
}

// Channel 11 in guild 1 with messages 1 and 2 loaded, its pins 101–103 by Ray; channel 12; DM 20
// with Ray; group DM 22; the DM 21 with the system user 3; thread 31 under 11; forum 13.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class PinsModelTests {
    let store = FakeStore()
    let account: FakeAccount
    let ray = user(2, name: "Ray")

    init() {
        account = FakeAccount(store: store)
        var system = user(3, name: "Discord")
        system.system = true
        store.update { [ray] state in
            state.connection = .online
            state.currentUser = user(1, name: "me")
            state.add(guild(1))
            state.list(
                [channel(11), channel(12), channel(13, kind: .guildForum), thread(31, in: 11)], in: 1)
            state.talk(dm(20, with: 2), group(22, name: "trio", with: [2, 3]), with: ray)
            state.talk(dm(21, with: 3), with: system)
            state.windows[id(11)] = window([1, 2])
            state.show(message(1, in: 11), message(2, in: 11))
        }
    }

    func pins(_ raws: [UInt64], in channel: UInt64 = 11, more: Bool = false, state: PinsState = .loaded)
        -> PinsPage
    {
        pinned(raws.map { message($0, in: channel, by: ray) }, more: more, state: state)
    }

    func session(in channel: UInt64 = 11) -> SessionModel {
        let session = SessionModel(userId: id(1), account: account) { _ in }
        session.start()
        session.open(channel < 30 && channel >= 20 ? .home : .guild(id(1)))
        session.open(channel: id(channel))
        return session
    }

    func opened(_ session: SessionModel) async throws -> PinsModel {
        session.togglePins()
        let pins = try #require(session.pins)
        await pins.open()
        return pins
    }

    func shown(_ pins: PinsModel) -> [UInt64] {
        pins.rows.map(\.message.id.rawValue)
    }

    // MARK: Loading

    @Test
    func openingAsksForTheFirstPageWithTheSpinnerMeanwhile() async throws {
        account.holdPinLoads.withLock { $0 = true }
        account.pinsAfterLoad.withLock { $0[id(11)] = self.pins([103, 102]) }
        let session = session()
        session.togglePins()
        let pins = try #require(session.pins)

        let opening = Task { await pins.open() }
        try await eventually { pins.phase == .loading && self.store.log.calls.contains(.loadPins(id(11))) }
        // The core says so as the page is asked for.
        session.apply(EventBatch([.pinsChanged(channelId: id(11))]))
        #expect(pins.phase == .loading)
        #expect(!pins.loadingMore, "the first page's spinner, not the one under the rows")
        #expect(pins.rows.isEmpty)
        account.pinLoadReplies.send(())
        await opening.value

        #expect(pins.phase == .list)
        #expect(shown(pins) == [103, 102])
        session.close()
    }

    // The core asks Discord only when its list needs it.
    @Test
    func everyOpeningLeavesTheRequestToTheCore() async throws {
        store.update { $0.pins[id(11)] = self.pins([103]) }
        let session = session()

        _ = try await opened(session)
        session.togglePins()
        #expect(session.pins == nil)
        _ = try await opened(session)

        #expect(store.log.calls.filter { $0 == .loadPins(id(11)) }.count == 2)
        session.close()
    }

    @Test
    func rowsAreTheMessagesNewestPinFirstEachWithTheLineOverAReply() async throws {
        let answer = reply(104, to: 1, in: 11, by: ray)
        store.update { state in
            state.pins[id(11)] = pinned([answer, message(102, in: 11, by: self.ray)])
            state.answer(preview(content: "the question"), message: 1, in: 11)
        }
        let pins = try await opened(session())

        #expect(shown(pins) == [104, 102])
        #expect(pins.rows.map(\.authorName) == ["Ray", "Ray"])
        #expect(pins.rows.first?.reply?.messageId == id(1))
        #expect(pins.rows.last?.reply == nil)
    }

    @Test
    func aChannelWithoutPinsSaysSoInTheOfficialWords() async throws {
        account.pinsAfterLoad.withLock { $0[id(11)] = self.pins([]) }
        let pins = try await opened(session())

        #expect(pins.phase == .empty)
        #expect(pins.emptyText == "This channel doesn't have any\npinned messages... yet.")
        #expect(pins.protip == "Users with the 'Pin Messages' permission can pin a message from its context menu.")
        #expect(PinsModel.protipLabel == "Protip:")
    }

    @Test(arguments: [20, 22] as [UInt64])
    func aConversationWithoutPinsSpeaksOfADirectMessage(conversation: UInt64) async throws {
        account.pinsAfterLoad.withLock { $0[id(conversation)] = self.pins([], in: conversation) }
        let pins = try await opened(session(in: conversation))

        #expect(pins.phase == .empty)
        #expect(pins.emptyText == "This direct message doesn't have\nany pinned messages... yet.")
        #expect(pins.protip == "You can pin a message from its context menu.")
    }

    // Akari: the official client shows a failed first page as an empty channel.
    @Test
    func aFailedFirstPageIsntAnEmptyChannelAndTriesAgain() async throws {
        account.pinLoadError.withLock { $0 = .Network(kind: .connect) }
        let pins = try await opened(session())

        #expect(pins.phase == .failed)
        #expect(PinsModel.failedText == "Akari couldn't load the pinned messages.")

        account.pinLoadError.withLock { $0 = nil }
        account.pinsAfterLoad.withLock { $0[id(11)] = self.pins([103]) }
        await pins.retry()

        #expect(store.log.calls.filter { $0 == .loadPins(id(11)) }.count == 2)
        #expect(pins.phase == .list)
        #expect(shown(pins) == [103])
    }

    @Test
    func theEndOfTheListLoadsTheNextPageWhileDiscordHasMore() async throws {
        store.update { $0.pins[id(11)] = self.pins([125, 124], more: true) }
        account.holdPinLoads.withLock { $0 = true }
        let session = session()
        session.togglePins()
        let pins = try #require(session.pins)
        let opening = Task { await pins.open() }
        try await eventually { self.store.log.calls.contains(.loadPins(id(11))) }
        account.pinLoadReplies.send(())
        await opening.value
        #expect(pins.hasMore)

        account.pinsAfterMore.withLock { $0[id(11)] = self.pins([125, 124, 123]) }
        let paging = Task { await pins.reachedEnd() }
        try await eventually { pins.loadingMore }
        await pins.reachedEnd()
        account.pinLoadReplies.send(())
        await paging.value

        #expect(store.log.calls.filter { $0 == .loadMorePins(id(11)) }.count == 1, "one at a time")
        #expect(shown(pins) == [125, 124, 123])
        #expect(!pins.hasMore && !pins.loadingMore)
        await pins.reachedEnd()
        #expect(store.log.calls.filter { $0 == .loadMorePins(id(11)) }.count == 1, "nothing more")
        session.close()
    }

    // Akari: the official client offers nothing after a failed later page.
    @Test
    func aFailedLaterPageKeepsTheRowsAndLeavesTheNextTryToTheButton() async throws {
        store.update { $0.pins[id(11)] = self.pins([125, 124], more: true) }
        let pins = try await opened(session())
        account.pinLoadError.withLock { $0 = .ServerError(status: 502) }

        await pins.reachedEnd()
        #expect(pins.phase == .list)
        #expect(shown(pins) == [125, 124])
        #expect(pins.offersMore)
        await pins.reachedEnd()
        #expect(store.log.calls.filter { $0 == .loadMorePins(id(11)) }.count == 1, "no retry by scrolling")

        account.pinLoadError.withLock { $0 = nil }
        account.pinsAfterMore.withLock { $0[id(11)] = self.pins([125, 124, 123]) }
        await pins.loadMore()

        #expect(store.log.calls.filter { $0 == .loadMorePins(id(11)) }.count == 2)
        #expect(shown(pins) == [125, 124, 123])
        #expect(!pins.offersMore)
    }

    // Every pin of this connection went (a bulk delete) while Discord has more: the core asks for
    // the first page again.
    @Test
    func anEmptyPageWithMoreAsksForTheNext() async throws {
        store.update { $0.pins[id(11)] = self.pins([], more: true) }
        let pins = try await opened(session())
        #expect(pins.phase == .list)
        #expect(pins.offersMore)

        await pins.loadMore()

        #expect(store.log.calls.filter { $0 == .loadMorePins(id(11)) }.count == 1)
    }

    // The view asks on every update near the end, and the rows still end there.
    @Test
    func theEndDoesntAskAgainAfterAPageThatChangedNothing() async throws {
        store.update { $0.pins[id(11)] = self.pins([], more: true) }
        let pins = try await opened(session())

        await pins.reachedEnd()
        await pins.reachedEnd()
        #expect(store.log.calls.filter { $0 == .loadMorePins(id(11)) }.count == 1)
        #expect(pins.offersMore)
        account.pinsAfterMore.withLock { $0[id(11)] = self.pins([103], more: true) }
        await pins.loadMore()
        await pins.reachedEnd()

        #expect(
            store.log.calls.filter { $0 == .loadMorePins(id(11)) }.count == 3,
            "the button asks, and the end once rows came")
    }

    @Test
    func theButtonForMoreShowsOnlyWhileNothingLoads() async throws {
        store.update { $0.pins[id(11)] = self.pins([125], more: true) }
        account.holdPinLoads.withLock { $0 = true }
        let session = session()
        session.togglePins()
        let pins = try #require(session.pins)
        let opening = Task { await pins.open() }
        try await eventually { self.store.log.calls.contains(.loadPins(id(11))) }
        // The core says so as the page is asked for.
        session.apply(EventBatch([.pinsChanged(channelId: id(11))]))
        #expect(!pins.offersMore)
        #expect(pins.loadingMore)
        account.pinLoadReplies.send(())
        await opening.value

        #expect(pins.offersMore)
        session.close()
    }

    // MARK: Following the core

    @Test
    func pinsChangedRereadsTheListAndOtherChannelsDont() async throws {
        store.update { $0.pins[id(11)] = self.pins([103]) }
        let session = session()
        let pins = try await opened(session)

        store.update { $0.pins[id(11)] = self.pins([104, 103]) }
        session.apply(EventBatch([.pinsChanged(channelId: id(12))]))
        #expect(shown(pins) == [103])
        session.apply(EventBatch([.pinsChanged(channelId: id(11))]))

        #expect(shown(pins) == [104, 103])
        session.close()
    }

    @Test
    func aPinnedRepliesLineFollowsItsTarget() async throws {
        let answer = reply(104, to: 1, in: 11, by: ray)
        store.update { $0.pins[id(11)] = pinned([answer]) }
        let session = session()
        let pins = try await opened(session)
        #expect(pins.rows.first?.reply?.target == .notLoaded)

        store.update { $0.answer(.deleted, message: 1, in: 11) }
        session.apply(EventBatch([.replyTargetChanged(channelId: id(11), targetId: id(1))]))

        #expect(pins.rows.first?.reply?.target == .deleted)
        session.close()
    }

    // A pin the gateway couldn't place (no copy of its message): the dot says so, and the core
    // asks again where Discord named a newer pin than the list has.
    @Test
    func aNewPinWhileOpenAsksTheCoreForTheList() async throws {
        store.update { $0.pins[id(11)] = self.pins([103]) }
        let session = session()
        let pins = try await opened(session)
        account.pinsAfterLoad.withLock { $0[id(11)] = self.pins([104, 103]) }

        store.update { $0.unreadPins = [id(11)] }
        session.apply(EventBatch([.pinsUnreadChanged(channelId: id(11))]))

        try await eventually { self.shown(pins) == [104, 103] }
        #expect(store.log.calls.filter { $0 == .loadPins(id(11)) }.count == 2)
        #expect(store.log.calls.filter { $0 == .ackPins(id(11)) }.count == 2)
        session.close()
    }

    // A new session leaves the list shown but not loaded.
    @Test
    func aListThatIsntLoadedAnyMoreIsAskedForAgain() async throws {
        store.update { $0.pins[id(11)] = self.pins([103]) }
        let session = session()
        let pins = try await opened(session)

        store.update { $0.pins[id(11)] = self.pins([103], state: .notLoaded) }
        session.apply(EventBatch([.pinsChanged(channelId: id(11))]))

        try await eventually { self.store.log.calls.filter { $0 == .loadPins(id(11)) }.count == 2 }
        #expect(shown(pins) == [103], "an earlier connection's pins show meanwhile")
        session.close()
    }

    // One at a time; the core asks Discord again only where the list still needs it.
    @Test
    func batchesWhileTheListLoadsAskTheCoreOnceMoreAfterIt() async throws {
        store.update { $0.pins[id(11)] = self.pins([103]) }
        let session = session()
        let pins = try await opened(session)
        account.holdPinLoads.withLock { $0 = true }
        account.pinsAfterLoad.withLock { $0[id(11)] = self.pins([104, 103]) }

        store.update { $0.pins[id(11)] = self.pins([103], state: .notLoaded) }
        session.apply(EventBatch([.pinsChanged(channelId: id(11))]))
        session.apply(EventBatch([.ready]))
        session.apply(EventBatch([.pinsUnreadChanged(channelId: id(11))]))

        try await eventually { self.store.log.calls.filter { $0 == .loadPins(id(11)) }.count == 2 }
        account.pinLoadReplies.send(())
        try await eventually { self.store.log.calls.filter { $0 == .loadPins(id(11)) }.count == 3 }
        #expect(shown(pins) == [104, 103])
        account.pinLoadReplies.send(())
        try await eventually { self.store.pins(channelId: id(11)).state == .loaded }
        #expect(store.log.calls.filter { $0 == .loadPins(id(11)) }.count == 3)
        session.close()
    }

    // The core drops a page asked before a new session, which leaves the list `.notLoaded`.
    @Test
    func aNewSessionWhileTheFirstPageLoadsAsksAgainOnceItEnds() async throws {
        account.holdPinLoads.withLock { $0 = true }
        let session = session()
        session.togglePins()
        let pins = try #require(session.pins)
        let opening = Task { await pins.open() }
        try await eventually { self.store.log.calls.filter { $0 == .loadPins(id(11)) }.count == 1 }
        account.pinsAfterLoad.withLock { $0[id(11)] = self.pins([103]) }

        store.update { $0.pins[id(11)] = self.pins([], state: .notLoaded) }
        session.apply(EventBatch([.ready]))
        account.pinLoadReplies.send(())
        try await eventually { self.store.log.calls.filter { $0 == .loadPins(id(11)) }.count == 2 }
        account.pinLoadReplies.send(())
        await opening.value

        try await eventually { self.shown(pins) == [103] }
        #expect(pins.phase == .list)
        session.close()
    }

    @Test
    func openingAcksAndANewPinWhileOpenAcksAgain() async throws {
        store.update { $0.unreadPins = [id(11)] }
        let session = session()
        _ = try await opened(session)
        #expect(store.log.calls.filter { $0 == .ackPins(id(11)) }.count == 1)

        session.apply(EventBatch([.pinsUnreadChanged(channelId: id(12))]))
        #expect(store.log.calls.filter { $0 == .ackPins(id(11)) }.count == 1)
        session.apply(EventBatch([.pinsUnreadChanged(channelId: id(11))]))

        #expect(store.log.calls.filter { $0 == .ackPins(id(11)) }.count == 2)
        session.apply(EventBatch([.ready]))
        #expect(store.log.calls.filter { $0 == .ackPins(id(11)) }.count == 3, "a new session's dot")
        session.togglePins()
        session.apply(EventBatch([.pinsUnreadChanged(channelId: id(11))]))
        #expect(store.log.calls.filter { $0 == .ackPins(id(11)) }.count == 3, "not once it closed")
        session.close()
    }

    // The view opens it as it shows.
    @Test
    func aPopoutNotShownYetAcksNothing() {
        let session = session()
        session.togglePins()

        session.apply(EventBatch([.pinsUnreadChanged(channelId: id(11))]))

        #expect(!store.log.calls.contains(.ackPins(id(11))))
        #expect(!store.log.calls.contains(.loadPins(id(11))))
        session.close()
    }

    // MARK: A row's buttons

    @Test
    func aRowsUnpinIsWhereTheCoreAllowsIt() async throws {
        store.update { state in
            state.pins[id(11)] = self.pins([103, 102])
            state.allow(allowed(unpin: true), message: 103, in: 11)
        }
        let session = session()
        let pins = try await opened(session)

        #expect(pins.rows.map(pins.canUnpin) == [true, false])
        pins.unpin(pins.rows[1], shift: true)
        pins.unpin(pins.rows[1], shift: false)
        pins.unpin(pins.rows[0], shift: true)
        try await eventually { self.store.log.calls.contains(.unpin(id(11), id(103))) }
        #expect(!store.log.calls.contains(.unpin(id(11), id(102))))
        #expect(session.pinQuestion == nil)
        session.close()
    }

    @Test
    func jumpClosesThePopoutUnlessShiftIsHeld() async throws {
        store.update { $0.pins[id(11)] = self.pins([103, 2]) }
        let session = session()
        let list = try #require(session.messages)
        let pins = try await opened(session)

        pins.jump(to: try #require(pins.rows.last), shift: true)
        #expect(session.pins === pins)
        await finished(session.jumping)
        #expect(list.messageJump?.messageId == id(2))

        pins.jump(to: try #require(pins.rows.first), shift: false)
        #expect(session.pins == nil)
        await finished(session.jumping)
        #expect(list.messageJump?.messageId == id(103))
        session.close()
    }

    @Test
    func unpinAsksTheSessionUnlessShiftIsHeld() async throws {
        store.update { state in
            state.pins[id(11)] = self.pins([103])
            state.allow(allowed(unpin: true), message: 103, in: 11)
        }
        let session = session()
        let pins = try await opened(session)
        let row = try #require(pins.rows.first)

        pins.unpin(row, shift: false)
        #expect(session.pinQuestion?.row.message.id == id(103))
        #expect(session.pinQuestion?.pins == false)
        #expect(!store.log.calls.contains(.unpin(id(11), id(103))))
        session.answerPinQuestion(true)
        try await eventually { self.store.log.calls.contains(.unpin(id(11), id(103))) }
        try await eventually { pins.unpinning.isEmpty }

        pins.unpin(row, shift: true)
        try await eventually { self.store.log.calls.filter { $0 == .unpin(id(11), id(103)) }.count == 2 }
        #expect(session.pinQuestion == nil)
        session.close()
    }

    @Test
    func anUnpinOnItsWayKeepsItsButtonBusy() async throws {
        store.update { state in
            state.pins[id(11)] = self.pins([103])
            state.allow(allowed(unpin: true), message: 103, in: 11)
        }
        account.holdPins.withLock { $0 = true }
        let session = session()
        let pins = try await opened(session)
        let row = try #require(pins.rows.first)

        pins.unpin(row, shift: true)
        try await eventually { pins.unpinning.contains(id(103)) }
        pins.unpin(row, shift: true)
        pins.unpin(row, shift: false)
        #expect(session.pinQuestion == nil)
        account.pinReplies.send(())

        try await eventually { pins.unpinning.isEmpty }
        #expect(store.log.calls.filter { $0 == .unpin(id(11), id(103)) }.count == 1)
        session.close()
    }

    @Test
    func aRefusedUnpinOffersToTryAgain() async throws {
        store.update { state in
            state.pins[id(11)] = self.pins([103])
            state.allow(allowed(unpin: true), message: 103, in: 11)
        }
        account.pinError.withLock { $0 = .Forbidden }
        let session = session()
        let pins = try await opened(session)

        pins.unpin(try #require(pins.rows.first), shift: true)
        try await eventually { session.messageConfirmation != nil }

        #expect(session.messageConfirmation?.title == "The Pin Is Stuck!")
        #expect(session.messageConfirmation?.confirm == "Try Again")
        account.pinError.withLock { $0 = nil }
        session.answerMessageConfirmation(true)
        try await eventually { self.store.log.calls.filter { $0 == .unpin(id(11), id(103)) }.count == 2 }
        session.close()
    }
}

// The header's pins button: where it shows, its dot, ⌘P and a pin line's link.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class PinsButtonTests {
    let store = FakeStore()
    let account: FakeAccount
    let session: SessionModel

    init() {
        account = FakeAccount(store: store)
        var system = user(3, name: "Discord")
        system.system = true
        store.update { state in
            state.connection = .online
            state.currentUser = user(1, name: "me")
            state.add(guild(1))
            state.list(
                [
                    channel(11), channel(12, kind: .guildNews), channel(13, kind: .guildForum),
                    thread(31, in: 11),
                ], in: 1)
            state.talk(dm(20, with: 2), group(22, name: "trio", with: [2, 3]), with: user(2, name: "Ray"))
            state.talk(dm(21, with: 3), with: system)
        }
        session = SessionModel(userId: id(1), account: account) { _ in }
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(11))
    }

    @Test
    func theButtonIsForChannelsThreadsAndConversations() {
        #expect(session.pinsChannel == id(11))
        session.open(channel: id(12))
        #expect(session.pinsChannel == id(12))
        session.open(channel: id(31))
        #expect(session.pinsChannel == id(31))
        session.open(channel: id(13))
        #expect(session.pinsChannel == nil, "a forum's page")
        session.open(.home)
        session.open(channel: id(20))
        #expect(session.pinsChannel == id(20))
        session.open(channel: id(22))
        #expect(session.pinsChannel == id(22))
        session.open(channel: id(21))
        #expect(session.pinsChannel == nil, "the system DM")
        session.close()
    }

    @Test
    func theDotFollowsTheCore() {
        store.update { $0.unreadPins = [id(12)] }
        #expect(!session.pinsUnread)
        session.open(channel: id(12))
        #expect(session.pinsUnread)

        store.update { $0.unreadPins = [] }
        session.apply(EventBatch([.pinsUnreadChanged(channelId: id(11))]))
        #expect(session.pinsUnread, "another channel's dot")
        session.apply(EventBatch([.pinsUnreadChanged(channelId: id(12))]))
        #expect(!session.pinsUnread)

        // A mute and a join come as settings and channel changes.
        store.update { $0.unreadPins = [id(12)] }
        session.apply(EventBatch([.guildSettingsChanged(guildId: id(1))]))
        #expect(session.pinsUnread)
        store.update { $0.unreadPins = [] }
        session.apply(EventBatch([.channelUpdated(channelId: id(12), guildId: id(1))]))
        #expect(!session.pinsUnread)
        session.close()
    }

    @Test
    func aNewSessionRereadsTheDot() {
        store.update { $0.unreadPins = [id(11)] }

        session.apply(EventBatch([.ready]))

        #expect(session.pinsUnread)
        session.close()
    }

    @Test
    func openingThePopoutTakesTheDot() async throws {
        store.update { $0.unreadPins = [id(11)] }
        session.apply(EventBatch([.pinsUnreadChanged(channelId: id(11))]))
        #expect(session.pinsUnread)

        session.togglePins()
        await session.pins?.open()
        session.apply(EventBatch([.pinsUnreadChanged(channelId: id(11))]))

        #expect(!session.pinsUnread)
        session.close()
    }

    @Test
    func toggleOpensAndClosesAndAnotherChannelCloses() {
        session.togglePins()
        #expect(session.pins?.channelId == id(11))
        session.togglePins()
        #expect(session.pins == nil)

        session.togglePins()
        session.open(channel: id(12))
        #expect(session.pins == nil)
        session.close()
    }

    @Test
    func nothingOpensWhereThereIsNoButtonOrBehindTheAgeGate() {
        session.open(channel: id(13))
        session.togglePins()
        #expect(session.pins == nil)

        store.update { $0.ageGates[id(11)] = .ask(server: false) }
        session.open(channel: id(11))
        #expect(session.pinsChannel == id(11))
        #expect(session.pinsDisabled)
        session.togglePins()
        #expect(session.pins == nil)
        session.close()
    }

    @Test
    func thePinsAndTheThreadsPopoutsTakeEachOthersPlace() {
        session.openThreadBrowser(of: id(11), as: .popout)
        session.togglePins()
        #expect(session.threadBrowser == nil)
        #expect(session.pins != nil)

        session.openThreadBrowser(of: id(11), as: .popout)
        #expect(session.pins == nil)
        session.close()
    }

    // As the official client: the line's channel is selected first, then the popout toggles.
    @Test
    func aPinLinesLinkTogglesItsChannelsPopout() {
        session.togglePins(of: id(11))
        #expect(session.pins?.channelId == id(11))
        session.togglePins(of: id(11))
        #expect(session.pins == nil)

        session.togglePins(of: id(31))
        #expect(session.openChannelId == id(31))
        #expect(session.pins?.channelId == id(31))
        session.togglePins()
        // Not known yet: it's fetched before it opens, and the open channel's pins stay closed.
        session.togglePins(of: id(99))
        #expect(session.pins == nil)
        session.close()
    }
}
