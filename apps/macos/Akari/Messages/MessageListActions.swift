import AkariKit

// What the table asks of the message list; the app wires it to the channel's models.
@MainActor
protocol MessageListActions: AnyObject {
    func retry(_ message: MessageId)
    func delete(_ message: MessageId)
    func loadMore(_ edge: MessageTimeline.Edge)
    // The model's latest page; scrolling to the bottom is the table's part.
    func jumpToLatest()
    // One action for Escape from the list and the composer; read state adds marking as read.
    func escape()
}
