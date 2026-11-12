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

    public init(post: MessageId, following: Bool, canFollow: Bool, invites: Bool) {
        self.post = post
        self.following = following
        self.canFollow = canFollow
        self.invites = invites
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
