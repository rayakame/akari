import AkariKit
import AppKit
import Testing

@testable import Akari

// Ray's 14:30 message 20 started thread "t1"; its newest message is Mira's, 25 minutes before the
// list's now (15:00). Measures from the user's screenshots (amendment 175) and Discord's CSS.
@MainActor
struct ThreadBoxViewTests {
    let list = MessageTableTests()
    let now = berlinTime(2026, 10, 10, 15, 0)
    let newest = berlinTime(2026, 10, 10, 14, 35)

    init() {
        list.window.appearance = NSAppearance(named: .darkAqua)
    }

    func latest(_ content: String = "the newest") -> Message {
        row(25, by: author(3, "Mira"), at: newest, content: content).message
    }

    func box(_ bottom: ThreadBox.Bottom? = nil, time: Date?? = nil) -> ThreadBox {
        ThreadBox(
            threadId: ChannelId(rawValue: 20), name: "t1", link: "16 Messages ›",
            bottom: bottom ?? .latest(latest(), authorName: "Mira"), time: time ?? newest)
    }

    func shown(_ box: ThreadBox? = nil) throws -> (cell: MessageCell, box: ThreadBoxView) {
        list.show([
            row(
                20, by: author(2, "Ray"), at: berlinTime(2026, 10, 10, 14, 30), content: "test",
                hasThread: true, threadBox: box ?? self.box())
        ])
        let cell = try #require(try list.cell(of: 20) as? MessageCell)
        return (cell, try #require(Self.boxView(in: cell)))
    }

    static func boxView(in view: NSView) -> ThreadBoxView? {
        for subview in view.subviews {
            if let box = subview as? ThreadBoxView, !box.isHidden {
                return box
            }
            if let box = boxView(in: subview) {
                return box
            }
        }
        return nil
    }

    // From `view`'s top left, in points.
    static func frame(of subview: NSView, in view: NSView) -> NSRect {
        let converted = subview.convert(subview.bounds, to: view)
        return view.isFlipped
            ? converted
            : NSRect(
                x: converted.minX, y: view.bounds.height - converted.maxY,
                width: converted.width, height: converted.height)
    }

    // A token as the 2x bitmap renders it: the color space shifts it (Support.swift).
    static func rendered(_ color: NSColor) throws -> NSColor? {
        let swatch = NSView(frame: NSRect(x: 0, y: 0, width: 4, height: 4))
        swatch.wantsLayer = true
        swatch.appearance = NSAppearance(named: .darkAqua)
        swatch.effectiveAppearance.performAsCurrentDrawingAppearance {
            swatch.layer?.backgroundColor = color.cgColor
        }
        return try renderedAt2x(swatch).colorAt(x: 4, y: 4)?.usingColorSpace(.sRGB)
    }

    // Nothing drawn: the cell renders on a clear background.
    func blank(_ color: NSColor?) -> Bool {
        (color?.alphaComponent ?? 1) < 0.05
    }

    static func pixel(_ rep: NSBitmapImageRep, _ x: CGFloat, _ y: CGFloat) -> NSColor? {
        rep.colorAt(x: Int(x * 2), y: Int(y * 2))?.usingColorSpace(.sRGB)
    }

    @Test
    func aMessageWithAThreadShowsItsBox() throws {
        let (_, box) = try shown()

        #expect(box.box == self.box())
        #expect(box.accessibilityRole() == .button)
        #expect(box.accessibilityLabel() == "Thread t1, 16 Messages")
        #expect(box.accessibilityValue() as? String == "Mira: the newest, 25m ago")
    }

    @Test
    func aMessageWithoutAThreadShowsNoBox() throws {
        list.show([row(21, at: now - 60)])

        #expect(Self.boxView(in: try list.cell(of: 21)) == nil)
    }

    @Test
    func theBoxMatchesItsMeasures() throws {
        let (cell, view) = try shown()
        let content = try #require(list.messageTexts(in: cell).first)
        let frame = Self.frame(of: view, in: cell)
        let box = view.boxFrame.offsetBy(dx: frame.minX, dy: frame.minY)
        let text = Self.frame(of: content, in: cell)

        #expect(box.minX == 72)
        #expect(box.height == 56)
        #expect(box.minY - text.maxY == 8)
        #expect(box.width <= 480)

        let rep = try renderedAt2x(cell)
        let fill = try Self.rendered(Palette.threadBoxFill)
        let border = try Self.rendered(Palette.threadBoxBorder)
        // 1 pt of border, then the fill, on every side.
        #expect(same(Self.pixel(rep, box.minX + 0.25, box.midY), border))
        #expect(same(Self.pixel(rep, box.minX + 0.75, box.midY), border))
        #expect(same(Self.pixel(rep, box.minX + 1.25, box.midY), fill))
        #expect(same(Self.pixel(rep, box.maxX - 0.25, box.midY), border))
        #expect(same(Self.pixel(rep, box.maxX - 1.25, box.midY), fill))
        #expect(same(Self.pixel(rep, box.minX + 20, box.minY + 0.25), border))
        #expect(same(Self.pixel(rep, box.minX + 20, box.maxY - 0.25), border))
        #expect(blank(Self.pixel(rep, box.minX + 20, box.minY - 0.25)))
        // As Discord's, the box ends with the message.
        #expect(box.maxY == cell.bounds.height)
        // A 4 pt radius leaves the corner itself to the chat behind it.
        #expect(blank(Self.pixel(rep, box.minX + 0.25, box.minY + 0.25)))
        #expect(same(Self.pixel(rep, box.minX + 0.25, box.minY + 4.25), border))
    }

    @Test
    func theConnectorIsTheSpinesCurve() throws {
        let (cell, view) = try shown()
        let frame = Self.frame(of: view, in: cell)
        let box = view.boxFrame.offsetBy(dx: frame.minX, dy: frame.minY)
        let avatarBottom = try #require(cell.avatarFrame).maxY
        let rep = try renderedAt2x(cell)
        let spine = try Self.rendered(Palette.threadSpine)

        // 2 wide on the avatar's center, from 5 below it.
        #expect(same(Self.pixel(rep, 36.25, avatarBottom + 5.25), spine))
        #expect(same(Self.pixel(rep, 35.25, avatarBottom + 5.25), spine))
        #expect(blank(Self.pixel(rep, 34.75, avatarBottom + 8)))
        #expect(blank(Self.pixel(rep, 37.25, avatarBottom + 8)))
        #expect(blank(Self.pixel(rep, 36, avatarBottom + 4.75)))
        // Into the box's middle, ending 3 before it.
        #expect(same(Self.pixel(rep, 55, box.midY - 0.75), spine))
        #expect(same(Self.pixel(rep, 55, box.midY + 0.75), spine))
        #expect(blank(Self.pixel(rep, 55, box.midY - 1.25)))
        #expect(same(Self.pixel(rep, 68.75, box.midY), spine))
        #expect(blank(Self.pixel(rep, 69.25, box.midY)))
        // The bend: no corner, the line curves away 7 above the middle.
        #expect(same(Self.pixel(rep, 36, box.midY - 10), spine))
        #expect(blank(Self.pixel(rep, 36, box.midY)))
        #expect(blank(Self.pixel(rep, 36, box.midY - 1.5)))
    }

    @Test
    func theBoxShowsTheLatestMessage() throws {
        let (_, view) = try shown()

        #expect(view.top == ["t1", "16 Messages ›"])
        #expect(view.bottom == ["Mira", "the newest", "25m ago"])

        let rep = try renderedAt2x(view)
        func brightestInk(_ part: String) throws -> CGFloat {
            brightest(in: try #require(view.rect(of: part)), of: rep)
        }
        // textStrong at 64% over the fill (#ADADAF), textDefault (#EFEFF1), textMuted (#96979D).
        let author = try brightestInk("Mira")
        let content = try brightestInk("the newest")
        let time = try brightestInk("25m ago")
        #expect(author > 0.64 && author < 0.72, "\(author)")
        #expect(content > 0.9, "\(content)")
        #expect(time > 0.55 && time < 0.63, "\(time)")
        #expect(try brightestInk("t1") > 0.95)
    }

    @Test
    func aThreadWithNoRecentMessagesSaysSo() throws {
        let (_, view) = try shown(box(.noRecentMessages))

        #expect(view.bottom == ["There are no recent messages in this thread.", "25m ago"])
    }

    @Test
    func anArchivedThreadsLineHasNoTime() throws {
        let (_, view) = try shown(box(.noRecentMessages, time: .some(nil)))

        #expect(view.bottom == ["There are no recent messages in this thread."])
    }

    @Test
    func anEmptyNewestMessageShowsItsPlaceholder() throws {
        let (_, view) = try shown(box(.latest(latest(""), authorName: "Mira")))

        #expect(view.bottom == ["Mira", "Click to see attachment", "25m ago"])
    }

    @Test
    func clickingTheBoxOpensTheThread() throws {
        let actions = MessageTableTests.RecordingActions()
        list.controller.actions = actions
        let (_, view) = try shown()
        actions.calls = []

        view.performClick(nil)

        #expect(actions.calls == ["thread 20 from 10"])
    }

    @Test
    func onlyTheCountUnderlinesWhileHovered() throws {
        let (_, view) = try shown()
        #expect(!view.underlinesLink)

        view.hovered = true

        #expect(view.underlinesLink)
    }

    @Test
    func aLineAboutAThreadStartedWithoutAMessageHasItsBoxRightUnderIt() throws {
        list.show([
            row(
                22, by: author(2, "Ray"), at: newest, kind: .threadCreated, content: "t2",
                reference: MessageReference(
                    messageId: nil, channelId: ChannelId(rawValue: 22),
                    guildId: GuildId(rawValue: 1)),
                hasThread: true, threadBox: box())
        ])
        let cell = try #require(try list.cell(of: 22) as? NoticeCell)
        let view = try #require(Self.boxView(in: cell))
        let text = try #require(list.messageTexts(in: cell).first)

        let box = view.boxFrame.offsetBy(
            dx: Self.frame(of: view, in: cell).minX, dy: Self.frame(of: view, in: cell).minY)
        #expect(box.minY - Self.frame(of: text, in: cell).maxY == 0)
        #expect(box.minX == 72)
    }

    @Test
    func rowsWithABoxAreAsTallAsTheirCells() throws {
        list.show([
            row(
                20, by: author(2, "Ray"), at: newest, content: "test", hasThread: true,
                threadBox: box()),
            row(
                22, by: author(2, "Ray"), at: newest + 60, kind: .threadCreated, content: "t2",
                hasThread: true, threadBox: box(.noMessages, time: .some(nil))),
            row(23, by: author(2, "Ray"), at: newest + 120, content: "after"),
        ])

        for raw: UInt64 in [20, 22, 23] {
            let index = list.index(of: raw)
            #expect(list.table.rect(ofRow: index).height == list.fittingHeight(ofRow: index), "\(raw)")
        }
    }
}

// The box's time moves on with the list's relative-time timer.
@MainActor
struct ThreadBoxTimeTests {
    let refresh = TimeRefreshTests()

    @Test
    func theRelativeTimeIsShortAndAdvances() throws {
        let since = refresh.clock.now - 30
        refresh.show([
            row(
                20, by: author(2, "Ray"), at: refresh.clock.now - 3600, content: "test",
                hasThread: true,
                threadBox: ThreadBox(
                    threadId: ChannelId(rawValue: 20), name: "t1", link: "1 Message ›",
                    bottom: .noRecentMessages, time: since))
        ])
        let box = { () throws -> ThreadBoxView in
            let index = try #require(
                refresh.controller.timeline.items.firstIndex {
                    $0.id == .message(MessageId(rawValue: 20))
                })
            let cell = try #require(
                refresh.table.view(atColumn: 0, row: index, makeIfNecessary: true))
            return try #require(ThreadBoxViewTests.boxView(in: cell))
        }
        #expect(try box().bottom.last == "1m ago")
        #expect(refresh.timers.active.map(\.date) == [since + 120])

        refresh.clock.now = since + 120
        refresh.timers.fire()

        #expect(try box().bottom.last == "2m ago")
    }
}
