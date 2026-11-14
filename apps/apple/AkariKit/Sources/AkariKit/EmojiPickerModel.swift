import Foundation

/// The emoji picker for reacting to one message: its sections (recently used, the user's
/// servers, Unicode's groups), search, the selection the keys move over the grid, and the pick.
/// Opened by `ReactionsModel.openPicker(for:from:)`; it closes itself when its message, channel
/// or server goes, or reactions stop showing.
@MainActor
public final class EmojiPickerModel {
    /// One emoji of the grid.
    public struct Item: Equatable, Sendable, Identifiable {
        /// A Unicode emoji by itself, a custom one by its ID.
        public enum ID: Hashable, Sendable {
            case unicode(String)
            case custom(EmojiId)
        }

        public let emoji: PickerEmoji

        public init(_ emoji: PickerEmoji) {
            self.emoji = emoji
        }

        public var id: ID {
            switch emoji {
            case .unicode(let unicode): .unicode(unicode.emoji)
            case .custom(let custom): .custom(custom.id)
            }
        }

        /// `:thumbsup:`, `:name:`.
        public var name: String {
            switch emoji {
            case .unicode(let unicode): ":\(unicode.name):"
            case .custom(let custom): ":\(custom.name):"
            }
        }

        /// The name as VoiceOver reads it: `wave`, `heart eyes`.
        public var spokenName: String { ReactionPill.spoken(name) }

        /// What the system's emoji font draws; `nil` for a custom emoji.
        public var glyph: String? {
            if case .unicode(let unicode) = emoji { unicode.emoji } else { nil }
        }

        /// A custom emoji's image.
        public var key: ImageKey? {
            if case .custom(let custom) = emoji { custom.key } else { nil }
        }

        public var animated: Bool {
            if case .custom(let custom) = emoji { custom.animated } else { false }
        }

        var guildId: GuildId? {
            if case .custom(let custom) = emoji { custom.guildId } else { nil }
        }

        var access: EmojiAccess {
            if case .custom(let custom) = emoji { custom.access } else { .usable }
        }

        /// Shown dimmed, and a pick does nothing.
        public var isLocked: Bool { access != .usable }

        /// Why a locked emoji can't be used here.
        public var reason: String? {
            switch access {
            case .usable: nil
            case .unavailable: "Unavailable: its server lost the boosts it needs"
            case .nitroLocked: "Needs Nitro to be used here"
            case .roleLocked: "Only some roles may use it"
            case .external: "This channel doesn't allow other servers' emoji"
            }
        }
    }

    /// A run of the grid under its header.
    public struct Section: Equatable, Sendable, Identifiable {
        public enum Kind: Hashable, Sendable {
            case recent
            case server(GuildId)
            case unicode(EmojiGroup)
            /// What a search found; no header.
            case results
        }

        public let kind: Kind
        public let title: String
        /// A server's icon.
        public let icon: ImageKey?
        /// An SF Symbol for the strip; `nil` for a server, which has its icon or initials.
        public let symbol: String?
        public let items: [Item]

        public var id: Kind { kind }
    }

    /// An emoji's place: its section and its index there.
    public struct Position: Hashable, Sendable {
        public let section: Int
        public let item: Int

        public init(section: Int, item: Int) {
            self.section = section
            self.item = item
        }
    }

    public enum Move: Sendable {
        case up, down, left, right
    }

    /// How the grid scrolls to a new selection.
    public enum Reveal: Equatable, Sendable {
        /// Not at all: the pointer chose it.
        case none
        /// Just into view.
        case item
        /// Its section's header to the top.
        case section
    }

    public enum Change: Equatable, Sendable {
        /// Lay the grid out again; `fromTop` after a new search or the end of one.
        case sections(fromTop: Bool)
        case selection(Reveal)
    }

    /// The grid's columns.
    public static let columns = 9
    public static let searchLimit: UInt32 = 450
    /// The official client's words in the search field.
    public static let placeholder = "Find the perfect emoji"
    /// VoiceOver's name for the picker, the official client's.
    public static let label = "Reaction Picker"

