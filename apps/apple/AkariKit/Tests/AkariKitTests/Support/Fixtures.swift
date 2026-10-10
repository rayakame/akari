import AkariKit
import Foundation

func id<Marker>(_ raw: UInt64) -> Snowflake<Marker> {
    Snowflake(rawValue: raw)
}

func user(_ raw: UInt64, name: String = "someone") -> User {
    User(
        id: id(raw), username: name, globalName: nil, displayName: name, bot: false,
        system: false
    )
}

func guild(_ raw: UInt64, name: String? = nil) -> Guild {
    Guild(id: id(raw), name: name ?? "Guild \(raw)")
}

func channel(
    _ raw: UInt64, guild: UInt64? = 1, kind: ChannelType = .guildText, name: String? = nil
) -> Channel {
    Channel(
        id: id(raw), kind: kind, guildId: guild.map(id), parentId: nil,
        name: name ?? (guild == nil ? nil : "channel-\(raw)"), position: 0, topic: nil,
        nsfw: false, rateLimitPerUser: 0, recipientIds: []
    )
}

func message(
    _ raw: UInt64, in channelRaw: UInt64 = 10, content: String? = nil,
    delivery: Delivery = .sent
) -> Message {
    Message(
        id: id(raw), channelId: id(channelRaw), kind: .default, author: user(1),
        fromWebhook: false, content: content ?? "message \(raw)",
        timestamp: Date(timeIntervalSince1970: 1_700_000_000 + Double(raw)),
        editedTimestamp: nil, pinned: false, mentionEveryone: false, attachments: [],
        embedCount: 0, stickerNames: [], delivery: delivery
    )
}

func window(
    _ ids: [UInt64], pending: [UInt64] = [], latest: Bool = true, oldest: Bool = false,
    stale: Bool = false
) -> MessageWindow {
    MessageWindow(
        messageIds: ids.map(id), pendingIds: pending.map(id), latest: latest, oldest: oldest,
        stale: stale
    )
}
