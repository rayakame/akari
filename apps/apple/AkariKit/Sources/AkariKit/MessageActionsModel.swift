import Foundation

/// Everything a message's hover bar and menu can offer, the features Akari doesn't have yet
/// included: their places in the orders are fixed here.
public enum MessageAction: String, CaseIterable, Sendable {
    case addReaction, viewReactions, reply, edit, createThread, viewThread
    case copyText, pin, unpin, retry, markUnread, copyLink
    case removeReactions, removeAllReactions, delete, copyId, more
}

/// One button of the hover bar or one item of the menu.
public struct MessageActionItem: Equatable, Sendable, Identifiable {
    public let action: MessageAction
    /// The official client's words: the button's tooltip and label, the item's title.
    public let title: String
    /// An SF Symbol.
    public let symbol: String
    public let enabled: Bool
    /// Red, as the official client's dangerous items.
    public let destructive: Bool

    public var id: MessageAction { action }

    public init(action: MessageAction, title: String, symbol: String, enabled: Bool, destructive: Bool) {
        self.action = action
        self.title = title
        self.symbol = symbol
        self.enabled = enabled
        self.destructive = destructive
    }
}

/// Words under a question's message: `bold` is drawn bold, `label` ("Protip:") stands before
/// them in the official client's green.
public struct MessageTip: Equatable, Sendable {
    public let label: String?
    public let text: String
    public let bold: String

    public init(label: String?, text: String, bold: String) {
        self.label = label
        self.text = text
        self.bold = bold
    }
}

/// What a message's hover bar and menu list, in the official client's order, and what each does.
/// Akari's core decides what is allowed (`Store.messageActions`, asked with the row's own IDs
/// every time: nothing announces a change) and builds a message's link (`Store.messageLink`); an
/// action takes the IDs of the message the row shows.
@MainActor
public final class MessageActionsModel {
    /// Why an action failed, for one alert.
    public struct Failure: Equatable, Sendable {
        public let title: String
        public let message: String

        public init(title: String, message: String) {
            self.title = title
            self.message = message
        }
    }

    /// The question before a message is deleted, with the message as its row shows it (the
    /// official client's modal); `run` deletes it.
    public struct DeleteQuestion: Identifiable {
        public static let title = "Delete Message"
        public static let message = "Are you sure you want to delete this message?"
        /// The tip, with the words that are bold in it.
        public static let tip =
            "You can hold down shift when clicking delete message to bypass this confirmation "
            + "entirely."
        public static let tipBold = "delete message"
        public static let confirm = "Delete"
        public static let cancel = "Cancel"

        public let id = UUID()
        public let row: MessageListModel.Row
        /// From the menu: the tip that Shift skips the question.
        public let showsTip: Bool
        public let run: @MainActor () -> Void

        public init(row: MessageListModel.Row, showsTip: Bool, run: @escaping @MainActor () -> Void) {
            self.row = row
            self.showsTip = showsTip
            self.run = run
        }
    }

    /// Pin's and Unpin's question, with the message as its row shows it (the official client's
    /// modals); `run` pins or unpins it.
    public struct PinQuestion: Identifiable {
        public let id = UUID()
        public let row: MessageListModel.Row
        /// Pins the message; unpins it otherwise.
        public let pins: Bool
        public let title: String
        public let message: String
        /// Unpin's: Shift skips the question.
        public let tip: MessageTip?
        public let confirm: String
        public let cancel: String
        public let run: @MainActor () -> Void

        /// Unpin's confirm is red.
        public var destructive: Bool { !pins }

        public init(
            row: MessageListModel.Row, pins: Bool, channelName: String?,
            run: @escaping @MainActor () -> Void
        ) {
            self.row = row
            self.pins = pins
            self.run = run
            cancel = "Cancel"
            if pins {
                title = "Pin It. Pin It Good."
                let place = channelName.map { "#\($0)" } ?? "the current channel"
                message =
                    "Hey, just double checking that you want to pin this message to \(place) for "
                    + "posterity and greatness?"
                tip = nil
                confirm = "Oh yeah. Pin it"
            } else {
                title = "Unpin Message"
                message = "You sure you want to remove this pinned message?"
                tip = MessageTip(
                    label: "Protip:",
                    text: "You can hold down shift when clicking unpin message to bypass this "
                        + "confirmation entirely.",
                    bold: "unpin message")
                confirm = "Remove it please!"
            }
        }
    }

