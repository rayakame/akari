import Foundation

/// One emoji's reactions on a message as one pill: plain and super reactions counted together,
/// the user's when either is theirs.
public struct ReactionPill: Equatable, Sendable, Identifiable {
    /// A custom emoji by its ID, a Unicode one by itself, as Discord tells reactions apart.
    public struct ID: Hashable, Sendable {
        public let emojiId: EmojiId?
        public let unicode: String?

        public init(_ emoji: ReactionEmoji) {
            emojiId = emoji.id
            unicode = emoji.id == nil ? emoji.name : nil
        }
    }

    public let emoji: ReactionEmoji
    public let normal: UInt32
    public let burst: UInt32
    public let mine: Bool

    public init(emoji: ReactionEmoji, normal: UInt32, burst: UInt32 = 0, mine: Bool) {
        self.emoji = emoji
        self.normal = normal
        self.burst = burst
        self.mine = mine
    }

    public init(_ reaction: Reaction) {
        self.init(
            emoji: reaction.emoji, normal: reaction.normal, burst: reaction.burst,
            mine: reaction.me || reaction.meBurst)
    }

    public var id: ID { ID(emoji) }
    public var count: UInt32 { normal + burst }

    /// The emoji as text names it: itself, or a custom one's `:name:`.
    public var name: String { Self.name(of: emoji) }

    public static func name(of emoji: ReactionEmoji) -> String {
        emoji.id == nil ? emoji.name ?? "" : ":\(emoji.name ?? "emoji"):"
    }

    /// The official client's words for a pill, as VoiceOver reads it.
    public var accessibilityLabel: String {
        let spoken = name.replacingOccurrences(of: ":", with: " ")
            .replacingOccurrences(of: "_", with: " ").trimmingCharacters(in: .whitespaces)
        let reactions = count == 1 ? "1 reaction" : "\(count) reactions"
        return "\(spoken), \(reactions), press to \(mine ? "remove your reaction" : "react")"
    }

    /// A message's pills in Discord's order; an emoji nobody reacts with has none.
    public static func merged(_ reactions: [Reaction]) -> [ReactionPill] {
        reactions.map(ReactionPill.init).filter { $0.count > 0 }
    }

    /// The pills by count, the most first, as the reactions window lists its emoji and a forum
    /// card picks its one; equal counts keep Discord's order.
    public static func byCount(_ pills: [ReactionPill]) -> [ReactionPill] {
        pills.enumerated().sorted { first, second in
            first.element.count != second.element.count
                ? first.element.count > second.element.count : first.offset < second.offset
        }.map(\.element)
    }
}

/// The reactions under a message, and what the user may do with them.
public struct ReactionRow: Equatable, Sendable {
    public let pills: [ReactionPill]
    /// The add button stands at the row's end: the user may react with a new emoji.
    public let adds: Bool
    /// A click on a pill adds or removes the user's reaction; without it the pills only show.
    public let toggles: Bool

    public init(pills: [ReactionPill], adds: Bool, toggles: Bool) {
        self.pills = pills
        self.adds = adds
        self.toggles = toggles
    }
}

/// An emoji the hover bar and the menu offer for one click.
public struct QuickReaction: Equatable, Sendable, Identifiable {
    public let emoji: ReactionEmoji
    /// The user reacted with it already: the click takes it back.
    public let mine: Bool

    public init(emoji: ReactionEmoji, mine: Bool) {
        self.emoji = emoji
        self.mine = mine
    }

    public var id: ReactionPill.ID { ReactionPill.ID(emoji) }
    public var name: String { ReactionPill.name(of: emoji) }
    /// The official client's tooltip under the emoji's name.
    public var hint: String { mine ? "Click to remove" : "Click to react" }
    public var accessibilityLabel: String {
        mine ? "Click to remove \(name)" : "Click to react with \(name)"
    }
}

/// The official client's words about reactions.
public enum ReactionWords {
    public static let add = "Add Reaction"
    public static let reactToPost = "React to Post"
    public static let window = "Reactions"
    public static let remove = "Remove Reaction"
    public static let tooManyTitle = "We appreciate the enthusiasm, but..."
    public static let tooManyText =
        "Your reaction was not added because there are too many reactions on this message."
    public static let removeAllTitle = "Remove All Reactions"
    public static let removeAllText = "Are you sure you want to remove all reactions from this message?"
    public static let removeEmojiTitle = "Remove Reactions for Emoji"
    public static let removeEmojiText =
        "Are you sure you want to remove all of the reactions for this emoji from this message?"
    public static let yes = "Yes"
    public static let no = "No"

    /// A pill's tooltip: up to three of the users who reacted by name, then how many others;
    /// without names, how many people. `nil` without anyone.
    public static func tooltip(emoji: String, names: [String], count: UInt32) -> String? {
        let names = Array(names.prefix(3))
        let others = Int(count) - names.count
        let rest = others == 1 ? "1 other" : "\(others) others"
        switch (names.count, others > 0) {
        case (0, false):
            return nil
        case (0, true):
            return "\(emoji) reacted by \(others) \(others == 1 ? "person" : "people")"
        case (1, false):
            return "\(emoji) reacted by \(names[0])"
        case (1, true):
            return "\(emoji) reacted by \(names[0]) and \(rest)"
        case (2, false):
            return "\(emoji) reacted by \(names[0]) and \(names[1])"
        case (2, true):
            return "\(emoji) reacted by \(names[0]), \(names[1]), and \(rest)"
        case (_, false):
            return "\(emoji) reacted by \(names[0]), \(names[1]), and \(names[2])"
        case (_, true):
            return "\(emoji) reacted by \(names[0]), \(names[1]), \(names[2]), and \(rest)"
        }
    }

    /// A tab of the reactions window, as VoiceOver reads it.
    public static func tab(_ pill: ReactionPill) -> String {
        "\(pill.name) \(pill.count == 1 ? "1 reaction" : "\(pill.count) reactions")"
    }
}

extension ReactionEmoji {
    /// A Unicode emoji, exactly as given.
    public static func unicode(_ emoji: String) -> ReactionEmoji {
        ReactionEmoji(id: nil, name: emoji, animated: false, key: nil)
    }

    /// A forum's default reaction; a custom one comes without its name.
    public init(_ emoji: TagEmoji) {
        switch emoji {
        case .custom(let id, let key):
            self.init(id: id, name: nil, animated: false, key: key)
        case .unicode(let emoji):
            self.init(id: nil, name: emoji, animated: false, key: nil)
        }
    }
}
