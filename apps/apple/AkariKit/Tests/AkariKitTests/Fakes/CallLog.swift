import AkariKit
import Foundation

// What the fakes were asked to do, in order, shared by a fake account and its store.
final class CallLog: Sendable {
    enum Call: Equatable {
        case subscribe
        case read(Read)
        case connect
        case disconnect
        case close
        case view(ChannelId)
        case load(ChannelId, MessageLoad)
        case supersede(ChannelId)
        case send(ChannelId, String)
        case deliver(ChannelId, MessageId)
        case retry(ChannelId, MessageId)
        case discard(ChannelId, MessageId)
        case fetchThread(ChannelId, ChannelId)
        case checkThread(ChannelId, ChannelId)
        case joinThread(ChannelId, ThreadLocation)
        case leaveThread(ChannelId, ThreadLocation)
        case flushCache
    }

    enum Read: Equatable {
        case connection
        case currentUser
        case guildIds
        case serverList
        case guildSettings(GuildId?)
        case mutedChannels([ChannelId])
        case guild(GuildId)
        case unavailableGuildIds
        case channelList(GuildId)
        case privateChannelList
        case user(UserId)
        case channel(ChannelId)
        case channels([ChannelId])
        case permissions(ChannelId)
        case window(ChannelId)
        case messages(ChannelId, [MessageId])
        case starterMessage(ChannelId)
        case latestMessage(ChannelId)
        case threadUnavailable(ChannelId)
        case messageLengthLimit
        case slowmode(ChannelId)
        case channelBadges([ChannelId])
        case guildBadges([GuildId])
        case readState(ChannelId)
        case unreadPrivateChannels
    }

    private let state = Locked<(calls: [Call], offMainThread: Int)>(([], 0))

    var calls: [Call] { state.current.calls }

    var reads: [Read] {
        calls.compactMap { call in
            if case .read(let read) = call { read } else { nil }
        }
    }

    // Everything but reads: subscriptions and account actions.
    var actions: [Call] {
        calls.filter { call in
            if case .read = call { false } else { true }
        }
    }

    // Store reads made off the main thread; view models only read on it.
    var readsOffTheMainThread: Int { state.current.offMainThread }

    func append(_ call: Call) {
        let main = Thread.isMainThread
        state.withLock { state in
            state.calls.append(call)
            if case .read = call, !main {
                state.offMainThread += 1
            }
        }
    }

    func forget() {
        state.withLock { $0.calls = [] }
    }
}