    /// A question before something that can't be undone; `run` does it.
    public struct Confirmation: Identifiable {
        public let id = UUID()
        public let title: String
        public let message: String
        public let confirm: String
        public let cancel: String
        /// The confirm button is red.
        public var destructive = true
        public let run: @MainActor () -> Void
    }

    enum When {
        case always
        /// The user's own message that can be edited: Reply moves into the Shift row.
        case editable
        case notEditable
        /// More gives way to Delete in the Shift row.
        case withoutDelete

        func fits(editable: Bool, deletable: Bool) -> Bool {
            switch self {
            case .always: true
            case .editable: editable
            case .notEditable: !editable
            case .withoutDelete: !deletable
            }
        }
    }

    struct Slot {
        let action: MessageAction
        var when = When.always
    }

    static let barOrder: [Slot] = [
        Slot(action: .addReaction), Slot(action: .reply, when: .notEditable), Slot(action: .edit),
        Slot(action: .createThread), Slot(action: .viewThread), Slot(action: .more),
    ]

    static let shiftBarOrder: [Slot] =
        [
            Slot(action: .copyId), Slot(action: .copyLink), Slot(action: .markUnread),
            Slot(action: .pin), Slot(action: .unpin), Slot(action: .reply, when: .editable),
        ] + barOrder.dropLast() + [Slot(action: .delete), Slot(action: .more, when: .withoutDelete)]

    static let failedBarOrder: [MessageAction] = [.retry, .delete]

    static let menuOrder: [[MessageAction]] = [
        [.addReaction, .viewReactions],
        [.edit, .reply, .createThread, .viewThread],
        [.copyText, .pin, .unpin, .retry, .markUnread, .copyLink],
        [.removeReactions, .removeAllReactions, .delete],
        [.copyId],
    ]

    /// Where an action was clicked.
    public enum Surface: Sendable {
        case bar, menu
    }

    private let store: Store
    private let account: Account
    private let copy: (String) -> Void
    private let retry: (MessageId) -> Void
    private let discard: (MessageId) -> Void
    private let createThread: (ChannelId, MessageId) -> Void
    private let viewThread: (ChannelId, ChannelId) -> Void
    private let marking: (ChannelId, MessageId) -> ((_ taken: Bool) -> Void)?
    private let reply: (MessageListModel.Row, Bool) -> Void
    private let reactions: ReactionsModel?
    private let viewReactions: (MessageListModel.Row) -> Void
    private let confirm: (Confirmation) -> Void
    private let edit: (MessageListModel.Row) -> Void
    private let askDelete: (DeleteQuestion) -> Void
    private let askPin: (PinQuestion) -> Void
    private let failed: (Failure) -> Void

    /// `copy` puts text on the host's pasteboard; `createThread` takes the message's channel and
    /// the message, `viewThread` the thread and its parent; `marking` shows a mark as unread at
    /// once and returns what takes Discord's answer: whether it took the mark; `reply` starts a
    /// reply to the row's message, with Shift as it was at the click; `reactions` is the list's
    /// (without it nothing about reactions is offered), `viewReactions` opens the reactions
    /// window and `confirm` asks before reactions are removed; `edit` opens the row's editor,
    /// `askDelete` asks before a message is deleted and `askPin` before one is pinned or unpinned.
    public init(
        store: Store, account: Account, copy: @escaping (String) -> Void,
        retry: @escaping (MessageId) -> Void, discard: @escaping (MessageId) -> Void,
        createThread: @escaping (ChannelId, MessageId) -> Void,
        viewThread: @escaping (ChannelId, ChannelId) -> Void,
        marking: @escaping (ChannelId, MessageId) -> ((_ taken: Bool) -> Void)? = { _, _ in nil },
        reply: @escaping (MessageListModel.Row, Bool) -> Void = { _, _ in },
        reactions: ReactionsModel? = nil,
        viewReactions: @escaping (MessageListModel.Row) -> Void = { _ in },
        confirm: @escaping (Confirmation) -> Void = { _ in },
        edit: @escaping (MessageListModel.Row) -> Void = { _ in },
        askDelete: @escaping (DeleteQuestion) -> Void = { _ in },
        askPin: @escaping (PinQuestion) -> Void = { _ in },
        failed: @escaping (Failure) -> Void
    ) {
        self.edit = edit
        self.askDelete = askDelete
        self.askPin = askPin
        self.reactions = reactions
        self.viewReactions = viewReactions
        self.confirm = confirm
        self.store = store
        self.account = account
        self.copy = copy
        self.retry = retry
        self.discard = discard
        self.createThread = createThread
        self.viewThread = viewThread
        self.marking = marking
        self.reply = reply
        self.failed = failed
    }

