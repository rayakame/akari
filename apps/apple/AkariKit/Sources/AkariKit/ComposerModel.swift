import Foundation
import Observation

/// The message the next send answers, as the bar over the composer shows it.
public struct PendingReply: Equatable, Sendable {
    public let messageId: MessageId
    /// The name the message's header shows for its author.
    public let authorName: String
    /// The reply pings the message's author.
    public var mention: Bool
    /// The bar has the switch for the ping: not in DMs, not for the user's own message.
    public let showsToggle: Bool

    public init(messageId: MessageId, authorName: String, mention: Bool, showsToggle: Bool) {
        self.messageId = messageId
        self.authorName = authorName
        self.mention = mention
        self.showsToggle = showsToggle
    }
}

/// What the user writes in a channel and what happens when they send it.
@MainActor @Observable
public final class ComposerModel {
    public enum Problem: Equatable, Sendable {
        case tooLong(by: Int, limit: Int)
        case failed(RequestError)
        /// The core no longer has the message a reply was for: nothing was sent.
        case replyTargetGone

        /// Akari's wording for the line under the composer.
        public var text: String {
            text(locale: .autoupdatingCurrent)
        }

        func text(locale: Locale) -> String {
            let number = { (value: Int) in value.formatted(.number.locale(locale)) }
            switch self {
            case .tooLong(let by, let limit):
                let characters = by == 1 ? "character" : "characters"
                return "This message is \(number(by)) \(characters) too long. "
                    + "The limit is \(number(limit))."
            case .failed(let error):
                return Self.text(for: error, number: number)
            case .replyTargetGone:
                return "The message you were replying to isn't available any more. "
                    + "Your message wasn't sent."
            }
        }

        private static func text(for error: RequestError, number: (Int) -> String) -> String {
            switch error {
            case .RateLimited(let retryAfter):
                return "You're sending messages too quickly. Try again in "
                    + (retryAfter.map(wait) ?? "a moment") + "."
            case .Discord(_, 50013, _):
                return "You don't have permission to send that here."
            case .Discord(_, 50007, _):
                return "Discord didn't deliver this message. This person may not accept "
                    + "messages from you."
            case .Discord(_, 50035, _):
                return "Discord refused this message as too long or malformed."
            case .Discord(_, _, let message):
                return "Discord refused this message: \(message)"
            case .ServerError:
                return "Discord had a problem. Try again in a moment."
            case .Network:
                return "Akari couldn't reach Discord. Check your connection, then retry."
            case .CaptchaRequired:
                return "Discord wants a captcha for this message, which Akari can't show yet."
            case .TooLong(let limit):
                return "This message is longer than \(number(Int(limit))) characters."
            case .Closed:
                return "Akari isn't connected. Reconnect to send messages."
            case .ReplyTargetDeleted:
                return "The message you replied to was deleted."
            case .Unauthorized, .InvalidRequest, .UnexpectedResponse:
                return error.localizedDescription
            }
        }

        private static func wait(_ seconds: TimeInterval) -> String {
            let whole = Int(seconds.rounded(.up))
            if whole < 60 {
                return whole == 1 ? "1 second" : "\(whole) seconds"
            }
            let minutes = Int((seconds / 60).rounded(.up))
            return minutes == 1 ? "1 minute" : "\(minutes) minutes"
        }
    }

    public let channelId: ChannelId
    /// Kept per channel while the session lasts.
    public var draft: String {
        didSet {
            drafts[channelId] = draft
            length = Self.length(of: draft)
            if case .tooLong = problem {
                problem =
                    length > lengthLimit
                    ? .tooLong(by: length - lengthLimit, limit: lengthLimit) : nil
            }
        }
    }
    /// Code points, as Discord counts them.
    public private(set) var length: Int
    public private(set) var lengthLimit: Int
    /// Characters left once 200 or fewer remain (negative past the limit), else `nil`.
    public var remaining: Int? {
        let left = lengthLimit - length
        return left <= Self.counterFrom ? left : nil
    }
    /// Whether the user's permissions allow sending here; DMs always do. `nil` until the store
    /// knows the channel, e.g. before READY.
    public private(set) var canSend: Bool?
    /// A locked thread: only moderators can send there.
    public private(set) var locked = false
    public private(set) var slowmode: Slowmode? {
        didSet { awaitCooldownEnd() }
    }
    public private(set) var problem: Problem?
    /// One per channel, kept while the session lasts: until its send starts, ✕, Escape, or the
    /// message is deleted.
    public private(set) var reply: PendingReply? {
        didSet { drafts.replies[channelId] = reply }
    }
    /// Counts the times the composer should take the focus: a reply was started.
    public private(set) var focusRequests = 0

    private static let counterFrom = 200

    @ObservationIgnored private let account: Account
    @ObservationIgnored private let store: Store
    @ObservationIgnored private let drafts: Drafts
    @ObservationIgnored private var guildId: GuildId?
    @ObservationIgnored private var cooldownEnd: Task<Void, Never>?
    @ObservationIgnored var onSend: (() -> Void)?

    init(channelId: ChannelId, account: Account, store: Store, drafts: Drafts) {
        self.channelId = channelId
        self.account = account
        self.store = store
        self.drafts = drafts
        draft = drafts[channelId]
        length = Self.length(of: drafts[channelId])
        reply = drafts.replies[channelId]
        lengthLimit = Int(store.messageLengthLimit())
        readCanSend()
        slowmode = store.slowmode(channelId: channelId)
        awaitCooldownEnd()
    }

