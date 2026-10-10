import AkariKit

// What the table asks of the message list; the app wires it to the channel's models.
@MainActor
protocol MessageListActions: AnyObject {
    func retry(_ message: MessageId)
    func delete(_ message: MessageId)
}
