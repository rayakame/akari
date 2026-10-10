import Testing

@testable import AkariKit

struct EventBatchTests {
    @Test
    func touchedIdsAreCollectedPerKind() {
        let batch = EventBatch([
            .ready,
            .currentUserUpdated,
            .userUpdated(userId: id(9)),
            .guildAdded(guildId: id(1)),
            .guildUpdated(guildId: id(2)),
            .guildRemoved(guildId: id(3)),
            .guildUnavailable(guildId: id(4)),
            .currentMemberUpdated(guildId: id(5)),
            .channelAdded(channelId: id(10), guildId: id(1)),
            .channelUpdated(channelId: id(11), guildId: id(1)),
            .channelRemoved(channelId: id(12), guildId: id(2)),
            .channelUpdated(channelId: id(13), guildId: nil),
            .messageInserted(channelId: id(20), messageId: id(100)),
            .messageUpdated(channelId: id(21), messageId: id(101)),
            .messageDeleted(channelId: id(22), messageId: id(102)),
            .messageReplaced(channelId: id(23), pendingId: id(103), messageId: id(104)),
            .messagesLoaded(channelId: id(24), first: id(105), last: id(106)),
            .messagesTrimmed(channelId: id(25), first: id(107), last: id(108)),
            .messagesStale(channelId: id(26)),
            .messagesCleared(channelId: id(27)),
        ])

        #expect(batch.ready)
        #expect(batch.currentUserChanged)
        #expect(batch.usersChanged == [id(9)])
        #expect(batch.guildsChanged == [id(1), id(2), id(3), id(4)])
        #expect(batch.guildListChanged)
        #expect(batch.membersChanged == [id(5)])
        let channels: [GuildId?: Set<ChannelId>] = [
            id(1): [id(10), id(11)], id(2): [id(12)], nil: [id(13)],
        ]
        #expect(batch.channelsChanged == channels)
        #expect(batch.removedChannels == [id(12)])
        #expect(batch.windowsChanged == [id(20), id(22), id(23), id(24), id(25), id(26), id(27)])
        #expect(batch.updatedMessages == [id(21): [id(101)]])
        #expect(batch.confirmed == [id(23): [id(104): id(103)]])
    }

    @Test
    func anUpdatedGuildKeepsTheListAndAnEmptyBatchTouchesNothing() {
        let updated = EventBatch([.guildUpdated(guildId: id(1))])
        let empty = EventBatch([])

        #expect(!updated.guildListChanged)
        #expect(!empty.ready && !empty.currentUserChanged && !empty.guildListChanged)
        #expect(empty.connection == nil)
        #expect(empty.guildsChanged.isEmpty && empty.membersChanged.isEmpty)
        #expect(empty.channelsChanged.isEmpty && empty.windowsChanged.isEmpty)
    }

    @Test
    func theLastConnectionStateWins() {
        let batch = EventBatch([
            .connection(state: .connecting), .ready, .connection(state: .online),
        ])

        #expect(batch.connection == .online)
    }
}
