import Foundation
import Testing

@testable import AkariKit

@MainActor @Suite(.timeLimit(.minutes(1)))
struct ComposerModelTests {
    let store = FakeStore()
    let account: FakeAccount
    let here: ChannelId = id(10)

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.channels[id(10)] = channel(10, guild: 1)
            state.permissions[id(10)] = [.viewChannel, .sendMessages]
        }
    }

    func makeComposer(drafts: Drafts = Drafts()) -> ComposerModel {
        let composer = ComposerModel(
            channelId: here, account: account, store: store, drafts: drafts)
        store.log.forget()
        return composer
    }

    var sends: [CallLog.Call] {
        store.log.actions.filter { call in
            if case .send = call { true } else { false }
        }
    }

    func slowmode(until: Date?, exempt: Bool = false) {
        store.update {
            $0.slowmodes[id(10)] = Slowmode(interval: 10, exempt: exempt, until: until)
        }
    }

    @Test
    func submitSendsTheTrimmedDraftAndClearsItAtOnce() async {
        let composer = makeComposer()
        account.holdSends.withLock { $0 = true }
        composer.draft = "  hi\n "

        async let submitted: Void = composer.submit()
        await account.sendReplies.pulled(1)
        #expect(composer.draft == "")
        account.sendReplies.send(nil)
        await submitted

        #expect(sends == [.send(here, "hi")])
        #expect(composer.problem == nil)
    }

    @Test
    func blankDraftsSendNothing() async {
        let composer = makeComposer()
        for blank in ["", " \n\t"] {
            composer.draft = blank
            await composer.submit()
        }

        #expect(sends.isEmpty)
        #expect(composer.problem == nil)
    }

    @Test
    func aDraftOverTheLimitStaysAndSaysByHowMuch() async {
        let composer = makeComposer()
        composer.draft = String(repeating: "a", count: 2001)

        await composer.submit()

        #expect(sends.isEmpty)
        #expect(composer.problem == .tooLong(by: 1, limit: 2000))
        #expect(composer.draft.count == 2001)
        composer.draft.removeLast()
        #expect(composer.problem == nil)
    }

    @Test
    func theCounterShowsFromTwoHundredLeft() {
        let composer = makeComposer()
        composer.draft = String(repeating: "a", count: 1799)
        #expect(composer.remaining == nil)
        composer.draft += "a"
        #expect(composer.remaining == 200)
        composer.draft = String(repeating: "a", count: 2003)
        #expect(composer.remaining == -3)
    }

    @Test
    func lengthCountsCodePoints() {
        let composer = makeComposer()
        composer.draft = "👍🏽"
        #expect(composer.length == 2)
    }

    @Test
    func nitroUsersGetTheirLimit() async {
        store.update { $0.lengthLimit = 4000 }
        let composer = makeComposer()
        composer.draft = String(repeating: "a", count: 3000)

        await composer.submit()

        #expect(sends.count == 1)
        #expect(composer.lengthLimit == 4000)
    }

    @Test
    func slowmodeHoldsTheDraftUntilItEnds() async {
        let now = Date()
        slowmode(until: now + 10)
        let composer = makeComposer()
        composer.draft = "hi"

        await composer.submit(at: now)
        #expect(sends.isEmpty)
        #expect(composer.draft == "hi")

        await composer.submit(at: now + 11)
        #expect(sends == [.send(here, "hi")])
    }

    @Test
    func slowmodeIsReadAgainAroundEachSend() async {
        let composer = makeComposer()
        account.holdSends.withLock { $0 = true }
        composer.draft = "hi"

        async let submitted: Void = composer.submit()
        await account.sendReplies.pulled(1)
        let until = Date() + 10
        slowmode(until: until)
        composer.apply(EventBatch([.messageInserted(channelId: here, messageId: id(900))]))
        #expect(composer.slowmode?.until == until)
        store.log.forget()
        account.sendReplies.send(nil)
        await submitted

        #expect(store.reads.contains(.slowmode(here)))
    }

    @Test
    func aBatchForThisChannelRereadsSlowmodeAndOthersDont() {
        let composer = makeComposer()

        composer.apply(EventBatch([.messageInserted(channelId: id(11), messageId: id(5))]))
        #expect(!store.reads.contains(.slowmode(here)))

        composer.apply(EventBatch([.messageInserted(channelId: here, messageId: id(5))]))
        #expect(store.reads.contains(.slowmode(here)))
    }

    @Test
    func canSendFollowsPermissionsAndDms() {
        store.update { $0.permissions[id(10)] = [.viewChannel] }
        let composer = makeComposer()
        #expect(composer.canSend == false)

        store.update { $0.permissions[id(10)] = [.viewChannel, .sendMessages] }
        composer.apply(EventBatch([.currentMemberUpdated(guildId: id(1))]))
        #expect(composer.canSend == true)

        store.update { $0.permissions[id(10)] = [.viewChannel] }
        composer.apply(EventBatch([.channelUpdated(channelId: here, guildId: id(1))]))
        #expect(composer.canSend == false)

        store.update { $0.permissions[id(10)] = [.administrator, .sendMessages] }
        composer.apply(EventBatch([.guildUpdated(guildId: id(1))]))
        #expect(composer.canSend == true)

        store.update { $0.permissions[id(10)] = [] }
        composer.apply(EventBatch([.ready]))
        #expect(composer.canSend == false)

        store.update { $0.channels[id(20)] = channel(20, guild: nil, kind: .dm) }
        let dm = ComposerModel(channelId: id(20), account: account, store: store, drafts: Drafts())
        let unknown = ComposerModel(
            channelId: id(30), account: account, store: store, drafts: Drafts())
        #expect(dm.canSend == true)
        #expect(unknown.canSend == nil)
    }

    @Test
    func aChannelNotKnownYetIsNeitherAllowedNorDenied() async {
        let unknown = ComposerModel(
            channelId: id(30), account: account, store: store, drafts: Drafts())
        #expect(unknown.canSend == nil)

        unknown.draft = "hi"
        await unknown.submit()
        #expect(sends.isEmpty)
        #expect(unknown.draft == "hi")

        store.update { $0.channels[id(30)] = channel(30, guild: nil, kind: .dm) }
        unknown.apply(EventBatch([.ready]))
        #expect(unknown.canSend == true)
        await unknown.submit()
        #expect(sends == [.send(id(30), "hi")])
    }

    @Test
    func aCooldownEndingReadsSlowmodeAgain() async {
        slowmode(until: Date() + 0.3)
        let composer = makeComposer()
        #expect(composer.slowmode?.until != nil)

        slowmode(until: nil)
        for _ in 0..<40 where composer.slowmode?.until != nil {
            try? await Task.sleep(for: .milliseconds(50))
        }

        #expect(composer.slowmode?.until == nil)
        #expect(composer.slowmode?.interval == 10)
    }

    @Test
    func willSendRunsOnlyForASend() async {
        let now = Date()
        let composer = makeComposer()
        var calls = 0
        let willSend = { [store] in
            #expect(!store.log.actions.contains { if case .send = $0 { true } else { false } })
            calls += 1
        }

        composer.draft = "  "
        await composer.submit(at: now, willSend: willSend)
        composer.draft = String(repeating: "a", count: 2001)
        await composer.submit(at: now, willSend: willSend)
        slowmode(until: now + 10)
        composer.apply(EventBatch([.ready]))
        composer.draft = "hi"
        await composer.submit(at: now, willSend: willSend)
        #expect(calls == 0)

        await composer.submit(at: now + 11, willSend: willSend)
        #expect(calls == 1)
        #expect(sends == [.send(here, "hi")])
    }

    @Test
    func aFailedSendShowsItsProblemUntilASendWorks() async {
        let composer = makeComposer()
        account.sendError.withLock { $0 = .RateLimited(retryAfter: 2) }

        composer.draft = "hello"
        await composer.submit()
        #expect(composer.problem == .failed(.RateLimited(retryAfter: 2)))

        account.sendError.withLock { $0 = nil }
        composer.draft = "again"
        await composer.submit()
        #expect(composer.problem == nil)
    }

    @Test
    func retryAndDiscardActOnTheFailedMessage() async {
        let composer = makeComposer()
        account.sendError.withLock { $0 = .Network(kind: .connect) }

        await composer.retry(id(7))
        #expect(composer.problem == .failed(.Network(kind: .connect)))
        account.sendError.withLock { $0 = nil }
        await composer.retry(id(7))
        #expect(composer.problem == nil)
        composer.discard(id(7))

        #expect(
            store.log.actions.filter { call in
                if case .read = call { false } else { true }
            } == [.retry(here, id(7)), .retry(here, id(7)), .discard(here, id(7))]
        )
    }

    @Test
    func problemsSayWhatHappened() {
        let english = Locale(identifier: "en_US")
        let cases: [(ComposerModel.Problem, String)] = [
            (
                .tooLong(by: 23, limit: 2000),
                "This message is 23 characters too long. The limit is 2,000."
            ),
            (
                .tooLong(by: 1, limit: 2000),
                "This message is 1 character too long. The limit is 2,000."
            ),
            (
                .failed(.RateLimited(retryAfter: 5.2)),
                "You're sending messages too quickly. Try again in 6 seconds."
            ),
            (
                .failed(.RateLimited(retryAfter: 1)),
                "You're sending messages too quickly. Try again in 1 second."
            ),
            (
                .failed(.RateLimited(retryAfter: 90)),
                "You're sending messages too quickly. Try again in 2 minutes."
            ),
            (
                .failed(.RateLimited(retryAfter: nil)),
                "You're sending messages too quickly. Try again in a moment."
            ),
            (
                .failed(.Discord(status: 403, code: 50013, message: "Missing Permissions")),
                "You don't have permission to send that here."
            ),
            (
                .failed(
                    .Discord(status: 403, code: 50007, message: "Cannot send messages to this user")
                ),
                "Discord didn't deliver this message. This person may not accept messages from you."
            ),
            (
                .failed(.Discord(status: 400, code: 50035, message: "Invalid Form Body")),
                "Discord refused this message as too long or malformed."
            ),
            (
                .failed(.Discord(status: 400, code: 10003, message: "Unknown Channel")),
                "Discord refused this message: Unknown Channel"
            ),
            (.failed(.ServerError(status: 502)), "Discord had a problem. Try again in a moment."),
            (
                .failed(.Network(kind: .timeout)),
                "Akari couldn't reach Discord. Check your connection, then retry."
            ),
            (.failed(.TooLong(limit: 2000)), "This message is longer than 2,000 characters."),
            (.failed(.Closed), "Akari isn't connected. Reconnect, then retry."),
            (.failed(.InvalidRequest), RequestError.InvalidRequest.localizedDescription),
        ]
        for (problem, text) in cases {
            #expect(problem.text(locale: english) == text)
        }
    }
}
