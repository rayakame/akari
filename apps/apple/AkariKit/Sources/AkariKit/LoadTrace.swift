/// One load of a message list, for the `load` log.
public struct LoadTrace: Equatable, Sendable {
    public let sequence: Int
    public let channelId: ChannelId
    public let kind: MessageListModel.Load
    /// `nil` when there was nothing to load, or the load failed.
    public let report: LoadReport?
    public let failure: RequestError?
    /// The rows the load added; for an older page, only those above the rows it had.
    public let page: Set<MessageId>
    /// The model's reads after the load.
    public let read: Duration
    public let started: ContinuousClock.Instant
    /// From the start until the model had the result.
    public let duration: Duration

    public init(
        sequence: Int, channelId: ChannelId, kind: MessageListModel.Load, report: LoadReport?,
        failure: RequestError?, page: Set<MessageId> = [], read: Duration,
        started: ContinuousClock.Instant, duration: Duration
    ) {
        self.sequence = sequence
        self.channelId = channelId
        self.kind = kind
        self.report = report
        self.failure = failure
        self.page = page
        self.read = read
        self.started = started
        self.duration = duration
    }
}