    /// The hover bar's buttons, left to right; with Shift held, the official client's longer row.
    /// Empty where the row has no bar: a sending or ephemeral message, one that allows nothing.
    public func bar(for row: MessageListModel.Row, shift: Bool) -> [MessageActionItem] {
        let offer = offer(for: row)
        if offer.failed {
            return Self.failedBarOrder.filter(offer.has).map { Self.item($0, in: .bar, failed: true) }
        }
        guard offer.allowsAnything else {
            return []
        }
        let slots = shift ? Self.shiftBarOrder : Self.barOrder
        let hasDelete = slots.contains { $0.action == .delete } && offer.has(.delete)
        return slots.compactMap { slot in
            let fits = slot.when.fits(editable: offer.actions.edit, deletable: hasDelete)
            return fits && offer.has(slot.action) ? Self.item(slot.action, in: .bar) : nil
        }
    }

    /// The emoji before the bar's buttons, each a reaction in one click; none with Shift held,
    /// on a failed send, or where Add Reaction isn't offered.
    public func quickReactions(for row: MessageListModel.Row, shift: Bool = false) -> [QuickReaction] {
        shift ? [] : reactions?.quick(for: row) ?? []
    }

    /// Add Reaction's submenu in the menu: up to twelve emoji, each a reaction in one click.
    public func menuReactions(for row: MessageListModel.Row) -> [QuickReaction] {
        reactions?.menu(for: row) ?? []
    }

    /// What "Remove Reactions" lists: the message's emoji, each removable for everyone.
    public func removableReactions(for row: MessageListModel.Row) -> [ReactionPill] {
        let own = row.message
        guard offer(for: row).has(.removeReactions) else {
            return []
        }
        return ReactionPill.merged(store.reactions(channelId: own.channelId, messageId: own.id))
    }

    /// A quick reaction's click: adds the user's reaction, or takes it back.
    public func react(
        _ quick: QuickReaction, on row: MessageListModel.Row, from location: ReactionLocation
    ) async {
        await reactions?.toggle(quick.emoji, on: row, from: location)
    }

    /// Removes everyone's reactions with `pill`'s emoji, or all of the message's without one,
    /// after a question; Shift at the click skips it, as in the official client.
    public func removeReactions(of pill: ReactionPill?, on row: MessageListModel.Row, shift: Bool) {
        let own = row.message
        // The model may be gone when the question is answered: a host makes one per update.
        let run = { @MainActor [account, failed] in
            _ = Task {
                await Self.removeReactions(
                    of: pill?.emoji, in: own.channelId, from: own.id, account: account, failed: failed)
            }
        }
        if shift {
            run()
            return
        }
        confirm(
            Confirmation(
                title: pill == nil ? ReactionWords.removeAllTitle : ReactionWords.removeEmojiTitle,
                message: pill == nil ? ReactionWords.removeAllText : ReactionWords.removeEmojiText,
                confirm: ReactionWords.yes, cancel: ReactionWords.no, run: run))
    }

