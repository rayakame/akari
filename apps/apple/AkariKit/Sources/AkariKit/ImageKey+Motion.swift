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
}
