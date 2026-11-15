import Foundation
import Observation

/// Why the editor's last save didn't go: one line under the editor (D162).
public enum EditProblem: Equatable, Sendable {
    /// Refused before asking: past the user's length limit.
    case tooLong(by: Int, limit: Int)
    /// Discord, or the core before asking, refused the save.
    case refused(EditError)

    /// Akari's words; AutoMod's and the edit limit's are Discord's.
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
        case .refused(.MessageBlocked(let reason)):
            return reason.isEmpty ? Self.blocked : reason
        case .refused(.TooManyEdits(let retryAfter)):
            let minutes = max(1, Int(((retryAfter ?? 60) / 60).rounded(.up)))
            return "To keep announcements speedy, published messages can only be edited 3 times "
                + "per hour. Please try again in \(number(minutes)) "
                + (minutes == 1 ? "minute." : "minutes.")
        case .refused(.Forbidden):
            return "You don't have permission to edit this message."
        case .refused(.Deleted):
            return "This message was deleted."
        case .refused(.RateLimited(let retryAfter)):
            return "You're editing too quickly. Try again in "
                + (retryAfter.map { Self.wait($0, number: number) } ?? "a moment") + "."
        case .refused(.Request(let error)):
            return Self.text(for: error, number: number)
        }
    }

    // The official client's words for an edit AutoMod blocked without a message of its own.
    static let blocked =
        "This message can't be edited because it contains content blocked by this community. "
        + "The message may also be viewed by the community owners."

    private static func text(for error: RequestError, number: (Int) -> String) -> String {
        switch error {
        case .RateLimited(let retryAfter):
            return "You're editing too quickly. Try again in "
                + (retryAfter.map { wait($0, number: number) } ?? "a moment") + "."
        case .Discord(_, 50013, _), .Discord(403, _, _):
            return "You don't have permission to edit this message."
        case .Discord(_, 50035, _):
            return "Discord refused this edit as too long or malformed."
        case .Discord(_, _, let message):
            return "Discord refused this edit: \(message)"
        case .ServerError:
            return "Discord had a problem. Try again in a moment."
        case .Network:
            return "Akari couldn't reach Discord. Check your connection, then save again."
        case .TooLong(let limit):
            return "This message is longer than \(number(Int(limit))) characters."
        case .Closed:
            return "Akari isn't connected. Reconnect to edit messages."
        case .InvalidRequest:
            return "This message can't be edited now."
        case .CaptchaRequired, .ReplyTargetDeleted, .Unauthorized, .UnexpectedResponse:
            return "Discord had a problem. Try again in a moment."
        }
    }

    private static func wait(_ seconds: TimeInterval, number: (Int) -> String) -> String {
        let whole = Int(seconds.rounded(.up))
        if whole < 60 {
            return whole == 1 ? "1 second" : "\(number(whole)) seconds"
        }
        let minutes = Int((seconds / 60).rounded(.up))
        return minutes == 1 ? "1 minute" : "\(number(minutes)) minutes"
    }
}

/// The user's edit of one of their messages in a channel, in the row's place: one per channel,
/// kept for the session; a save closes it at once, and a refusal opens it again with why (D162).
@MainActor @Observable
public final class EditModel {
    public struct Editing: Equatable, Sendable {
        /// The row's own message (a thread's starter row edits the message it shows).
        public let messageId: MessageId
        public let problem: EditProblem?
        /// Counts the openings: each one puts `text` into the editor and gives it the focus.
        public let opening: Int
        /// Counts Edit on the message already being edited: the editor takes the focus, text and
        /// caret as they are.
        public let focus: Int

        public init(messageId: MessageId, problem: EditProblem? = nil, opening: Int, focus: Int = 0) {
            self.messageId = messageId
            self.problem = problem
            self.opening = opening
            self.focus = focus
        }
    }