    public let channelId: ChannelId
    public let messageId: MessageId
    /// Where a pick reacts from: the picker's opener.
    public let location: ReactionLocation
    public private(set) var sections: [Section] = []
    public private(set) var selection: Position?
    /// The search as typed.
    public private(set) var query = ""
    public private(set) var isOpen = true
    /// The grid changed; the host lays it out or moves to the selection.
    public var onChange: (Change) -> Void = { _ in }
    /// The picker closed: picked, closed by the host, or its message went.
    public var onClose: () -> Void = {}
    /// A pick hit a locked emoji.
    public var onRefused: () -> Void = {}
    var closed: () -> Void = {}

    private let store: Store
    private let guildId: GuildId?
    // A thread's permissions come from its parent's overwrites.
    private let places: Set<ChannelId>
    private var permissions: Permissions?
    private let react: (ReactionEmoji, ReactionLocation) async -> Void
    private var browsing: [Section] = []
    private(set) var searches = 0
    private(set) var answersHandled = 0
    private(set) var reacting: Task<Void, Never>?
    // The search whose answer shows; an older answer never replaces it.
    private var shownSearch = 0
    private var shownQuery: String?
    private var searching: Task<Void, Never>?
    // A Return before the answer to what was typed, with its Shift.
    private var waitingPick: Bool?

    init(
        channelId: ChannelId, messageId: MessageId, location: ReactionLocation, store: Store,
        react: @escaping (ReactionEmoji, ReactionLocation) async -> Void
    ) {
        self.channelId = channelId
        self.messageId = messageId
        self.location = location
        self.store = store
        self.react = react
        let channel = store.channel(id: channelId)
        guildId = channel?.guildId
        places = Set([channelId, channel?.parentId].compactMap { $0 })
        permissions = guildId == nil ? nil : store.permissions(channelId: channelId)
        browsing = browse()
        sections = browsing
        selection = first(in: sections)
    }

    public var selected: Item? {
        selection.map { sections[$0.section].items[$0.item] }
    }

    /// A search that found nothing.
    public var foundNothing: Bool {
        sections.count == 1 && sections[0].kind == .results && sections[0].items.isEmpty
    }

    /// What the footer says under an emoji's name: why it is locked, or a custom one's server.
    public func detail(of item: Item) -> String? {
        item.reason ?? item.guildId.flatMap { store.guild(id: $0)?.name }
    }

    /// The pointer's choice; it doesn't scroll.
    public func select(_ position: Position?) {
        guard let position, contains(position), position != selection else {
            return
        }
        selection = position
        onChange(.selection(.none))
    }

    /// The arrow keys: left and right go through the emoji in order, up and down keep the column,
    /// to the end of a shorter row and across sections.
    public func move(_ move: Move) {
        guard let current = selection else {
            return
        }
        let next =
            switch move {
            case .left: step(from: current, by: -1)
            case .right: step(from: current, by: 1)
            case .up: vertical(from: current, down: false)
            case .down: vertical(from: current, down: true)
            }
        guard next != current else {
            return
        }
        selection = next
        onChange(.selection(.item))
    }

    /// The strip's choice: the section's first emoji, its header at the top. A search ends.
    public func jump(to kind: Section.Kind) {
        if !query.isEmpty {
            endSearch()
        }
        guard let section = sections.firstIndex(where: { $0.kind == kind }), !sections[section].items.isEmpty
        else {
            return
        }
        selection = Position(section: section, item: 0)
        onChange(.selection(.section))
    }

    /// The search field's text, each keystroke; spaces and colons alone browse.
    public func search(_ text: String) {
        guard text != query else {
            return
        }
        query = text
        waitingPick = nil
        guard !Self.isBlank(text) else {
            endSearch(clearing: false)
            return
        }
        ask(text)
    }

    /// Reacts with the emoji at `position`, else the selection, and closes unless `keepOpen`
    /// (Shift). Without a position before the search's answer, the answer's first once it shows.
    /// `false` for a locked emoji (`onRefused`) or none: nothing happens.
    @discardableResult
    public func pick(_ position: Position? = nil, keepOpen: Bool) -> Bool {
        guard isOpen else {
            return false
        }
        if position == nil, !Self.isBlank(query), shownQuery != query {
            waitingPick = keepOpen
            return true
        }
        waitingPick = nil
        guard let position = position ?? selection, contains(position) else {
            return false
        }
        if selection != position {
            selection = position
            onChange(.selection(.none))
        }
        let item = sections[position.section].items[position.item]
        guard !item.isLocked else {
            onRefused()
            return false
        }
        let emoji = pickerReaction(emoji: item.emoji)
        if !keepOpen {
            close()
        }
        let react = react
        let location = location
        reacting = Task { await react(emoji, location) }
        return true
    }

