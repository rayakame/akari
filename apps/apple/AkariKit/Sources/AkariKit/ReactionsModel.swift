import Foundation

/// A list's reactions: what a click on a pill or a quick reaction does, what a refusal shows,
/// and the names in a pill's tooltip. The core changes the store at once and takes the change
/// back itself when Discord refuses; nothing here waits for an answer before the pills change.
@MainActor
public final class ReactionsModel {
    /// How many emoji Add Reaction's submenu lists, as in the official client.
    public static let menuLimit = 12
    /// How long the pointer rests on a pill before its names are asked, as the official
    /// client's tooltip opens.
    public static let tooltipDelay = Duration.milliseconds(500)
    /// How many pills' names are kept: the ones asked longest ago go first.
    public static let namesKept = 64

    private struct NamesKey: Hashable {
        let message: MessageId
        let pill: ReactionPill.ID
    }

    private let store: Store
    private let account: Account
    private let delay: Duration
    /// "Show emoji reactions": off, no pill shows and nothing offers a reaction.
    public private(set) var shows: Bool
    /// The one alert a refusal shows (too many reactions).
    var failed: (MessageActionsModel.Failure) -> Void = { _ in }
    /// Discord refused without words (blocked, not allowed): the host's stand-in for the
    /// official client's shake.
    public var refused: () -> Void = {}
    /// A pill's names arrived: its tooltip reads differently now.
    public var tooltipChanged: (MessageId, ReactionPill.ID) -> Void = { _, _ in }
    /// The open emoji picker; one at a time.
    public private(set) var picker: EmojiPickerModel?
    // With the number of the ask that brought them: the lowest goes first.
    private var names: [NamesKey: (names: [String], ask: Int)] = [:]
    private var asking: [NamesKey: Int] = [:]
    private var asks = 0
    private var resting: (key: NamesKey, task: Task<Void, Never>)?
    var wait: @Sendable (Duration) async throws -> Void = { try await Task.sleep(for: $0) }

    public init(store: Store, account: Account, tooltipDelay: Duration = ReactionsModel.tooltipDelay) {
        self.store = store
        self.account = account
        delay = tooltipDelay
        shows = store.displaySettings().renderReactions
    }

    /// The reactions under the message asked for with a row's own IDs; `nil` without any, or
    /// while the setting hides them.
    public func row(channelId: ChannelId, messageId: MessageId) -> ReactionRow? {
        row(
            channelId: channelId, messageId: messageId,
            reactions: store.reactions(channelId: channelId, messageId: messageId))
    }

    func row(channelId: ChannelId, messageId: MessageId, reactions: [Reaction]) -> ReactionRow? {
        let pills = shows ? ReactionPill.merged(reactions) : []
        guard !pills.isEmpty else {
            return nil
        }
        let actions = store.messageActions(channelId: channelId, messageId: messageId)
        return ReactionRow(pills: pills, adds: actions.react, toggles: actions.toggleReaction)
    }

    /// The emoji the hover bar and the menu offer for one click; none where the user may not
    /// react with a new emoji or the setting is off.
    public func quick(for row: MessageListModel.Row) -> [QuickReaction] {
        let own = row.message
        guard offersReactions(on: own) else {
            return []
        }
        let quick = store.quickReactions(channelId: own.channelId, newest: EmojiCatalog.newest)
        return quickReactions(quick.map { pickerReaction(emoji: $0) }, on: own)
    }

    private func offersReactions(on message: Message) -> Bool {
        shows && message.delivery == .sent
            && store.messageActions(channelId: message.channelId, messageId: message.id).react
    }

    private func quickReactions(_ emoji: [ReactionEmoji], on message: Message) -> [QuickReaction] {
        let mine = Set(
            ReactionPill.merged(store.reactions(channelId: message.channelId, messageId: message.id))
                .filter(\.mine).map(\.id))
        return emoji.map { QuickReaction(emoji: $0, mine: mine.contains(ReactionPill.ID($0))) }
    }