    public let channelId: ChannelId
    public private(set) var editing: Editing?
    /// What the editor holds, written by it as the user types; read when an edit opens.
    @ObservationIgnored public var text: String {
        didSet {
            drafts.edits[channelId]?.text = text
            if case .tooLong = editing?.problem, Self.length(of: text) <= lengthLimit {
                show(nil)
            }
        }
    }
    /// The user's limit: 4,000 with Nitro, else 2,000.
    @ObservationIgnored public private(set) var lengthLimit: Int
    /// Characters left once a tenth of the limit or fewer remain (negative past it), else `nil`,
    /// as the official editor's counter.
    public var remaining: Int? {
        let left = lengthLimit - Self.length(of: text)
        return left <= lengthLimit / 10 ? left : nil
    }

    /// Whether the host's editor has the keyboard focus: an edit the user didn't end hands it
    /// to the composer only then.
    @ObservationIgnored public var editorFocused = false

    @ObservationIgnored private let account: Account
    @ObservationIgnored private let store: Store
    @ObservationIgnored private let drafts: Drafts
    @ObservationIgnored var row: (MessageId) -> MessageListModel.Row? = { _ in nil }
    @ObservationIgnored var rows: () -> [MessageListModel.Row] = { [] }
    // Where the list's loaded rows would show the message, but don't.
    @ObservationIgnored var lost: (MessageId) -> Bool = { _ in false }
    @ObservationIgnored var askDelete: (MessageListModel.Row) -> Void = { _ in }
    @ObservationIgnored var ended: () -> Void = {}
    @ObservationIgnored private(set) var saving: Task<Void, Never>?

    init(channelId: ChannelId, account: Account, store: Store, drafts: Drafts) {
        self.channelId = channelId
        self.account = account
        self.store = store
        self.drafts = drafts
        lengthLimit = Int(store.messageLengthLimit())
        let kept = drafts.edits[channelId]
        text = kept?.text ?? ""
        if let kept {
            drafts.editOpenings += 1
            editing = Editing(
                messageId: kept.messageId, problem: kept.problem, opening: drafts.editOpenings)
        }
    }

    /// Edit and Up arrow: the row's text, or a refused save's; replaces the channel's open edit,
    /// and on the message already being edited only asks the editor's focus.
    public func start(_ row: MessageListModel.Row) {
        let own = row.message
        guard own.channelId == channelId,
            store.messageActions(channelId: own.channelId, messageId: own.id).edit
        else {
            return
        }
        if let editing, editing.messageId == own.id {
            self.editing = Editing(
                messageId: own.id, problem: editing.problem, opening: editing.opening,
                focus: editing.focus + 1)
            return
        }
        if let refused = refusal(of: row) {
            reopen(row, text: refused.text, error: refused.error)
        } else {
            open(own.id, text: row.shown.content, problem: nil)
        }
    }

    /// Escape and "cancel": the message stays as it is.
    public func cancel() {
        guard editing != nil else {
            return
        }
        end()
    }

    /// Return and "save": unchanged text sends nothing, emptied text asks to delete the message,
    /// and text past the limit stays with the reason.
    public func save() {
        guard let editing else {
            return
        }
        guard let row = row(editing.messageId) else {
            end()
            return
        }
        let content = text.trimmingCharacters(in: .whitespacesAndNewlines)
        let shown = row.shown
        if content == shown.content {
            end()
            return
        }
        if content.isEmpty, shown.attachments.isEmpty, shown.stickers.isEmpty {
            end()
            askDelete(row)
            return
        }
        let length = Self.length(of: content)
        guard length <= lengthLimit else {
            show(.tooLong(by: length - lengthLimit, limit: lengthLimit))
            return
        }
        end()
        let own = row.message
        drafts.saves += 1
        let token = drafts.saves
        drafts.unansweredSaves[own.id] = token
        drafts.takenRefusals.remove(own.id)
        drafts.thrownRefusals[own.id] = nil
        saving = Task { [weak self, account, drafts] in
            do {
                try await account.editMessage(
                    channelId: own.channelId, messageId: own.id, content: content)
                if drafts.unansweredSaves[own.id] == token {
                    drafts.unansweredSaves[own.id] = nil
                }
            } catch {
                // The message's refusal may have opened the editor first.
                guard drafts.unansweredSaves[own.id] == token else {
                    return
                }
                drafts.unansweredSaves[own.id] = nil
                let refused = error as? EditError ?? .Request(error: .UnexpectedResponse)
                if !Self.deleted(refused) {
                    drafts.thrownRefusals[own.id] = (content, refused)
                    self?.reopenRefused()
                }
            }
        }
    }

