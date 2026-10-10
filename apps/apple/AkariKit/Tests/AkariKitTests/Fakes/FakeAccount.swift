import AkariKit
import Foundation

final class FakeAccount: Account, @unchecked Sendable {
    let fakeStore: FakeStore
    let connectError = Locked<GatewayError?>(nil)
    // While set, each load waits for a reply: `nil` succeeds, an error fails it.
    let holdLoads = Locked(false)
    let loadReplies = AsyncQueue<RequestError?>()
    let loadError = Locked<RequestError?>(nil)
    // The window a successful load leaves in the store, as akari-core's load would.
    let windowAfterLoad = Locked<MessageWindow?>(nil)
    // Fails sends and retries.
    let sendError = Locked<RequestError?>(nil)

    init(store: FakeStore = FakeStore()) {
        fakeStore = store
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    var log: CallLog { fakeStore.log }

    override func store() -> Store {
        fakeStore
    }

    override func connect() throws {
        log.append(.connect)
        if let error = connectError.current {
            throw error
        }
    }

    override func disconnect() {
        log.append(.disconnect)
    }

    // Like akari-core: the closed state is the subscription's last event.
    override func close() {
        log.append(.close)
        fakeStore.subscription.send(.connection(state: .closed(error: nil)))
        fakeStore.subscription.end()
    }

    override func viewChannel(channelId: ChannelId) {
        log.append(.view(channelId))
    }

    override func loadMessages(channelId: ChannelId, load: MessageLoad) async throws {
        log.append(.load(channelId, load))
        let error = holdLoads.current ? await loadReplies.next() ?? nil : loadError.current
        if let error {
            throw error
        }
        if let window = windowAfterLoad.current {
            fakeStore.update { $0.windows[channelId] = window }
        }
    }

    override func sendMessage(channelId: ChannelId, content: String) async throws -> MessageId {
        log.append(.send(channelId, content))
        if let error = sendError.current {
            throw error
        }
        return id(900)
    }

    override func retryMessage(channelId: ChannelId, pendingId: MessageId) async throws
        -> MessageId
    {
        log.append(.retry(channelId, pendingId))
        if let error = sendError.current {
            throw error
        }
        return id(901)
    }

    override func discardMessage(channelId: ChannelId, pendingId: MessageId) {
        log.append(.discard(channelId, pendingId))
    }
}