    static func removeReactions(
        of emoji: ReactionEmoji?, in channel: ChannelId, from message: MessageId, account: Account,
        failed: (Failure) -> Void
    ) async {
        do {
            try await account.removeReactions(channelId: channel, messageId: message, emoji: emoji)
        } catch {
            failed(
                Failure(
                    title: "Couldn't Remove Reactions",
                    message: Self.text(for: error as? ReactionError ?? .Request(error: .UnexpectedResponse))))
        }
    }

    /// The menu's items in the official client's groups; a separator goes between two groups.
    public func menu(for row: MessageListModel.Row) -> [[MessageActionItem]] {
        let offer = offer(for: row)
        return Self.menuOrder.compactMap { group in
            let items = group.filter { $0 != .more && offer.has($0) }
                .map { Self.item($0, in: .menu, failed: offer.failed) }
            return items.isEmpty ? nil : items
        }
    }

    /// Does what a button (`surface` `.bar`) or an item says. `shift` is the key at the click, for
    /// the actions that read it. More is the host's: it opens the menu; so are Add Reaction and
    /// Remove Reactions, which list emoji to choose from.
    public func perform(
        _ action: MessageAction, on row: MessageListModel.Row, shift: Bool = false,
        from surface: Surface = .menu
    ) {
        let message = row.shown
        switch action {
        case .copyText:
            copy(message.content)
        case .copyLink:
            copy(store.messageLink(channelId: row.message.channelId, messageId: row.message.id))
        case .markUnread:
            Task { await markUnread(from: row) }
        case .createThread:
            createThread(message.channelId, message.id)
        case .viewThread:
            viewThread(ChannelId(rawValue: message.id.rawValue), message.channelId)
        case .retry:
            retry(row.message.id)
        case .delete:
            delete(row, shift: shift, from: surface)
        case .edit:
            edit(row)
        case .reply:
            reply(row, shift)
        case .viewReactions:
            viewReactions(row)
        case .removeAllReactions:
            removeReactions(of: nil, on: row, shift: shift)
        case .pin, .unpin:
            guard offer(for: row).has(action) else {
                return
            }
            Self.pin(
                row, pins: action == .pin, shift: shift, store: store, account: account,
                ask: askPin, confirm: confirm, failed: failed)
        case .addReaction, .removeReactions, .copyId, .more:
            break
        }
    }

    static func pin(
        _ row: MessageListModel.Row, pins: Bool, shift: Bool, store: Store, account: Account,
        ask: (PinQuestion) -> Void, confirm: @escaping (Confirmation) -> Void,
        failed: @escaping (Failure) -> Void, busy: @escaping @MainActor (Bool) -> Void = { _ in }
    ) {
        let channel = store.channel(id: row.shown.channelId)
        let isPrivate = channel?.kind == .dm || channel?.kind == .groupDm
        let name = isPrivate ? nil : channel?.name ?? ""
        let question = PinQuestion(row: row, pins: pins, channelName: name) { @MainActor in
            _ = Task {
                await request(
                    row, pins: pins, channelName: name, account: account, confirm: confirm,
                    failed: failed, busy: busy)
            }
        }
        if shift {
            question.run()
        } else {
            ask(question)
        }
    }

    // Nothing changes before the gateway says so, as in the official client.
    private static func request(
        _ row: MessageListModel.Row, pins: Bool, channelName: String?, account: Account,
        confirm: @escaping (Confirmation) -> Void, failed: @escaping (Failure) -> Void,
        busy: @escaping @MainActor (Bool) -> Void
    ) async {
        let own = row.message
        busy(true)
        defer { busy(false) }
        do {
            if pins {
                try await account.pin(channelId: own.channelId, messageId: own.id)
            } else {
                try await account.unpin(channelId: own.channelId, messageId: own.id)
            }
        } catch {
            guard pins else {
                confirm(
                    Confirmation(
                        title: "The Pin Is Stuck!",
                        message: "Discord failed to unpin that message for some reason. Sorry about that.",
                        confirm: "Try Again", cancel: "Cancel", destructive: false
                    ) { @MainActor in
                        _ = Task {
                            await request(
                                row, pins: false, channelName: channelName, account: account,
                                confirm: confirm, failed: failed, busy: busy)
                        }
                    })
                return
            }
            failed(
                pinFailure(error as? PinError ?? .Request(error: .UnexpectedResponse), channelName: channelName))
        }
    }

