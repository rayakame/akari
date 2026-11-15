import AkariKit
import AppKit
import SwiftUI

/// A message outside a list, as the list draws it at a group's start: the thread form's source
/// message, whose text can be selected and copied, and in the delete question and later the pins'
/// modals and popout, where nothing of it takes the pointer.
struct MessagePreview: NSViewRepresentable {
    let message: Message
    /// The line over it when it answers a message; shown, not clickable.
    var reply: ReplyLine?
    var now = Date()
    @Environment(\.imageCache) private var images

    func makeNSView(context: Context) -> MessagePreviewView {
        MessagePreviewView()
    }

    func updateNSView(_ view: MessagePreviewView, context: Context) {
        view.cell.images = images
        Self.configure(view.cell, with: message, reply: reply, now: now)
    }

    /// `authorName` is the header's name, as the list resolves it; without `spaced` the row
    /// leaves out the space a list keeps before a group.
    static func configure(
        _ cell: MessageCell, with message: Message, reply: ReplyLine? = nil, authorName: String? = nil,
        menu: Bool = true, spaced: Bool = true, now: Date
    ) {
        cell.menuItems = { menu ? MessageMenu.copyText(message) : [] }
        let calendar = Calendar.current
        let locale = Locale.current
        cell.configure(
            message, body: Self.text(message, now: now), authorName: authorName, startsGroup: true,
            followsLine: !spaced,
            groupTime: MessageFormat.groupTime(
                message.timestamp, now: now, calendar: calendar, locale: locale),
            shortTime: MessageFormat.shortTime(message.timestamp, calendar: calendar, locale: locale),
            fullDate: MessageFormat.fullDate(message.timestamp, calendar: calendar, locale: locale),
            embeds: embeds(message, now: now),
            reply: reply.map { line in
                let times = TimestampText(
                    locale: locale, timeZone: calendar.timeZone, calendar: calendar)
                return (line, ReplyLineView.text(of: line, now: now, times: times))
            }, replyJumps: false)
    }

    // As the list shows them, nothing revealed.
    private static func embeds(_ message: Message, now: Date) -> [EmbedView.Shown] {
        let calendar = Calendar.current
        let locale = Locale.current
        let times = TimestampText(locale: locale, timeZone: calendar.timeZone, calendar: calendar)
        return message.embeds.map { embed in
            EmbedView.shown(
                embed, cover: MessageCell.cover(embed, MessageCell.MediaState()),
                spoilersShown: message.spoilersShown, revealed: [], now: now, times: times,
                footerTime: embed.footer?.timestamp.map {
                    MessageFormat.footerTime($0, now: now, calendar: calendar, locale: locale)
                })
        }
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: MessagePreviewView, context: Context)
        -> CGSize?
    {
        let width = proposal.width ?? SplitWidth.standard
        return CGSize(width: width, height: Self.height(of: message, reply: reply, width: width))
    }

    static func height(
        of message: Message, reply: ReplyLine? = nil, spaced: Bool = true, width: CGFloat
    ) -> CGFloat {
        let now = Date()
        return MessageCell.height(
            message, body: text(message, now: now), startsGroup: true, followsLine: !spaced,
            embeds: embeds(message, now: now), hasReply: reply != nil, width: width)
    }

    private static func text(_ message: Message, now: Date) -> NSAttributedString {
        MessageText.build(
            message, revealed: [], now: now,
            times: TimestampText(locale: .current, timeZone: .current, calendar: .current)
        ).text
    }
}

/// The preview's cell with a pointer of its own, as no list tracks it: the I-beam over its text,
/// the hand over a link, the arrow elsewhere. Not `interactive`, nothing of it takes the pointer.
final class MessagePreviewView: NSView, CursorAuthority {
    let cell = MessageCell()
    let interactive: Bool
    private var tracking: NSTrackingArea?

    init(interactive: Bool = true) {
        self.interactive = interactive
        super.init(frame: .zero)
        cell.expressionsEnabled = false
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        interactive ? super.hitTest(point) : nil
    }

