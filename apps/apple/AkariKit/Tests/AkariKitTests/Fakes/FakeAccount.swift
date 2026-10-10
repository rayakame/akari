import AkariKit
import Foundation

final class FakeAccount: Account, @unchecked Sendable {
    let fakeStore: FakeStore
    let connectError = Locked<GatewayError?>(nil)

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
}