    /// Sends the trimmed draft and clears it, unless it is blank, too long, or held back by
    /// slowmode.
    /// `willSend` runs right before a message is sent, not when nothing is.
    public func submit(at now: Date = Date(), willSend: () -> Void = {}) async {
        let content = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        // Before the store knows the channel the core would refuse it after the draft is gone.
        guard !content.isEmpty, canSend == true else {
            return
        }
        let length = Self.length(of: content)
        guard length <= lengthLimit else {
            problem = .tooLong(by: length - lengthLimit, limit: lengthLimit)
            return
        }
        if let until = slowmode?.until, until > now {
            return
        }
        // The draft goes only once the core holds the message: a refusal (closed, too long)
        // leaves it exactly as typed.
        let pending: MessageId
        do {
            pending = try account.queueMessage(
                channelId: channelId, content: content,
                reply: reply.map { ReplyTo(messageId: $0.messageId, mention: $0.mention) })
        } catch {
            let failure = error as? RequestError ?? .UnexpectedResponse
            // The core refuses a reply whose message it no longer holds.
            if reply != nil, failure == .InvalidRequest {
                reply = nil
                problem = .replyTargetGone
            } else {
                problem = .failed(failure)
            }
            return
        }
        draft = ""
        reply = nil
        onSend?()
        willSend()
        do {
            _ = try await account.deliverMessage(channelId: channelId, pendingId: pending)
            problem = nil
        } catch {
            problem = .failed(error as? RequestError ?? .UnexpectedResponse)
        }
        slowmode = store.slowmode(channelId: channelId)
    }

    /// Reply in a message's bar or menu: the next send answers `row`'s message. Shift at the click
    /// starts it without the ping. Replaces a pending reply; nothing where the core doesn't allow
    /// the reply.
    public func startReply(to row: MessageListModel.Row, shift: Bool) {
        let message = row.message
        let actions = store.messageActions(channelId: message.channelId, messageId: message.id)
        guard message.channelId == channelId, actions.reply else {
            return
        }
        reply = PendingReply(
            messageId: message.id, authorName: row.authorName,
            mention: actions.replyPings && !shift, showsToggle: actions.mentionToggle)
        if problem == .replyTargetGone {
            problem = nil
        }
        focusRequests += 1
    }

    /// The bar's "@" switch; nothing where the bar doesn't show it.
    public func toggleReplyMention() {
        guard reply?.showsToggle == true else {
            return
        }
        reply?.mention.toggle()
    }

    /// The bar's ✕ and Escape.
    public func cancelReply() {
        reply = nil
    }

    /// Sends a failed message again.
    public func retry(_ id: MessageId) async {
        do {
            _ = try await account.retryMessage(channelId: channelId, pendingId: id)
            problem = nil
        } catch {
            problem = .failed(error as? RequestError ?? .UnexpectedResponse)
        }
        slowmode = store.slowmode(channelId: channelId)
    }

    /// Drops a failed message.
    public func discard(_ id: MessageId) {
        account.discardMessage(channelId: channelId, pendingId: id)
    }

    func apply(_ batch: EventBatch) {
        endReplyToDeleted(batch)
        let guildChanged =
            guildId.map { batch.guildsChanged.contains($0) || batch.membersChanged.contains($0) }
            ?? false
        let channelChanged = batch.channelsChanged[guildId]?.contains(channelId) == true
        if batch.everythingChanged || guildChanged || channelChanged {
            readCanSend()
        }
        // Only the user's own sends and messages start a cooldown, and they change the window.
        if batch.everythingChanged || guildChanged || channelChanged
            || batch.windowsChanged.contains(channelId) || batch.confirmed[channelId] != nil
        {
            slowmode = store.slowmode(channelId: channelId)
        }
        if batch.everythingChanged || batch.currentUserChanged {
            lengthLimit = Int(store.messageLengthLimit())
        }
    }

    // Bulk deletes too, which the official client misses.
    private func endReplyToDeleted(_ batch: EventBatch) {
        guard let target = reply?.messageId else {
            return
        }
        let deleted =
            batch.deletedMessages[channelId]?.contains(target) == true
            || batch.replyTargetsChanged[channelId]?.contains(target) == true
                && store.replyTarget(channelId: channelId, targetId: target) == .deleted
        if deleted {
            reply = nil
        }
    }

    // Nothing else reads slowmode when a cooldown runs out in a quiet channel.
    private func awaitCooldownEnd() {
        cooldownEnd?.cancel()
        guard let until = slowmode?.until else {
            return
        }
        let wait = max(0, until.timeIntervalSinceNow) + 0.05
        cooldownEnd = Task { [weak self] in
            try? await Task.sleep(for: .seconds(wait))
            guard !Task.isCancelled, let self else {
                return
            }
            slowmode = store.slowmode(channelId: channelId)
        }
    }

    private func readCanSend() {
        guard let channel = store.channel(id: channelId) else {
            canSend = nil
            locked = false
            return
        }
        guildId = channel.guildId
        locked = channel.thread?.locked == true
        // DMs have no permissions to check.
        canSend =
            channel.guildId == nil
            || store.permissions(channelId: channelId)?.contains(.sendMessages) == true
    }

    private static func length(of text: String) -> Int {
        text.isEmpty ? 0 : Int(messageLength(content: text))
    }
}
