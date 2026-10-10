extension Channel {
    /// Text and announcement channels, DMs, group DMs and threads. Voice, stage, forum, media
    /// and directory channels don't show a message list in Akari yet.
    public var opensMessageList: Bool {
        switch kind {
        case .guildText, .guildNews, .dm, .groupDm, .newsThread, .publicThread, .privateThread:
            true
        default:
            false
        }
    }
}
