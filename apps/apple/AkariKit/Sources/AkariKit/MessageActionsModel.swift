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

    enum Surface {
        case bar, menu
    }

    private let store: Store
    private let account: Account
    private let copy: (String) -> Void
    private let retry: (MessageId) -> Void
    private let discard: (MessageId) -> Void
    private let createThread: (ChannelId, MessageId) -> Void
    private let viewThread: (ChannelId, ChannelId) -> Void
    private let marking: (ChannelId, MessageId) -> (() -> Void)?
    private let failed: (Failure) -> Void

    /// `copy` puts text on the host's pasteboard; `createThread` takes the message's channel and
    /// the message, `viewThread` the thread and its parent; `marking` shows a mark as unread at
    /// once and returns what takes it back.
    public init(
        store: Store, account: Account, copy: @escaping (String) -> Void,
        retry: @escaping (MessageId) -> Void, discard: @escaping (MessageId) -> Void,
        createThread: @escaping (ChannelId, MessageId) -> Void,
        viewThread: @escaping (ChannelId, ChannelId) -> Void,
        marking: @escaping (ChannelId, MessageId) -> (() -> Void)? = { _, _ in nil },
        failed: @escaping (Failure) -> Void
    ) {
        self.store = store
        self.account = account
        self.copy = copy
        self.retry = retry
        self.discard = discard
        self.createThread = createThread
        self.viewThread = viewThread
        self.marking = marking
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
            let fits = slot.when.fits(editable: offer.has(.edit), deletable: hasDelete)
            return fits && offer.has(slot.action) ? Self.item(slot.action, in: .bar) : nil
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

    /// Does what a button or an item says. `shift` is the key at the click, for the actions
    /// that read it. More is the host's: it opens the menu.
    public func perform(_ action: MessageAction, on row: MessageListModel.Row, shift: Bool = false) {
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
            if row.message.delivery == .failed {
                discard(row.message.id)
            }
        case .addReaction, .viewReactions, .reply, .edit, .pin, .unpin, .removeReactions,
            .removeAllReactions, .copyId, .more:
            break
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
        let undo = marking(message.channelId, message.id)
        do {
            let outcome = try await account.markUnread(
                channelId: message.channelId, messageId: message.id)
            if outcome == .replaced {
                undo?()
            }
        } catch {
            undo?()
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
            case .delete: failed && actions.delete
            case .more: true
            // Their features aren't built yet.
            case .addReaction, .viewReactions, .reply, .edit, .pin, .unpin, .removeReactions,
                .removeAllReactions, .copyId:
                false
            }
        }
    }

    // The core answers for a thread's starter row asked with the row's own IDs.
    private func offer(for row: MessageListModel.Row) -> Offer {
        let own = row.message
        return Offer(
            actions: store.messageActions(channelId: own.channelId, messageId: own.id),
            failed: own.delivery == .failed, hasText: !row.shown.content.isEmpty)
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
