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
    _ raw: UInt64, guild: UInt64? = 1, kind: ChannelType = .guildText, name: String? = nil,
    parent: UInt64? = nil
) -> Channel {
    Channel(
        id: id(raw), kind: kind, guildId: guild.map(id), parentId: parent.map(id),
        name: name ?? (guild == nil ? nil : "channel-\(raw)"), position: 0, topic: nil,
        nsfw: false, rateLimitPerUser: 0, recipientIds: [], thread: nil
    )
}

func settings(
    muted: Bool = false, suppressEveryone: Bool = false, hideMutedChannels: Bool = false
) -> GuildSettings {
    GuildSettings(
        muted: muted, muteEndsAt: nil, suppressEveryone: suppressEveryone, suppressRoles: false,
        hideMutedChannels: hideMutedChannels
    )
}

func folder(_ raw: Int64, _ guilds: [UInt64], name: String? = nil) -> ServerListItem {
    .folder(folder: Folder(id: raw, name: name, color: nil, guildIds: guilds.map(id)))
}

func dm(_ raw: UInt64, with recipient: UInt64) -> Channel {
    Channel(
        id: id(raw), kind: .dm, guildId: nil, parentId: nil, name: nil, position: 0, topic: nil,
        nsfw: false, rateLimitPerUser: 0, recipientIds: [id(recipient)], thread: nil
    )
}

func group(_ raw: UInt64, name: String? = nil, with recipients: [UInt64]) -> Channel {
    Channel(
        id: id(raw), kind: .groupDm, guildId: nil, parentId: nil, name: name, position: 0,
        topic: nil, nsfw: false, rateLimitPerUser: 0, recipientIds: recipients.map(id), thread: nil
    )
}

func message(
    _ raw: UInt64, in channelRaw: UInt64 = 10, by author: User = user(1),
    content: String? = nil, delivery: Delivery = .sent
) -> Message {
    let content = content ?? "message \(raw)"
    return Message(
        id: id(raw), channelId: id(channelRaw), kind: .default, author: author,
        fromWebhook: false, content: content,
        timestamp: Date(timeIntervalSince1970: 1_700_000_000 + Double(raw)),
        editedTimestamp: nil, pinned: false, mentionEveryone: false, attachments: [],
        embedCount: 0, stickerNames: [], componentsV2: false, delivery: delivery,
        formatted: formattedText(content: content), mentionsMe: false
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

func success(_ raw: UInt64, token: String = "new.token") -> LoginSuccess {
    LoginSuccess(userId: id(raw), token: FakeToken(token), passwordUpdateRequired: false)
}

func mfa(_ methods: [MfaMethod], smsSentTo: String? = nil) -> MfaChallenge {
    MfaChallenge(methods: methods, webauthnOptions: nil, smsSentTo: smsSentTo)
}

let captcha = CaptchaChallenge(
    service: "hcaptcha", sitekey: "site-key", rqdata: nil, rqtoken: nil, sessionId: nil,
    shouldServeInvisible: false
)

func loadReport(
    headers: TimeInterval = 0.182, newConnection: Bool = false, received: UInt16 = 50,
    remaining: UInt32? = 4, resetAfter: TimeInterval? = 1.2, parse: TimeInterval = 0.0004,
    parsed: UInt32 = 50
) -> LoadReport {
    LoadReport(
        outcome: .applied, queued: 0, limited: 0.0004, headers: headers,
        newConnection: newConnection, body: 0.012,
        bytes: 48_213, decode: 0.0021, store: 0.0004, received: received, retries: 0,
        remaining: remaining, resetAfter: resetAfter, parse: parse, parsed: parsed
    )
}

let noBadge = Badge(unread: false, mentions: 0)

func badge(unread: Bool = true, mentions: UInt32 = 0) -> Badge {
    Badge(unread: unread, mentions: mentions)
}
