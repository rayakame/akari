import AkariKit
import Foundation

final class FakeStore: Store, @unchecked Sendable {
    struct State {
        var connection: ConnectionState = .offline
        var currentUser: User?
        var guildIds: [GuildId] = []
        var guilds: [GuildId: Guild] = [:]
        var unavailableGuildIds: [GuildId] = []
        var channelLists: [GuildId: [ChannelId]] = [:]
        var privateChannels: [ChannelId] = []
        var users: [UserId: User] = [:]
        var channels: [ChannelId: Channel] = [:]
        var permissions: [ChannelId: Permissions] = [:]
        var windows: [ChannelId: MessageWindow] = [:]
        var messages: [MessageId: Message] = [:]
        var lengthLimit: UInt32 = 2000
        var slowmodes: [ChannelId: Slowmode] = [:]

        mutating func add(_ guilds: Guild...) {
            for guild in guilds {
                guildIds.append(guild.id)
                self.guilds[guild.id] = guild
            }
        }

        mutating func list(_ channels: [Channel], in guild: UInt64) {
            channelLists[id(guild)] = channels.map(\.id)
            for channel in channels {
                self.channels[channel.id] = channel
            }
        }

        mutating func talk(_ channels: Channel..., with users: User...) {
            for channel in channels {
                privateChannels.append(channel.id)
                self.channels[channel.id] = channel
            }
            for user in users {
                self.users[user.id] = user
            }
        }

        mutating func show(_ messages: Message...) {
            for message in messages {
                self.messages[message.id] = message
            }
        }
    }

    let subscription: FakeSubscription
    let log: CallLog
    private let state: Locked<State>

    init(_ state: State = State(), subscription: FakeSubscription = FakeSubscription()) {
        self.state = Locked(state)
        self.subscription = subscription
        self.log = CallLog()
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    var reads: [CallLog.Read] { log.reads }

    func update(_ change: (inout State) -> Void) {
        state.withLock(change)
    }

    private func read<T>(_ read: CallLog.Read, _ value: (State) -> T) -> T {
        log.append(.read(read))
        return value(state.current)
    }

    override func subscribe() -> StoreSubscription {
        log.append(.subscribe)
        return subscription
    }

    override func connection() -> ConnectionState {
        read(.connection) { $0.connection }
    }

    override func currentUser() -> User? {
        read(.currentUser) { $0.currentUser }
    }

    override func user(id: UserId) -> User? {
        read(.user(id)) { $0.users[id] }
    }

    override func guildIds() -> [GuildId] {
        read(.guildIds) { $0.guildIds }
    }

    override func guild(id: GuildId) -> Guild? {
        read(.guild(id)) { $0.guilds[id] }
    }

    override func unavailableGuildIds() -> [GuildId] {
        read(.unavailableGuildIds) { $0.unavailableGuildIds }
    }

    override func channelList(guildId: GuildId) -> [ChannelId] {
        read(.channelList(guildId)) { $0.channelLists[guildId] ?? [] }
    }

    override func privateChannelList() -> [ChannelId] {
        read(.privateChannelList) { $0.privateChannels }
    }

    override func channel(id: ChannelId) -> Channel? {
        read(.channel(id)) { $0.channels[id] }
    }

    override func channels(ids: [ChannelId]) -> [Channel] {
        read(.channels(ids)) { state in ids.compactMap { state.channels[$0] } }
    }

    override func permissions(channelId: ChannelId) -> Permissions? {
        read(.permissions(channelId)) { $0.permissions[channelId] }
    }

    override func window(channelId: ChannelId) -> MessageWindow? {
        read(.window(channelId)) { $0.windows[channelId] }
    }

    override func messages(channelId: ChannelId, ids: [MessageId]) -> [Message] {
        read(.messages(channelId, ids)) { state in
            ids.compactMap { state.messages[$0] }.filter { $0.channelId == channelId }
        }
    }

    override func messageLengthLimit() -> UInt32 {
        read(.messageLengthLimit) { $0.lengthLimit }
    }

    override func slowmode(channelId: ChannelId) -> Slowmode? {
        read(.slowmode(channelId)) { $0.slowmodes[channelId] }
    }
}
