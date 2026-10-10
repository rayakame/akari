// What one batch touched, so each model reads only what changed.
struct EventBatch {
    var ready = false
    var connection: ConnectionState?
    var currentUserChanged = false
    var guildsChanged: Set<GuildId> = []
    var guildListChanged = false
    var membersChanged: Set<GuildId> = []
    // The `nil` guild holds DMs and group DMs.
    var channelsChanged: [GuildId?: Set<ChannelId>] = [:]
    var removedChannels: Set<ChannelId> = []
    var windowsChanged: Set<ChannelId> = []
    var updatedMessages: [ChannelId: Set<MessageId>] = [:]
    // Confirmed message ID → the pending ID it replaced.
    var confirmed: [ChannelId: [MessageId: MessageId]] = [:]

    init(_ events: [StoreEvent]) {
        for event in events {
            switch event {
            case .connection(let state):
                connection = state
            case .ready:
                ready = true
            case .currentUserUpdated:
                currentUserChanged = true
            case .userUpdated:
                break
            case .guildAdded(let guild), .guildRemoved(let guild), .guildUnavailable(let guild):
                guildsChanged.insert(guild)
                guildListChanged = true
            case .guildUpdated(let guild):
                guildsChanged.insert(guild)
            case .currentMemberUpdated(let guild):
                membersChanged.insert(guild)
            case .channelAdded(let channel, let guild), .channelUpdated(let channel, let guild):
                channelsChanged[guild, default: []].insert(channel)
            case .channelRemoved(let channel, let guild):
                channelsChanged[guild, default: []].insert(channel)
                removedChannels.insert(channel)
            case .messageInserted(let channel, _), .messageDeleted(let channel, _),
                .messagesLoaded(let channel, _, _), .messagesTrimmed(let channel, _, _),
                .messagesStale(let channel), .messagesCleared(let channel):
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