    // `channelName` is `nil` in a DM or a group DM.
    static func pinFailure(_ error: PinError, channelName: String?) -> Failure {
        let retry = "An error occurred, please try again."
        switch error {
        case .TooMany(let limit):
            let place = channelName.map { "channel #\($0)" } ?? "this channel"
            return Failure(
                title: "The Pin Broke",
                message: "Discord is unable to pin that message. You may have hit the limit of "
                    + "\(limit) pins in \(place).")
        case .Forbidden:
            return Failure(
                title: "No Access", message: "You do not have permission to pin messages in this channel.")
        case .WrongChannel:
            return Failure(
                title: "Invalid Channel Type", message: "This message cannot be pinned in this channel.")
        case .Archived:
            return Failure(
                title: "Invalid Thread Archive State",
                message: "This message cannot be pinned in an archived thread.")
        case .SystemMessage:
            return Failure(
                title: "Invalid Message Type",
                message: "This message is a system message and cannot be pinned.")
        case .Gone:
            return Failure(
                title: "Unknown Message", message: "The message you are trying to pin does not exist.")
        case .Request(.Discord(_, _, let message)):
            return Failure(title: "The Pin Broke", message: message.isEmpty ? retry : message)
        case .Request:
            return Failure(title: "Error", message: retry)
        }
    }

    private func delete(_ row: MessageListModel.Row, shift: Bool, from surface: Surface) {
        guard offer(for: row).has(.delete) else {
            return
        }
        if row.message.delivery == .failed {
            discard(row.message.id)
            return
        }
        let question = Self.deleteQuestion(row, showsTip: surface == .menu, account: account, failed: failed)
        if shift {
            question.run()
        } else {
            askDelete(question)
        }
    }

    // Its `run` holds what it needs, not the model, which a host may have let go by the answer.
    static func deleteQuestion(
        _ row: MessageListModel.Row, showsTip: Bool, account: Account,
        failed: @escaping (Failure) -> Void
    ) -> DeleteQuestion {
        let own = row.message
        return DeleteQuestion(row: row, showsTip: showsTip) { @MainActor in
            _ = Task {
                do {
                    try await account.deleteMessage(channelId: own.channelId, messageId: own.id)
                } catch {
                    failed(
                        Failure(
                            title: "Couldn't Delete Message",
                            message: Self.text(for: error as? RequestError ?? .UnexpectedResponse)))
                }
            }
        }
    }

    /// Marks the row's channel unread from its message on (Mark Unread, Option-click); nothing
    /// where the core doesn't allow it. A mark the core says was replaced is taken back without
    /// a word: nothing failed.
    public func markUnread(from row: MessageListModel.Row) async {
        let message = row.message
        guard store.messageActions(channelId: message.channelId, messageId: message.id).markUnread
        else {
            return
        }
        let answered = marking(message.channelId, message.id)
        do {
            let outcome = try await account.markUnread(
                channelId: message.channelId, messageId: message.id)
            answered?(outcome == .marked)
        } catch {
            answered?(false)
            failed(
                Failure(
                    title: "Couldn't Mark as Unread",
                    message: Self.text(for: error as? RequestError ?? .UnexpectedResponse)))
        }
    }

    private struct Offer {
        var actions: MessageActions
        var failed: Bool
        var hasText: Bool
        // "Show emoji reactions" is on and the list has its reactions model.
        var reactions: Bool
        var hasReactions: Bool

        var allowsAnything: Bool {
            MessageAction.allCases.contains { $0 != .more && $0 != .copyText && has($0) }
        }

