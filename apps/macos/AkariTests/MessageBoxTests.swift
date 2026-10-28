import AkariKit
import AppKit
import Testing

@testable import Akari

// Measured in the official client at 2 px per point: in a group start's box the avatar starts 4
// below the top, the name's baseline sits at 18.5 and the text's at 41; a follow-up's text
// line starts at the same place within its line.
@MainActor
struct MessageBoxTests {
    let window = TestWindow.make(size: NSSize(width: 600, height: 300))

    struct Laid {
        let cell: MessageCell
        let height: CGFloat
        // From the box's top.
        let avatar: CGFloat?
        let name: CGFloat?
        let text: CGFloat
    }

    func laid(startsGroup: Bool, followsDay: Bool = false) throws -> Laid {
        let message = row(1, at: berlinTime(2026, 10, 10, 12, 0), content: "test").message
        let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)
        let body = MessageText.build(message, revealed: [], now: .now, times: times).text
        let height = MessageCell.height(
            message, body: body, startsGroup: startsGroup, followsDay: followsDay, width: 600)
        let cell = MessageCell()
        cell.configure(
            message, body: body, startsGroup: startsGroup, followsDay: followsDay,
            groupTime: "12:00", shortTime: "12:00", fullDate: "")
        cell.translatesAutoresizingMaskIntoConstraints = true
        cell.frame = NSRect(x: 0, y: 0, width: 600, height: height)
        window.contentView?.addSubview(cell)
        cell.layoutSubtreeIfNeeded()
        let boxTop = cell.isFlipped ? cell.box.minY : cell.bounds.height - cell.box.maxY
        let fromBoxTop = { (view: NSView, y: CGFloat) in
            let frame = view.convert(view.bounds, to: cell)
            return (cell.isFlipped ? frame.minY : cell.bounds.height - frame.maxY) + y - boxTop
        }
        func views(_ view: NSView) -> [NSView] {
            [view] + view.subviews.flatMap(views)
        }
        let shown = views(cell).filter { !$0.isHiddenOrHasHiddenAncestor }
        let avatar = shown.first { $0 is ImageLayerView }.map { fromBoxTop($0, 0) }
        let name = shown.compactMap { $0 as? NSTextField }.first { $0.stringValue == "Mira" }
            .map { fromBoxTop($0, $0.firstBaselineOffsetFromTop) }
        let text = try #require(shown.compactMap { $0 as? MessageTextView }.first)
        let layout = try #require(text.layoutManager)
        let line = layout.lineFragmentRect(forGlyphAt: 0, effectiveRange: nil)
        let baseline = line.minY + layout.location(forGlyphAt: 0).y + text.textContainerOrigin.y
        return Laid(
            cell: cell, height: height, avatar: avatar, name: name,
            text: fromBoxTop(text, baseline))
    }

    @Test
    func aGroupStartsContentSitsWhereTheOfficialClientsDoes() throws {
        let start = try laid(startsGroup: true)

        // The list's rhythm stays: 16 before the group, a 47 box.
        #expect(start.height == 63)
        #expect(start.cell.box.height == 47)
        let name = try #require(start.name)
        #expect(start.avatar == 4)
        #expect(abs(name - 18.5) <= 0.5)
        #expect(abs(start.text - 41) <= 0.5)
    }

    // The divider above it brings most of the space: its box starts 17.5 below the divider's
    // line, and the divider's row ends 11.5 below it.
    @Test
    func aDaysFirstMessageSitsCloserToItsDivider() throws {
        let first = try laid(startsGroup: true, followsDay: true)
        let fromTop = first.cell.isFlipped
            ? first.cell.box.minY : first.cell.bounds.height - first.cell.box.maxY

        #expect(fromTop == 6)
        #expect(first.height == 53)
        #expect(first.cell.box.height == 47)
        #expect(first.avatar == 4)
    }

    @Test
    func aFollowUpsTextSitsWhereTheOfficialClientsDoes() throws {
        let follow = try laid(startsGroup: false)

        #expect(follow.height == 26)
        #expect(follow.cell.box.height == 26)
        #expect(abs(follow.text - 19) <= 0.5)
    }
}
