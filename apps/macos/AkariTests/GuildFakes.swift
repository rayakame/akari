import AkariKit
import Foundation
import os

// One server with one text channel (11), and no messages.
nonisolated final class GuildAccount: Account, @unchecked Sendable {
    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    let loads = OSAllocatedUnfairLock(initialState: 0)

    override func store() -> Store {
        GuildStore()
    }

    override func loadMessages(channelId: ChannelId, load: MessageLoad) async throws {
        loads.withLock { $0 += 1 }
    }

    override func viewChannel(channelId: ChannelId) {}
}

// One server with one text channel, and no messages.
nonisolated final class GuildStore: Store, @unchecked Sendable {
    private let channel = Channel(
        id: ChannelId(rawValue: 11), kind: .guildText, guildId: GuildId(rawValue: 1),
        parentId: nil, name: "general", position: 0, topic: nil, nsfw: false,
        rateLimitPerUser: 0, recipientIds: [])

    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    override func channelList(guildId: GuildId) -> [ChannelId] {
        [channel.id]
    }

    override func channels(ids: [ChannelId]) -> [Channel] {
        ids.contains(channel.id) ? [channel] : []
    }

    override func channel(id: ChannelId) -> Channel? {
        id == channel.id ? channel : nil
    }

    override func permissions(channelId: ChannelId) -> Permissions? {
        nil
    }

    override func window(channelId: ChannelId) -> MessageWindow? {
        nil
    }

    override func messageLengthLimit() -> UInt32 {
        2000
    }

    override func slowmode(channelId: ChannelId) -> Slowmode? {
        nil
    }
}
