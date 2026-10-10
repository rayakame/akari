/// What one batch of store events touched, so each model reads only what changed.
struct EventBatch {
    var ready = false
    /// The last connection state in the batch.
    var connection: ConnectionState?
    var currentUserChanged = false
    /// Added, updated, removed or unavailable guilds.
    var guildsChanged: Set<GuildId> = []
    /// A guild was added, removed or became unavailable.
    var guildListChanged = false
    var membersChanged: Set<GuildId> = []
    /// Added, updated or removed channels per guild; `nil` holds DMs and group DMs.
    var channelsChanged: [GuildId?: Set<ChannelId>] = [:]
    var removedChannels: Set<ChannelId> = []
    /// Channels whose window gained, lost or replaced messages, or changed its state.
    var windowsChanged: Set<ChannelId> = []
    var updatedMessages: [ChannelId: Set<MessageId>] = [:]
    /// Confirmed message ID → the pending ID it replaced, per channel.
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
