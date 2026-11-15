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
/// kept for the session (a channel switch keeps it with its text), ended by Escape, a save,
/// another edit in the channel and the message's deletion (bulk deletes too). A save closes the
/// editor at once; a refusal opens it again with the saved text and why (D162).
@MainActor @Observable
public final class EditModel {
    public struct Editing: Equatable, Sendable {
        /// The row's own message (a thread's starter row edits the message it shows).
        public let messageId: MessageId
        public let problem: EditProblem?
        /// Counts the openings: each one puts `text` into the editor and gives it the focus.
        public let opening: Int

        public init(messageId: MessageId, problem: EditProblem? = nil, opening: Int) {
            self.messageId = messageId
            self.problem = problem
            self.opening = opening
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

    @ObservationIgnored private let account: Account
    @ObservationIgnored private let store: Store
    @ObservationIgnored private let drafts: Drafts
    // The list's row for a message, as it shows now.
    @ObservationIgnored var row: (MessageId) -> MessageListModel.Row? = { _ in nil }
    // The list's rows, newest last.
    @ObservationIgnored var rows: () -> [MessageListModel.Row] = { [] }
    // An emptied message without media asks to be deleted instead.
    @ObservationIgnored var askDelete: (MessageListModel.Row) -> Void = { _ in }
    // The composer takes the focus back.
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

    /// Edit in a message's bar or menu, and Up arrow: the editor takes the row's text, or the
    /// text of a save Discord refused; nothing where the core doesn't allow the edit. Replaces
    /// the channel's open edit and its text, as in the official client.
    public func start(_ row: MessageListModel.Row) {
        let own = row.message
        guard own.channelId == channelId,
            store.messageActions(channelId: own.channelId, messageId: own.id).edit
        else {
            return
        }
        if case .failed(let content, let error) = row.shown.edit, !drafts.takenRefusals.contains(own.id) {
            reopen(row, text: content, error: error)
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

    /// Return and "save". Text the message shows already sends nothing; emptied text on a
    /// message without attachments or stickers asks to delete it; text past the limit stays in
    /// the editor with the reason. Otherwise the editor closes and the core saves (the row shows
    /// the text at once, dimmed until Discord takes it).
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
        drafts.unansweredSaves.insert(own.id)
        drafts.takenRefusals.remove(own.id)
        saving = Task { [weak self, account, drafts] in
            do {
                try await account.editMessage(
                    channelId: own.channelId, messageId: own.id, content: content)
                drafts.unansweredSaves.remove(own.id)
            } catch {
                // The message's refusal may have opened the editor first.
                guard drafts.unansweredSaves.remove(own.id) != nil, let self, self.editing == nil
                else {
                    return
                }
                let refusal = error as? EditError ?? .Request(error: .UnexpectedResponse)
                open(own.id, text: content, problem: .refused(refusal))
                account.dismissEdit(channelId: shown.channelId, messageId: shown.id)
            }
        }
    }

    func apply(_ batch: EventBatch) {
        if batch.everythingChanged || batch.currentUserChanged {
            lengthLimit = Int(store.messageLengthLimit())
        }
        if let editing, batch.deletedMessages[channelId]?.contains(editing.messageId) == true {
            end()
            return
        }
        let updated = batch.updatedMessages[channelId] ?? []
        if editing == nil, batch.everythingChanged || !updated.isEmpty || batch.windowsChanged.contains(channelId) {
            reopenRefused()
        }
    }

    /// Opens the editor again for the newest row whose save Discord refused, unless an edit is
    /// open; the core forgets the refusal once the editor holds its text.
    func reopenRefused() {
        guard editing == nil else {
            return
        }
        for row in rows().reversed() where !drafts.takenRefusals.contains(row.message.id) {
            if case .failed(let content, let error) = row.shown.edit {
                reopen(row, text: content, error: error)
                return
            }
        }
    }

    private func reopen(_ row: MessageListModel.Row, text content: String, error: EditError) {
        drafts.unansweredSaves.remove(row.message.id)
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
        self.editing = Editing(messageId: editing.messageId, problem: problem, opening: editing.opening)
    }

    private func end() {
        editing = nil
        drafts.edits[channelId] = nil
        text = ""
        ended()
        reopenRefused()
    }

    static func length(of text: String) -> Int {
        text.isEmpty ? 0 : Int(messageLength(content: text))
    }
}
