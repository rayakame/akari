import Observation

/// A channel's messages.
@MainActor @Observable
public final class MessageListModel {
    public let channelId: ChannelId

    @ObservationIgnored private let account: Account
    @ObservationIgnored private let store: Store

    init(channelId: ChannelId, account: Account, store: Store) {
        self.channelId = channelId
        self.account = account
        self.store = store
    }

    func apply(_ batch: EventBatch) {}
}
