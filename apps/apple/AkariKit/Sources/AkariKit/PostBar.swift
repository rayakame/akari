import AkariFFI

/// The action bar under a forum post's first message.
public struct PostBar: Equatable, Sendable {
    /// The first message's ID, which is the post's.
    public let post: MessageId
    public let following: Bool
    /// Not an archived post's.
    public let canFollow: Bool
    /// Whether "Start the conversation!" stands under the bar of a post without replies: the user
    /// may answer and didn't start it.
    public let invites: Bool
    /// The first message's reactions, in Discord's order; the bar shows as many as fit.
    public let reactions: [ReactionPill]
    /// The forum's default reaction, offered alone while the post has no reactions and the
    /// user may react.
    public let defaultReaction: ReactionEmoji?
    /// The user may react with a new emoji: "React to Post", the add button.
    public let reacts: Bool
    /// A click on a pill adds or removes the user's reaction.
    public let toggles: Bool

    public init(
        post: MessageId, following: Bool, canFollow: Bool, invites: Bool,
        reactions: [ReactionPill] = [], defaultReaction: ReactionEmoji? = nil, reacts: Bool = false,
        toggles: Bool = false
    ) {
        self.post = post
        self.following = following
        self.canFollow = canFollow
        self.invites = invites
        self.reactions = reactions
        self.defaultReaction = defaultReaction
        self.reacts = reacts
        self.toggles = toggles
    }

    /// How many pills the bar shows in `width`, as the official client counts them; the rest
    /// stand behind "+N".
    public static func pillsShown(width: Double) -> Int {
        max(0, Int(((width - 280) / 58).rounded(.down)))
    }

    public static let follow = "Follow"
    public static let following = "Following"
    public static let followTip =
        "Add this post to your channel list and receive notifications about it."
    public static let copyLink = "Copy Link"
    public static let copied = "Copied"
    public static let inviteTitle = "Start the conversation!"
    public static let inviteText = "Be the first to share what you think!"

    /// The link "Copy Link" gives, as the official client's.
    public static func link(guild: GuildId, post: ChannelId) -> String {
        "https://discord.com/channels/\(guild.rawValue)/\(post.rawValue)"
    }
}

/// What a forum post's beginning shows over its first message.
public struct PostStart: Equatable, Sendable {
    /// The post's tags the user may apply, in the post's order: moderated ones only for those
    /// who manage threads, as the official client shows them there.
    public let tags: [ForumTag]
    /// The start is loaded and the first message isn't there.
    public let firstMessageDeleted: Bool
    /// A media channel's post: its channel's symbol stands over it.
    public let isMedia: Bool

    public static let deleted = "Original message was deleted"

    public init(tags: [ForumTag], firstMessageDeleted: Bool, isMedia: Bool = false) {
        self.tags = tags
        self.firstMessageDeleted = firstMessageDeleted
        self.isMedia = isMedia
    }
}