        func has(_ action: MessageAction) -> Bool {
            switch action {
            case .copyText: hasText
            case .retry: failed
            case .markUnread: actions.markUnread
            case .copyLink: actions.copyLink
            case .createThread: actions.createThread
            case .viewThread: actions.viewThread
            case .delete: actions.delete
            case .edit: !failed && actions.edit
            case .more: true
            case .reply: actions.reply
            case .addReaction: reactions && !failed && actions.react
            case .viewReactions: reactions && hasReactions
            case .removeReactions, .removeAllReactions:
                reactions && hasReactions && actions.removeReactions
            // The official menu leaves them out for a failed send.
            case .pin: !failed && actions.pin
            case .unpin: !failed && actions.unpin
            // Developer mode isn't built yet.
            case .copyId:
                false
            }
        }
    }

    // The core answers for a thread's starter row asked with the row's own IDs.
    private func offer(for row: MessageListModel.Row) -> Offer {
        let own = row.message
        let shows = reactions?.shows ?? false
        return Offer(
            actions: store.messageActions(channelId: own.channelId, messageId: own.id),
            failed: own.delivery == .failed, hasText: !row.shown.content.isEmpty,
            reactions: shows,
            hasReactions: shows
                && !ReactionPill.merged(store.reactions(channelId: own.channelId, messageId: own.id))
                    .isEmpty)
    }

    static func item(_ action: MessageAction, in surface: Surface, failed: Bool = false) -> MessageActionItem {
        let title =
            switch (action, surface) {
            case (.addReaction, _): "Add Reaction"
            case (.viewReactions, _): "View Reactions"
            case (.reply, _): "Reply"
            case (.edit, .bar): "Edit"
            case (.edit, .menu): "Edit Message"
            case (.createThread, _): "Create Thread"
            case (.viewThread, _): "View Thread"
            case (.copyText, _): "Copy Text"
            case (.pin, _): "Pin Message"
            case (.unpin, _): "Unpin Message"
            case (.retry, .bar): "Retry"
            case (.retry, .menu): "Resend message"
            case (.markUnread, _): "Mark Unread"
            case (.copyLink, .bar): "Copy Link"
            case (.copyLink, .menu): "Copy Message Link"
            case (.removeReactions, _): "Remove Reactions"
            case (.removeAllReactions, _): "Remove All Reactions"
            case (.delete, .bar): "Delete"
            case (.delete, .menu): "Delete Message"
            case (.copyId, _): "Copy Message ID"
            case (.more, _): "More"
            }
        let symbol =
            switch action {
            case .addReaction: "face.smiling"
            case .viewReactions: "face.smiling"
            case .reply: "arrowshape.turn.up.left"
            case .edit: "pencil"
            case .createThread, .viewThread: "text.bubble"
            case .copyText: "doc.on.doc"
            case .pin: "pin"
            case .unpin: "pin.slash"
            case .retry: "arrow.clockwise"
            case .markUnread: "message.badge"
            case .copyLink: "link"
            case .removeReactions, .removeAllReactions: "xmark.circle"
            case .delete: "trash"
            case .copyId: "number"
            case .more: "ellipsis"
            }
        // A failed send's bar button isn't red in the official client.
        let destructive =
            switch action {
            case .delete: !(failed && surface == .bar)
            case .removeReactions, .removeAllReactions: true
            default: false
            }
        return MessageActionItem(
            action: action, title: title, symbol: symbol, enabled: true, destructive: destructive)
    }

    private static func text(for error: ReactionError) -> String {
        switch error {
        case .Forbidden, .Blocked: text(for: RequestError.Discord(status: 403, code: 50013, message: ""))
        case .RateLimited: text(for: RequestError.RateLimited(retryAfter: nil))
        case .Request(let error): text(for: error)
        case .TooMany, .UnknownEmoji: text(for: RequestError.UnexpectedResponse)
        }
    }

    private static func text(for error: RequestError) -> String {
        switch error {
        case .Network:
            "Akari couldn't reach Discord. Check your connection, then try again."
        case .Discord(_, 50013, _), .Discord(403, _, _):
            "You don't have permission to do that here."
        case .RateLimited:
            "Discord is asking Akari to slow down. Try again in a moment."
        case .Closed:
            "Akari isn't connected. Reconnect, then try again."
        default:
            "Discord had a problem. Try again in a moment."
        }
    }
}
