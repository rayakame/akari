import AkariKit
import AppKit
import Testing

@testable import Akari

// Discord's client highlights the message a jump lands on (its flash component and CSS, read
// 2026-10-20): #5865F2 at 41/255 over the message, faded in over 0.2 s with CSS's ease, held
// to 2 s, faded out over 0.2 s; with reduced motion on and off at once.
@MainActor
struct JumpFlashTests {
    final class Clock {
        var now = berlinTime(2026, 10, 10, 15, 0)
    }

    let clock = Clock()
    let list: MessageTableTests

    init() {
        list = MessageTableTests(onScreen: MessageTableController.isOnScreen, now: { [clock] in clock.now })
        list.window.appearance = NSAppearance(named: .darkAqua)
        list.controller.reducesMotion = { false }
    }

    func jump(to raw: UInt64, _ number: Int) -> MessageListModel.MessageJump {
        MessageListModel.MessageJump(messageId: MessageId(rawValue: raw), number: number)
    }

    func flash(of raw: UInt64) throws -> CAAnimation? {
        try #require(try list.cell(of: raw) as? MessageCell).flashAnimation
    }

    func jumped(to raw: UInt64, rows: [MessageListModel.Row]? = nil) {
        let rows = rows ?? list.page
        list.show(MessageTableState(rows: rows))
        list.show(MessageTableState(rows: rows, messageJump: jump(to: raw, 1)))
    }

    @Test
    func aJumpFlashesTheMessageItLandsOn() throws {
        jumped(to: 120)

        let fade = try #require(try flash(of: 120) as? CAKeyframeAnimation)
        #expect(fade.keyPath == "opacity")
        #expect(fade.duration == 2.2)
        #expect(fade.values as? [Double] == [0, 1, 1, 0])
        #expect(fade.keyTimes?.map(\.doubleValue) == [0, 0.2 / 2.2, 2 / 2.2, 1])
        let ease = fade.timingFunctions?.map { function in
            (0..<4).map { index in
                var point: [Float] = [0, 0]
                function.getControlPoint(at: index, values: &point)
                return point
            }
        }
        // CSS's ease is cubic-bezier(0.25, 0.1, 0.25, 1).
        #expect(ease?.first == [[0, 0], [0.25, 0.1], [0.25, 1], [1, 1]])
        #expect(ease?.last == ease?.first)
        #expect(try flash(of: 121) == nil)
    }

    // The same in both themes.
    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theFlashIsDiscordsBlurpleOverTheMessagesBox(appearance: NSAppearance.Name) throws {
        list.window.appearance = NSAppearance(named: appearance)
        jumped(to: 120)
        let cell = try #require(try list.cell(of: 120) as? MessageCell)

        let color = try #require(cell.flashLayer.backgroundColor.flatMap(NSColor.init(cgColor:)))
        let srgb = try #require(color.usingColorSpace(.sRGB))
        #expect(abs(srgb.redComponent * 255 - 88) < 0.5)
        #expect(abs(srgb.greenComponent * 255 - 101) < 0.5)
        #expect(abs(srgb.blueComponent * 255 - 242) < 0.5)
        #expect(abs(srgb.alphaComponent * 255 - 41) < 0.5)
        #expect(cell.flashFrame == cell.box, "\(cell.flashFrame) \(cell.box)")
        #expect(cell.flashLayer.cornerRadius == 4)
        #expect(cell.flashLayer.maskedCorners == [.layerMaxXMinYCorner, .layerMaxXMaxYCorner])
    }

    // A legacy scroller's gutter can come and go without resizing the cell (CI's Macs).
    @Test
    func theFlashFollowsALegacyScrollersGutter() throws {
        list.controller.scrollView.scrollerStyle = .legacy
        NotificationCenter.default.post(
            name: NSScroller.preferredScrollerStyleDidChangeNotification, object: nil)

        jumped(to: 120)

        let cell = try #require(try list.cell(of: 120) as? MessageCell)
        #expect(cell.flashFrame == cell.box, "\(cell.flashFrame) \(cell.box)")
    }

    @Test
    func withReducedMotionItComesAndGoesAtOnce() throws {
        list.controller.reducesMotion = { true }

        jumped(to: 120)

        let hold = try #require(try flash(of: 120) as? CABasicAnimation)
        #expect(hold.fromValue as? Double == 1)
        #expect(hold.toValue as? Double == 1)
        #expect(hold.duration == 2)
    }

    // Discord flashes only the message itself.
    @Test
    func aJumpToAGoneMessageFlashesNothing() throws {
        jumped(to: 120, rows: list.page.filter { $0.id.rawValue != 120 })

        #expect(try flash(of: 121) == nil)
    }

    @Test
    func theUnreadBarsJumpDoesntFlash() throws {
        let line = MessageId(rawValue: 150)
        list.show(MessageTableState(rows: list.page, newMessages: line))
        list.show(MessageTableState(rows: list.page, newMessages: line, unreadJumps: 1))

        #expect(try flash(of: 150) == nil)
    }

    // A cell shown again mid-flash carries on; one shown for another message, or after the 2.2 s,
    // doesn't.
    @Test
    func aCellShownAgainKeepsOnlyAFlashStillRunning() throws {
        jumped(to: 120)
        let cell = try #require(try list.cell(of: 120) as? MessageCell)

        cell.flash(elapsed: nil, reducesMotion: false)
        #expect(cell.flashAnimation == nil)
        clock.now += 1
        list.show(MessageTableState(rows: list.page + [row(201, at: list.noon)], messageJump: jump(to: 120, 1)))
        list.table.reloadData(forRowIndexes: [list.index(of: 120)], columnIndexes: [0])
        let shown = try #require(try list.cell(of: 120) as? MessageCell)
        let begin = try #require(shown.flashAnimation).beginTime
        let layerNow = shown.flashLayer.convertTime(CACurrentMediaTime(), from: nil)
        #expect(abs(layerNow - begin - 1) < 0.1, "it carries on 1 s in")

        clock.now += 1.3
        list.table.reloadData(forRowIndexes: [list.index(of: 120)], columnIndexes: [0])
        #expect(try flash(of: 120) == nil)
    }

    // Discord has one flash at a time.
    @Test
    func aSecondJumpEndsTheFirstsFlash() throws {
        jumped(to: 121)

        list.show(MessageTableState(rows: list.page, messageJump: jump(to: 120, 2)))

        #expect(list.visibleRows.contains(list.index(of: 121)))
        #expect(try flash(of: 121) == nil)
        #expect(try flash(of: 120) != nil)
    }

    @Test
    func aCellReusedForAnotherRowCarriesNoFlash() throws {
        jumped(to: 120)

        list.scroll(toRowOf: 195)

        let flashing = list.visibleRows.filter { row in
            (list.table.view(atColumn: 0, row: row, makeIfNecessary: false) as? MessageCell)?
                .flashAnimation != nil
        }
        #expect(flashing.isEmpty)
    }

    @Test
    func jumpingAgainRestartsTheFlash() throws {
        jumped(to: 120)
        clock.now += 3

        list.show(MessageTableState(rows: list.page, messageJump: jump(to: 120, 2)))

        #expect(try flash(of: 120) != nil)
    }
}