    /// Escape, a click outside, the host going.
    public func close() {
        guard isOpen else {
            return
        }
        isOpen = false
        waitingPick = nil
        searching?.cancel()
        searching = nil
        onClose()
        closed()
    }

    func apply(_ batch: EventBatch) {
        guard isOpen else {
            return
        }
        if batch.deletedMessages[channelId]?.contains(messageId) == true
            || batch.removedChannels.contains(channelId)
            || guildId.map(batch.removedGuilds.contains) == true
        {
            close()
            return
        }
        guard mayChangeLists(batch) else {
            return
        }
        let fresh = browse()
        guard fresh != browsing else {
            return
        }
        browsing = fresh
        if Self.isBlank(query) {
            let kept = selection.flatMap { kept($0, in: fresh) }
            sections = fresh
            selection = kept ?? first(in: sections)
            onChange(.sections(fromTop: false))
        } else {
            ask(query, again: true)
        }
    }

    // A DM's channel changes with every message, a server with any role's edit: only what decides
    // which emoji the lists hold, and how a listed server shows, reads them again.
    private func mayChangeLists(_ batch: EventBatch) -> Bool {
        if batch.everythingChanged || !batch.emojisChanged.isEmpty || batch.guildListChanged
            || batch.currentUserChanged || !batch.membersChanged.isEmpty
        {
            return true
        }
        let listed = browsing.compactMap { section -> GuildId? in
            if case .server(let guild) = section.kind { guild } else { nil }
        }
        var changed = !batch.guildsChanged.isDisjoint(with: listed)
        if let guildId,
            batch.guildsChanged.contains(guildId)
                || batch.channelsChanged[guildId].map({ !$0.isDisjoint(with: places) }) == true
        {
            let now = store.permissions(channelId: channelId)
            changed = changed || now != permissions
            permissions = now
        }
        return changed
    }

    // MARK: Sections

    private func browse() -> [Section] {
        let newest = EmojiCatalog.newest
        var sections: [Section] = []
        let recent = store.recentReactions(channelId: channelId, newest: newest)
        if !recent.isEmpty {
            sections.append(
                Section(
                    kind: .recent, title: "Frequently Used", icon: nil, symbol: "clock",
                    items: recent.map(Item.init)))
        }
        for server in store.customEmojiSections(channelId: channelId) where !server.emojis.isEmpty {
            let guild = store.guild(id: server.guildId)
            sections.append(
                Section(
                    kind: .server(server.guildId), title: guild?.name ?? "", icon: guild?.icon,
                    symbol: nil, items: server.emojis.map { Item(.custom(emoji: $0)) }))
        }
        for group in EmojiCatalog.groups {
            sections.append(
                Section(
                    kind: .unicode(group.group), title: Self.title(of: group.group), icon: nil,
                    symbol: Self.symbol(of: group.group),
                    items: group.emojis.map { Item(.unicode(emoji: $0)) }))
        }
        return sections
    }

    static func title(of group: EmojiGroup) -> String {
        switch group {
        case .smileysAndEmotion: "Smileys & Emotion"
        case .peopleAndBody: "People & Body"
        case .animalsAndNature: "Animals & Nature"
        case .foodAndDrink: "Food & Drink"
        case .travelAndPlaces: "Travel & Places"
        case .activities: "Activities"
        case .objects: "Objects"
        case .symbols: "Symbols"
        case .flags: "Flags"
        }
    }

    static func symbol(of group: EmojiGroup) -> String {
        switch group {
        case .smileysAndEmotion: "face.smiling"
        case .peopleAndBody: "hand.wave"
        case .animalsAndNature: "pawprint"
        case .foodAndDrink: "fork.knife"
        case .travelAndPlaces: "car"
        case .activities: "basketball"
        case .objects: "lightbulb"
        case .symbols: "heart"
        case .flags: "flag"
        }
    }

    // MARK: Search