    static func usable(_ emoji: PickerEmoji) -> Bool {
        if case .custom(let custom) = emoji {
            return custom.access == .usable
        }
        return true
    }

    /// Add Reaction's submenu: the recently used reactions the user may use here, then the
    /// quick reactions, each once, `menuLimit` at most; none where the bar has no quick ones.
    public func menu(for row: MessageListModel.Row) -> [QuickReaction] {
        let own = row.message
        guard offersReactions(on: own) else {
            return []
        }
        var seen = Set<ReactionPill.ID>()
        let recent = store.recentReactions(channelId: own.channelId, newest: EmojiCatalog.newest)
            .filter { Self.usable($0) }
        let quick = store.quickReactions(channelId: own.channelId, newest: EmojiCatalog.newest)
        let emoji = (recent + quick).map { pickerReaction(emoji: $0) }
            .filter { seen.insert(ReactionPill.ID($0)).inserted }
        return quickReactions(Array(emoji.prefix(Self.menuLimit)), on: own)
    }

    /// The emoji picker for the row's message, in place of one already open; `nil` where Add
    /// Reaction isn't offered. `location` is the opener's, which a pick reacts from.
    public func openPicker(
        for row: MessageListModel.Row, from location: ReactionLocation
    ) -> EmojiPickerModel? {
        let own = row.message
        guard offersReactions(on: own) else {
            return nil
        }
        picker?.close()
        let (channel, message) = (own.channelId, own.id)
        let picker = EmojiPickerModel(
            channelId: channel, messageId: message, location: location, store: store
        ) { [weak self] emoji, location in
            await self?.add(emoji, channelId: channel, messageId: message, from: location)
        }
        picker.closed = { [weak self, weak picker] in
            if let self, self.picker === picker {
                self.picker = nil
            }
        }
        self.picker = picker
        return picker
    }

    /// A pick from the emoji picker: the user's reaction with `emoji`, never taken back when it
    /// is theirs already.
    public func add(
        _ emoji: ReactionEmoji, channelId: ChannelId, messageId: MessageId,
        from location: ReactionLocation
    ) async {
        await react(emoji, channelId: channelId, messageId: messageId, from: location, takesBack: false)
    }

    /// A click on a pill: takes the user's reaction back, or adds it. Nothing where the pills
    /// only show.
    public func toggle(
        _ pill: ReactionPill, on row: MessageListModel.Row,
        from location: ReactionLocation = .inlineButton
    ) async {
        await toggle(pill.emoji, on: row, from: location)
    }

    /// A click on an emoji (a quick reaction, a forum's default reaction): the user's reaction
    /// with it is added, or taken back when it is theirs already.
    public func toggle(
        _ emoji: ReactionEmoji, on row: MessageListModel.Row, from location: ReactionLocation
    ) async {
        await toggle(
            emoji, channelId: row.message.channelId, messageId: row.message.id, from: location)
    }

    public func toggle(
        _ emoji: ReactionEmoji, channelId: ChannelId, messageId: MessageId,
        from location: ReactionLocation
    ) async {
        await react(emoji, channelId: channelId, messageId: messageId, from: location, takesBack: true)
    }

    private func react(
        _ emoji: ReactionEmoji, channelId: ChannelId, messageId: MessageId,
        from location: ReactionLocation, takesBack: Bool
    ) async {
        guard shows else {
            return
        }
        let actions = store.messageActions(channelId: channelId, messageId: messageId)
        let current = ReactionPill.merged(store.reactions(channelId: channelId, messageId: messageId))
            .first { $0.id == ReactionPill.ID(emoji) }
        guard current == nil ? actions.react : actions.toggleReaction,
            takesBack || current?.mine != true
        else {
            return
        }
        do {
            if current?.mine == true {
                try await account.unreact(
                    channelId: channelId, messageId: messageId, emoji: emoji, location: location)
            } else {
                try await account.react(
                    channelId: channelId, messageId: messageId, emoji: emoji, location: location)
            }
        } catch {
            show(error as? ReactionError ?? .Request(error: .UnexpectedResponse))
        }
    }

