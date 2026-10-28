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
    // Whether the new messages line before `line` is on screen; nothing while the window doesn't
    // show.
    func newMessagesLineOnScreen(_ onScreen: Bool, line: MessageId)
    // Whether the list is at its bottom, after the user's scrolls and the table's own.
    func atBottomChanged(_ atBottom: Bool)
    // The user's scrolls only, positive toward older messages; after atBottomChanged for a move.
    func userScrolled(by distance: CGFloat, height: CGFloat)
    func reveal(_ spoiler: SpoilerKey)
    // Wherever the channel is: another server's, or a conversation.
    func openChannel(_ id: ChannelId)
    // A "started a thread" line's thread, fetched first when the store doesn't have it.
    func openThread(_ id: ChannelId, from parent: ChannelId)
    // "threads" in a "started a thread" line: the channel's Threads modal.
    func openThreads(of channel: ChannelId)
    // "a message" in a pin's line; the table scrolls once the model has jumped.
    func jump(to message: MessageId)
    // A server a system line names.
    func openGuild(_ id: GuildId)
    // A masked link may ask first (D65).
    func openLink(_ url: URL, masked: Bool)
    // A message's "Create Thread": whether it shows, and the form it opens.
    func canCreateThread(from message: MessageId) -> Bool
    func createThread(from message: MessageId)
    // A clicked emoji's or sticker's popout; nil where there's none to show.
    func expressionPopout(_ target: ExpressionTarget) -> ExpressionPopoutModel?
    // A file's name, or media the viewer can't show: the original in the browser.
    func openMedia(_ key: ImageKey)
    // The viewer on a row's media from `index` (D146); nil leaves it to the browser.
    func viewer(_ items: [ViewerModel.Item], at index: Int, of row: MessageListModel.Row) -> ViewerModel?
    // A spoilered attachment's click, for the session.
    func revealMedia(_ key: MediaKey)
    // A sensitive attachment's eye.
    func toggleSensitive(_ key: MediaKey)
    // A GIFV's video, which plays in the list.
    func mediaURL(_ key: ImageKey) async -> URL?
}

extension MessageListActions {
    func canCreateThread(from message: MessageId) -> Bool { false }
    func createThread(from message: MessageId) {}
    func expressionPopout(_ target: ExpressionTarget) -> ExpressionPopoutModel? { nil }
    func openMedia(_ key: ImageKey) {}
    func viewer(_ items: [ViewerModel.Item], at index: Int, of row: MessageListModel.Row) -> ViewerModel? {
        nil
    }
    func revealMedia(_ key: MediaKey) {}
    func toggleSensitive(_ key: MediaKey) {}
    func mediaURL(_ key: ImageKey) async -> URL? { nil }
}
