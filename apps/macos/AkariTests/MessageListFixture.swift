import AkariKit
import AppKit
import Testing

@testable import Akari

// A message list in a window, with its actions recorded.
@MainActor
final class MessageListFixture {
    let controller: MessageTableController
    let window: NSWindow
    let actions = MessageTableTests.RecordingActions()
    let noon = berlinTime(2026, 10, 10, 12, 0)
    private(set) var state = MessageTableState(rows: [])

    init(
        width: CGFloat = 600, height: CGFloat = 400,
        build: @escaping MessageTableController.Build = MessageText.build
    ) {
        controller = MessageTableController(
            calendar: { berlin }, now: { berlinTime(2026, 10, 10, 15, 0) }, pointer: { nil },
            build: build)
        window = TestWindow.make(size: NSSize(width: width, height: height), style: [.titled, .resizable])
        window.contentView = controller.scrollView
        controller.actions = actions
        window.layoutIfNeeded()
    }

    func show(_ rows: [MessageListModel.Row], revealed: Set<SpoilerKey> = []) {
        state = MessageTableState(rows: rows, reachedOldest: true, revealedSpoilers: revealed)
        controller.show(state)
        window.layoutIfNeeded()
    }

    func reveal(_ keys: Set<SpoilerKey>) {
        state.revealedSpoilers = keys
        controller.show(state)
        window.layoutIfNeeded()
    }

    var table: NSTableView { controller.tableView }

    func index(of raw: UInt64) -> Int {
        controller.timeline.items.firstIndex { $0.id == .message(MessageId(rawValue: raw)) } ?? -1
    }

    func text(of raw: UInt64) throws -> MessageTextView {
        let cell = try #require(
            table.view(atColumn: 0, row: index(of: raw), makeIfNecessary: true))
        cell.layoutSubtreeIfNeeded()
        return try #require(Self.messageText(in: cell))
    }

    static func messageText(in view: NSView) -> MessageTextView? {
        view.subviews.lazy.compactMap { ($0 as? MessageTextView) ?? messageText(in: $0) }.first
    }

    // What NSTextView calls when a link is clicked.
    func click(_ shown: String, in raw: UInt64) throws {
        let text = try text(of: raw)
        let index = (text.string as NSString).range(of: shown).location
        try #require(index != NSNotFound, "\(shown) in \(text.string)")
        let link = try #require(text.textStorage?.attribute(.link, at: index, effectiveRange: nil))
        text.clicked(onLink: link, at: index)
    }
}

// Names as the store would give them once loaded: links to channel 9 point to a missing one.
func named(_ row: MessageListModel.Row) -> MessageListModel.Row {
    var message = row.message
    for block in message.formatted.blocks.indices {
        for span in message.formatted.blocks[block].spans.indices {
            let kind = message.formatted.blocks[block].spans[span].kind
            switch kind {
            case .user(let id, _):
                message.formatted.blocks[block].spans[span].kind = .user(
                    id: id, name: .known(name: "mira"))
            case .role(let id, _, _):
                message.formatted.blocks[block].spans[span].kind = .role(
                    id: id, name: .known(name: "Mods"), color: 0)
            case .channel(let id, _, _):
                message.formatted.blocks[block].spans[span].kind = .channel(
                    id: id, name: .known(name: "general"), kind: .guildText)
            default:
                break
            }
        }
    }
    for index in message.formatted.links.indices {
        guard let channel = message.formatted.links[index].channel else {
            continue
        }
        message.formatted.links[index].channel?.name =
            channel.channelId.rawValue == 9 ? .missing : .known(name: "general")
        message.formatted.links[index].channel?.kind = .guildText
    }
    return MessageListModel.Row(id: row.id, message: message)
}