    // The store went back already; only what the official client shows is shown.
    private func show(_ error: ReactionError) {
        switch error {
        case .TooMany:
            failed(.init(title: ReactionWords.tooManyTitle, message: ReactionWords.tooManyText))
        case .Blocked, .Forbidden:
            refused()
        case .UnknownEmoji, .RateLimited, .Request:
            break
        }
    }

    /// A pill's tooltip as it reads now: with the names once they arrived, else how many
    /// people reacted.
    public func tooltip(of pill: ReactionPill, on row: MessageListModel.Row) -> String? {
        let known = names[NamesKey(message: row.message.id, pill: pill.id)]?.names ?? []
        return ReactionWords.tooltip(emoji: pill.name, names: known, count: pill.count)
    }

    /// The pointer rests on a pill: its first three names are asked once, after the tooltip's
    /// delay, and kept until the row's reactions change.
    public func pointerEntered(_ pill: ReactionPill, on row: MessageListModel.Row) {
        let key = NamesKey(message: row.message.id, pill: pill.id)
        guard resting?.key != key else {
            return
        }
        resting?.task.cancel()
        resting = nil
        guard names[key] == nil, asking[key] == nil else {
            return
        }
        let (channel, message) = (row.message.channelId, row.message.id)
        let task = Task { [weak self, delay, wait] in
            try? await wait(delay)
            guard !Task.isCancelled else {
                return
            }
            await self?.ask(key, pill, channelId: channel, messageId: message)
        }
        resting = (key, task)
    }

    public func pointerLeft() {
        resting?.task.cancel()
        resting = nil
    }

    private func ask(
        _ key: NamesKey, _ pill: ReactionPill, channelId: ChannelId, messageId: MessageId
    ) async {
        asks += 1
        let ask = asks
        asking[key] = ask
        var found: [String] = []
        for kind in [ReactionKind.normal, .burst] where found.count < 3 {
            guard (kind == .normal ? pill.normal : pill.burst) > 0 else {
                continue
            }
            let page = try? await account.reactors(
                channelId: channelId, messageId: messageId, emoji: pill.emoji, kind: kind,
                after: nil, limit: 3)
            for name in page?.users.map(\.displayName) ?? [] where !found.contains(name) {
                found.append(name)
            }
        }
        // The reactions changed meanwhile: these names are of the ones before.
        guard asking[key] == ask else {
            return
        }
        asking[key] = nil
        guard !found.isEmpty else {
            return
        }
        names[key] = (Array(found.prefix(3)), ask)
        if names.count > Self.namesKept, let oldest = names.min(by: { $0.value.ask < $1.value.ask }) {
            names[oldest.key] = nil
        }
        tooltipChanged(key.message, key.pill)
    }

    func apply(_ batch: EventBatch) {
        if batch.everythingChanged || batch.displaySettingsChanged {
            shows = store.displaySettings().renderReactions
        }
        if !shows {
            picker?.close()
        }
        picker?.apply(batch)
        // A message that arrives whole again may bring other reactions.
        let changed =
            batch.everythingChanged
            ? nil
            : [batch.reactionsChanged, batch.updatedMessages].flatMap(\.values)
                .reduce(into: Set<MessageId>()) { $0.formUnion($1) }
        guard changed?.isEmpty != true else {
            return
        }
        let stale = { (key: NamesKey) in changed?.contains(key.message) ?? true }
        names = names.filter { !stale($0.key) }
        asking = asking.filter { !stale($0.key) }
        if let resting, stale(resting.key) {
            resting.task.cancel()
            self.resting = nil
        }
    }
}
