import AkariKit
import Foundation

// What the table asks of the message list; the app wires it to the channel's models.
@MainActor
protocol MessageListActions: AnyObject {
    func retry(_ message: MessageId)
    func delete(_ message: MessageId)
    func loadMore(_ edge: MessageTimeline.Edge)
    // The model's latest page; scrolling to the bottom is the table's part.
    func jumpToLatest()
    // One action for Escape from the list and the composer: marks read and jumps to the present.
    func escape()
    // Shift+Escape from the list and the composer.
    func markServerRead()
    // Shift+Page Up from the list and the composer; the table scrolls once the model has jumped.
    func jumpToUnread()
    // Whether the new messages line is on screen; nothing while the window doesn't show.
    func newMessagesLineOnScreen(_ onScreen: Bool)
    // The user scrolled away from the bottom, or the list is back there.
    func atBottomChanged(_ atBottom: Bool)
    func reveal(_ spoiler: SpoilerKey)
    // Wherever the channel is: another server's, or a conversation.
    func openChannel(_ id: ChannelId)
    // A masked link may ask first (D65).
    func openLink(_ url: URL, masked: Bool)
}
