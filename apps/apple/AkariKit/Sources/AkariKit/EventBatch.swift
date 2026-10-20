// What one batch touched, so each model reads only what changed.
struct EventBatch {
    var ready = false
    var restored = false
    var connection: ConnectionState?
    // Any state but online, even when the batch ends online again: the connection dropped.
    var leftOnline = false
    var currentUserChanged = false
    var usersChanged: Set<UserId> = []
    var guildsChanged: Set<GuildId> = []
    var guildListChanged = false
    var membersChanged: Set<GuildId> = []
    // The `nil` guild holds the DM settings.
    var guildSettingsChanged: Set<GuildId?> = []
    // The `nil` guild holds DMs and group DMs.
    var channelsChanged: [GuildId?: Set<ChannelId>] = [:]
    // Channels whose badge or read state changed, by guild (`nil`: DMs).
    var badgesChanged: [GuildId?: Set<ChannelId>] = [:]
    var guildBadgesChanged: Set<GuildId> = []
    var removedChannels: Set<ChannelId> = []
    var windowsChanged: Set<ChannelId> = []
    var updatedMessages: [ChannelId: Set<MessageId>] = [:]
    // Confirmed message ID → the pending ID it replaced.
    var confirmed: [ChannelId: [MessageId: MessageId]] = [:]

    // Read everything again: a new session, or the disk cache's state before the first one.
    var everythingChanged: Bool { ready || restored }

    init(_ events: [StoreEvent]) {
        for event in events {
            switch event {
            case .connection(let state):
                connection = state
                if state != .online {
                    leftOnline = true
                }
            case .ready:
                ready = true
            case .restored:
                restored = true
            case .currentUserUpdated:
                currentUserChanged = true
            case .userUpdated(let user):
                usersChanged.insert(user)
            case .guildAdded(let guild), .guildRemoved(let guild), .guildUnavailable(let guild):
                guildsChanged.insert(guild)
                guildListChanged = true
            case .guildUpdated(let guild):
                guildsChanged.insert(guild)
            case .currentMemberUpdated(let guild):
                membersChanged.insert(guild)
            case .serverListChanged:
                guildListChanged = true
            case .guildSettingsChanged(let guild):
                guildSettingsChanged.insert(guild)
            case .badgeChanged(let channel, let guild):
                badgesChanged[guild, default: []].insert(channel)
            case .guildBadgeChanged(let guild):
                guildBadgesChanged.insert(guild)
            case .channelAdded(let channel, let guild), .channelUpdated(let channel, let guild):
                channelsChanged[guild, default: []].insert(channel)
            case .threadListingChanged(let guild, let thread),
                .threadUnavailable(let guild, let thread):
                channelsChanged[guild, default: []].insert(thread)
            case .channelRemoved(let channel, let guild):
                channelsChanged[guild, default: []].insert(channel)
                removedChannels.insert(channel)
            case .messageInserted(let channel, _), .messageDeleted(let channel, _),
                .messagesLoaded(let channel, _, _), .messagesTrimmed(let channel, _, _),
                .messagesStale(let channel), .messagesRefreshed(let channel),
                .messagesCleared(let channel):
                windowsChanged.insert(channel)
            case .messageUpdated(let channel, let message):
                updatedMessages[channel, default: []].insert(message)
            case .messageReplaced(let channel, let pending, let message):
                windowsChanged.insert(channel)
                confirmed[channel, default: [:]][message] = pending
            }
        }
    }
}