    private static func isBlank(_ text: String) -> Bool {
        text.allSatisfy { $0.isWhitespace || $0 == ":" }
    }

    private func ask(_ text: String, again: Bool = false) {
        searches += 1
        let asked = searches
        let store = store
        let channel = channelId
        searching = Task { [weak self] in
            let found = await store.searchEmoji(
                channelId: channel, query: text, newest: EmojiCatalog.newest, limit: Self.searchLimit)
            self?.answer(found, to: text, asked: asked, again: again)
        }
    }

    private func answer(_ found: [PickerEmoji], to text: String, asked: Int, again: Bool) {
        answersHandled += 1
        guard isOpen, asked > shownSearch, !Self.isBlank(query) else {
            return
        }
        shownSearch = asked
        let results = [Section(kind: .results, title: "", icon: nil, symbol: nil, items: found.map(Item.init))]
        let refresh = again && shownQuery == text
        shownQuery = text
        if !refresh {
            sections = results
            selection = first(in: sections)
            onChange(.sections(fromTop: true))
        } else if results != sections {
            let kept = selection.flatMap { kept($0, in: results) }
            sections = results
            selection = kept ?? first(in: sections)
            onChange(.sections(fromTop: false))
        }
        if let keepOpen = waitingPick, shownQuery == query {
            pick(keepOpen: keepOpen)
        }
    }

    private func endSearch(clearing: Bool = true) {
        if clearing {
            query = ""
        }
        waitingPick = nil
        shownQuery = nil
        // Answers still coming are older than what shows now.
        shownSearch = searches
        guard sections != browsing else {
            return
        }
        sections = browsing
        selection = first(in: sections)
        onChange(.sections(fromTop: true))
    }

    // MARK: Positions

    private func contains(_ position: Position) -> Bool {
        sections.indices.contains(position.section)
            && sections[position.section].items.indices.contains(position.item)
    }

    private func first(in sections: [Section]) -> Position? {
        sections.firstIndex { !$0.items.isEmpty }.map { Position(section: $0, item: 0) }
    }

    // Its own section first: an emoji can also be in Frequently Used, far from where the user is.
    private func kept(_ position: Position, in new: [Section]) -> Position? {
        let section = sections[position.section]
        let id = section.items[position.item].id
        if let index = new.firstIndex(where: { $0.kind == section.kind }), !new[index].items.isEmpty {
            let items = new[index].items
            return Position(
                section: index, item: items.firstIndex { $0.id == id } ?? min(position.item, items.count - 1))
        }
        return self.position(of: id, in: new)
    }

    private func position(of id: Item.ID, in sections: [Section]) -> Position? {
        for (section, entry) in sections.enumerated() {
            if let item = entry.items.firstIndex(where: { $0.id == id }) {
                return Position(section: section, item: item)
            }
        }
        return nil
    }

    private func step(from position: Position, by offset: Int) -> Position {
        let item = position.item + offset
        if sections[position.section].items.indices.contains(item) {
            return Position(section: position.section, item: item)
        }
        let neighbour = neighbouring(position.section, down: offset > 0)
        guard let neighbour else {
            return position
        }
        return Position(section: neighbour, item: offset > 0 ? 0 : sections[neighbour].items.count - 1)
    }

    private func vertical(from position: Position, down: Bool) -> Position {
        let columns = Self.columns
        let count = sections[position.section].items.count
        let row = position.item / columns
        let column = position.item % columns
        let lastRow = (count - 1) / columns
        if down, row < lastRow {
            return Position(section: position.section, item: min((row + 1) * columns + column, count - 1))
        }
        if !down, row > 0 {
            return Position(section: position.section, item: (row - 1) * columns + column)
        }
        guard let neighbour = neighbouring(position.section, down: down) else {
            return position
        }
        let other = sections[neighbour].items.count
        let start = down ? 0 : (other - 1) / columns * columns
        return Position(section: neighbour, item: min(start + column, other - 1))
    }

    private func neighbouring(_ section: Int, down: Bool) -> Int? {
        var index = section + (down ? 1 : -1)
        while sections.indices.contains(index) {
            if !sections[index].items.isEmpty {
                return index
            }
            index += down ? 1 : -1
        }
        return nil
    }
}