    func apply(_ batch: EventBatch) {
        if batch.everythingChanged || batch.currentUserChanged {
            lengthLimit = Int(store.messageLengthLimit())
        }
        let updated = batch.updatedMessages[channelId] ?? []
        if let editing, gone(editing.messageId, in: batch, updated: updated) {
            end(byUser: false)
            return
        }
        if editing == nil, batch.everythingChanged || !updated.isEmpty || batch.windowsChanged.contains(channelId) {
            reopenRefused()
        }
    }

    // A delete while the channel was closed, or before the list loaded, said nothing to it.
    func opened() {
        if let editing, lost(editing.messageId) {
            end(byUser: false)
        } else {
            reopenRefused()
        }
    }

    func reopenRefused() {
        guard editing == nil else {
            return
        }
        for row in rows().reversed() {
            if let refused = refusal(of: row) {
                reopen(row, text: refused.text, error: refused.error)
                return
            }
        }
    }

    private func gone(_ message: MessageId, in batch: EventBatch, updated: Set<MessageId>) -> Bool {
        if batch.deletedMessages[channelId]?.contains(message) == true {
            return true
        }
        let reloaded = batch.everythingChanged || batch.windowsChanged.contains(channelId)
        if reloaded, lost(message) {
            return true
        }
        // A thread's starter row stays when the message it shows goes.
        return (reloaded || updated.contains(message)) && row(message)?.starter == .unavailable
    }

    // A refused save that is still to open: the core's, or one it kept no record of.
    private func refusal(of row: MessageListModel.Row) -> (text: String, error: EditError)? {
        let id = row.message.id
        if case .failed(let content, let error) = row.shown.edit, !drafts.takenRefusals.contains(id),
            !Self.deleted(error)
        {
            return (content, error)
        }
        return drafts.thrownRefusals[id]
    }

    // Nothing is left to save to; the row's own deletion says the rest.
    private static func deleted(_ error: EditError) -> Bool {
        if case .Deleted = error { true } else { false }
    }

    private func reopen(_ row: MessageListModel.Row, text content: String, error: EditError) {
        drafts.unansweredSaves[row.message.id] = nil
        drafts.thrownRefusals[row.message.id] = nil
        drafts.takenRefusals.insert(row.message.id)
        open(row.message.id, text: content, problem: .refused(error))
        account.dismissEdit(channelId: row.shown.channelId, messageId: row.shown.id)
    }

    private func open(_ message: MessageId, text content: String, problem: EditProblem?) {
        drafts.editOpenings += 1
        drafts.edits[channelId] = EditDraft(messageId: message, text: content, problem: problem)
        text = content
        editing = Editing(messageId: message, problem: problem, opening: drafts.editOpenings)
    }

    private func show(_ problem: EditProblem?) {
        guard let editing else {
            return
        }
        drafts.edits[channelId]?.problem = problem
        self.editing = Editing(
            messageId: editing.messageId, problem: problem, opening: editing.opening, focus: editing.focus)
    }

    private func end(byUser: Bool = true) {
        editing = nil
        drafts.edits[channelId] = nil
        text = ""
        if byUser || editorFocused {
            ended()
        }
        editorFocused = false
        reopenRefused()
    }

    static func length(of text: String) -> Int {
        text.isEmpty ? 0 : Int(messageLength(content: text))
    }
}