    required init?(coder: NSCoder) {
        nil
    }

    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        place()
    }

    override func layout() {
        super.layout()
        place()
    }

    // SwiftUI lays the view out before it has a size, where the cell's margins can't hold.
    private func place() {
        guard bounds.width > 0, bounds.height > 0 else {
            return
        }
        cell.frame = bounds
        if cell.superview == nil {
            addSubview(cell)
        }
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let tracking {
            removeTrackingArea(tracking)
        }
        guard interactive else {
            return
        }
        let tracking = NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .mouseMoved, .activeInActiveApp, .inVisibleRect],
            owner: self)
        addTrackingArea(tracking)
        self.tracking = tracking
    }

    override func mouseEntered(with event: NSEvent) {
        cursor(at: event.locationInWindow)?.set()
    }

    override func mouseMoved(with event: NSEvent) {
        cursor(at: event.locationInWindow)?.set()
    }

    override func mouseExited(with event: NSEvent) {
        if let window {
            CursorArea.cursor(at: event.locationInWindow, in: window).set()
        }
    }

    func cursor(at point: NSPoint) -> NSCursor? {
        guard interactive, let superview, bounds.contains(convert(point, from: nil)),
            let hit = hitTest(superview.convert(point, from: nil))
        else {
            return nil
        }
        var view: NSView? = hit
        while let current = view, current !== self {
            if let text = current as? MessageTextView {
                return text.cursor(at: text.convert(point, from: nil))
            }
            view = current.superview
        }
        return .arrow
    }
}

/// A message in the official confirmations' box (the delete question, later the pins'): the
/// panel's color, radius 3, a 1 ring and a soft shadow, 10 above and below the row, which keeps
/// its own insets; nothing in it takes the pointer, and past `maxHeight` it scrolls.
final class MessagePreviewBox: NSView {
    static let padding: CGFloat = 10
    static let radius: CGFloat = 3

    let preview = MessagePreviewView(interactive: false)
    private let scroll = NSScrollView()
    private let document = FlippedView()
    private var shown: MessageListModel.Row?

    override var isFlipped: Bool { true }
    override var wantsUpdateLayer: Bool { true }

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = Self.radius
        layer?.borderWidth = 1
        layer?.shadowOffset = NSSize(width: 0, height: -2)
        layer?.shadowRadius = 5
        layer?.shadowOpacity = 1
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.horizontalScrollElasticity = .none
        scroll.documentView = document
        scroll.wantsLayer = true
        scroll.layer?.cornerRadius = Self.radius
        scroll.layer?.masksToBounds = true
        document.addSubview(preview)
        addSubview(scroll)
        setAccessibilityElement(false)
    }

    required init?(coder: NSCoder) {
        nil
    }

    func show(_ row: MessageListModel.Row, images: ImageCache?, now: Date = Date()) {
        shown = row
        preview.cell.images = images
        MessagePreview.configure(
            preview.cell, with: row.shown, reply: row.reply, authorName: row.authorName, menu: false,
            spaced: false, now: now)
        needsLayout = true
    }

    /// The box's height around `row` at `width`, before any cap.
    static func height(of row: MessageListModel.Row, width: CGFloat) -> CGFloat {
        2 * padding + MessagePreview.height(of: row.shown, reply: row.reply, spaced: false, width: width)
    }

    override func layout() {
        super.layout()
        scroll.frame = bounds
        guard let shown else {
            return
        }
        let width = scroll.contentSize.width
        let row = MessagePreview.height(of: shown.shown, reply: shown.reply, spaced: false, width: width)
        document.frame = NSRect(x: 0, y: 0, width: width, height: row + 2 * Self.padding)
        preview.frame = NSRect(x: 0, y: Self.padding, width: width, height: row)
    }

    override func updateLayer() {
        layer?.backgroundColor = Palette.panel.cgColor
        layer?.borderColor = Palette.previewRing.cgColor
        layer?.shadowColor = Palette.previewShadow.cgColor
    }
}
