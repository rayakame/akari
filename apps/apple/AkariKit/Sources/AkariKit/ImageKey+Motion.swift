extension ImageKey {
    /// Whether an animated form exists: an `a_` hash, or an animated emoji. A still key never
    /// asks for an animation.
    public var canAnimate: Bool {
        switch self {
        case .avatar(_, let hash), .memberAvatar(_, _, let hash), .guildIcon(_, let hash):
            hash.hasPrefix("a_")
        case .emoji(_, let animated):
            animated
        default:
            false
        }
    }

    /// Whether `other` pictures the same user, server, group, role, emoji, sticker or
    /// attachment: a box keeps the old picture while the subject's new one loads, and clears
    /// at once for another subject.
    public func sameSubject(as other: ImageKey) -> Bool {
        switch (self, other) {
        case (.avatar(let a, _), .avatar(let b, _)), (.memberAvatar(_, let a, _), .memberAvatar(_, let b, _)),
            (.avatar(let a, _), .memberAvatar(_, let b, _)), (.memberAvatar(_, let a, _), .avatar(let b, _)):
            a == b
        case (.guildIcon(let a, _), .guildIcon(let b, _)):
            a == b
        case (.groupIcon(let a, _), .groupIcon(let b, _)):
            a == b
        case (.roleIcon(let a, _), .roleIcon(let b, _)):
            a == b
        case (.emoji(let a, _), .emoji(let b, _)):
            a == b
        case (.sticker(let a, _), .sticker(let b, _)):
            a == b
        case (.attachment(_, _, let a), .attachment(_, _, let b)):
            a == b
        case (.embedMedia(_, let a, let ea, let sa), .embedMedia(_, let b, let eb, let sb)):
            a == b && ea == eb && sa == sb
        default:
            false
        }
    }
}
