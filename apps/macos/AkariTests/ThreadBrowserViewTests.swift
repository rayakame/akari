import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// Discord's numbers (canary 646904, CSS and module 751479) and Chris's two screenshots: the
// popout 480 × 651 in an 813-point window, the modal 680 × 700 with 56 above and below.
@MainActor
struct ThreadBrowserLayoutTests {
    @Test(arguments: [(1100.0, 480.0), (1500.0, 525.0), (2000.0, 600.0)])
    func thePopoutIs35PercentOfTheWindowBetween480And600(window: CGFloat, width: CGFloat) {
        #expect(ThreadBrowserLayout.popoutWidth(window: window) == width)
    }

    @Test
    func thePopoutIsAsTallAsItsListBetween400And80PercentOfTheWindow() {
        #expect(abs(ThreadBrowserLayout.popoutHeight(list: 1000, window: 813) - 650.4) < 0.01)
        #expect(ThreadBrowserLayout.popoutHeight(list: 100, window: 813) == 400)
        #expect(ThreadBrowserLayout.popoutHeight(list: 300, window: 813) == 400)
        #expect(ThreadBrowserLayout.popoutHeight(list: 450, window: 813) == 498)
        // CSS: the minimum wins over the maximum.
        #expect(ThreadBrowserLayout.popoutHeight(list: 1000, window: 400) == 400)
    }

    @Test
    func theModalIs680WideAndStops56FromTheWindowsEdges() {
        let size = ThreadBrowserLayout.modalSize(list: 2000, window: NSSize(width: 1355, height: 812))
        #expect(size == NSSize(width: 680, height: 700))
        let narrow = ThreadBrowserLayout.modalSize(list: 100, window: NSSize(width: 600, height: 812))
        #expect(narrow.width == 552)
        // 8 above the browser, its 48 header, the list, 16 below, the border.
        let tall: CGFloat = 8 + 48 + 100 + 16 + 2
        #expect(narrow.height == tall)
    }

    @Test
    func theListCountsEachLabelOnceAndEachCardWithItsGap() {
        let nine: CGFloat = 8 + 32 + 9 * 80
        let three: CGFloat = 8 + 3 * 32 + 6 * 80
        let results: CGFloat = 16 + 3 * 80 + 8
        #expect(ThreadBrowserLayout.listHeight(sections: [9]) == nine)
        #expect(ThreadBrowserLayout.listHeight(sections: [2, 3, 1]) == three)
        #expect(ThreadBrowserLayout.listHeight(results: 3) == results)
        #expect(ThreadBrowserLayout.emptyHeight == 400)
    }

    // Measured: 236 in the popout and 315 in the modal, both with Discord's Create button.
    @Test
    func theSearchFieldSharesTheFreeSpaceWithTheSpacer() {
        #expect(ThreadBrowserLayout.searchWidth(free: 257) == 236)
        #expect(abs(ThreadBrowserLayout.searchWidth(free: 425) - 315) <= 5)
        #expect(ThreadBrowserLayout.searchWidth(free: 100) == 100)
    }
}

@MainActor
struct ThreadBrowserViewTests {
    let store = FakeBrowserStore()

    func rendered(
        _ presentation: ThreadBrowserModel.Presentation = .popout,
        size: NSSize = NSSize(width: 480, height: 651)
    ) throws -> Rendered {
        let session = store.session()
        session.openThreadBrowser(of: FakeBrowserStore.parent, as: presentation)
        let browser = try #require(session.threadBrowser)
        return try Rendered(
            ThreadBrowserView(browser: browser, session: session, now: FakeBrowserStore.now)
                .frame(width: size.width, height: size.height)
                .background(Color(nsColor: Palette.surfaceHigh))
                .ignoresSafeArea(),
            size: size)
    }

    // The ink of rows `ys` between `xs`, in points.
    func ink(_ image: Rendered, xs: ClosedRange<CGFloat>, ys: ClosedRange<CGFloat>)
        -> ClosedRange<CGFloat>?
    {
        let background = image.color(xs.lowerBound, ys.lowerBound)
        let columns = stride(from: xs.lowerBound, through: xs.upperBound, by: 0.5).filter { x in
            stride(from: ys.lowerBound, through: ys.upperBound, by: 0.5).contains { y in
                guard let color = image.color(x, y), let background else {
                    return false
                }
                return abs(color.brightnessComponent - background.brightnessComponent) > 0.15
            }
        }
        guard let first = columns.first, let last = columns.last else {
            return nil
        }
        return first...last
    }

    func drawn(_ token: NSColor) throws -> NSColor? {
        try Rendered(Color(nsColor: token).ignoresSafeArea(), size: NSSize(width: 10, height: 10))
            .color(5, 5)
    }

    // The header's line is its last point; the first label's box is 8...32 below it, its text
    // centered at 68; cards start at 88, 168 and so on, 16 in from each side.
    @Test
    func theListMatchesDiscordsMeasures() throws {
        let image = try rendered()

        let line = try #require(image.firstChange(at: 470, below: 20))
        #expect(abs(CGFloat(line) / image.scale - 47) <= 0.5, "line at \(line)")
        let card = try #require(image.firstChange(at: 240, below: 60))
        #expect(abs(CGFloat(card) / image.scale - 88) <= 0.5, "first card at \(card)")
        let second = try #require(image.firstChange(at: 240, below: 162))
        #expect(abs(CGFloat(second) / image.scale - 168) <= 0.5, "second card at \(second)")
        let label = try #require(ink(image, xs: 10...200, ys: 58...78))
        #expect(abs(label.lowerBound - 16) <= 1.5, "label starts at \(label)")
        let background = image.color(2, 120)
        let left = (0..<Int(40 * image.scale)).first { x in
            !same(image.pixel(x, Int(120 * image.scale)), background)
        }
        #expect(left.map { abs(CGFloat($0) / image.scale - 16) <= 0.5 } == true, "card at \(String(describing: left))")
    }

    @Test
    func theCardsDrawDiscordsColors() throws {
        let image = try rendered()

        #expect(same(image.color(240, 120), try drawn(Palette.surfaceHigher)))
        #expect(same(image.color(2, 120), try drawn(Palette.surfaceHigh)))
    }

    // Icon in a 24 box at 16, 8, "Threads", 16, the divider, 16, the field.
    @Test
    func theHeaderMatchesDiscordsMeasures() throws {
        let image = try rendered()

        let icon = try #require(ink(image, xs: 10...42, ys: 12...36))
        #expect(icon.lowerBound >= 16 && icon.upperBound <= 40, "icon \(icon)")
        let title = try #require(ink(image, xs: 44...120, ys: 14...34))
        #expect(abs(title.lowerBound - 48) <= 1.5, "title \(title)")
        let field = try #require(image.firstChange(at: 300, below: 2))
        #expect(abs(CGFloat(field) / image.scale - 8) <= 1, "field's top at \(field)")
    }

    @Test
    func theModalHasACloseButtonAndThePopoutNone() throws {
        let modal = try rendered(.modal, size: NSSize(width: 680, height: 700))
        let popout = try rendered()

        #expect(ink(modal, xs: 640...676, ys: 12...36) != nil)
        #expect(ink(popout, xs: 440...476, ys: 12...36) == nil)
    }

    // Discord's Create Thread button is its brand blue; Akari has none until it creates threads.
    @Test
    func theEmptyStateHasDiscordsWordsAndNoButton() throws {
        store.empty = true
        let image = try rendered()

        #expect(ThreadBrowserView.emptyTitle == "There are no threads.")
        #expect(
            ThreadBrowserView.emptyText
                == "Stay focused on a conversation with a thread - a temporary text channel.")
        // The symbol, the title and the text, centered below the header.
        let drawn = try #require(ink(image, xs: 0...479, ys: 60...440))
        #expect(abs((drawn.lowerBound + drawn.upperBound) / 2 - 240) <= 8, "\(drawn)")
        let brand = stride(from: CGFloat(56), to: 651, by: 1).contains { y in
            stride(from: CGFloat(0), to: 480, by: 1).contains { x in
                guard let color = image.color(x, y) else {
                    return false
                }
                return color.blueComponent > 0.8 && color.redComponent < 0.5
                    && color.greenComponent < 0.55
            }
        }
        #expect(!brand)
    }
}

@MainActor
struct ThreadBrowserScreenTests {
    let store = FakeBrowserStore()

    // The app's window: a transparent title bar over the full-size content, 1100 × 813.
    func screen(_ presentation: ThreadBrowserModel.Presentation) async throws -> Rendered {
        let session = store.session()
        session.open(channel: FakeBrowserStore.parent)
        session.openThreadBrowser(of: FakeBrowserStore.parent, as: presentation)
        return try Rendered(
            SessionScreen(session: session, splitWidth: SplitWidth()),
            size: NSSize(width: 1100, height: 813))
    }

    func drawn(_ token: NSColor) throws -> NSColor? {
        try Rendered(Color(nsColor: token).ignoresSafeArea(), size: NSSize(width: 10, height: 10))
            .color(5, 5)
    }

    // The header's button is a 32 box 16 from the page's right edge, as the thread's "…" is,
    // 8 below the header's top (the title bar's 32): the popout's right edge is 1084, its top
    // 32 + 40 + 9 = 81.
    @Test
    func thePopoutHangsUnderTheThreadsButtonRightAligned() async throws {
        let image = try await screen(.popout)
        let fill = try drawn(Palette.surfaceHigh)

        let right = (900..<1100).last { same(image.color(CGFloat($0), 200), fill) }
        #expect(right.map { abs(CGFloat($0) + 1 - 1084) <= 1 } == true, "right \(String(describing: right))")
        let top = (60..<120).first { same(image.color(1000, CGFloat($0)), fill) }
        #expect(top.map { abs(CGFloat($0) - 81) <= 1 } == true, "top \(String(describing: top))")
        let left = (500..<700).first { same(image.color(CGFloat($0), 200), fill) }
        #expect(left.map { abs(CGFloat($0) - 604) <= 1 } == true, "left \(String(describing: left))")
    }

    // 680 wide and 701 tall in an 813 window: 56 above and below, the page dimmed around it.
    @Test
    func theModalSitsInTheMiddleOfTheDimmedWindow() async throws {
        let image = try await screen(.modal)
        let fill = try drawn(Palette.surfaceHigh)

        let left = (0..<550).first { same(image.color(CGFloat($0), 400), fill) }
        #expect(left.map { abs(CGFloat($0) - 211) <= 1.5 } == true, "left \(String(describing: left))")
        let top = (0..<200).first { same(image.color(550, CGFloat($0)), fill) }
        #expect(top.map { abs(CGFloat($0) - 57) <= 1.5 } == true, "top \(String(describing: top))")
        let dimmed = try #require(image.color(20, 400))
        #expect(dimmed.brightnessComponent < 0.05, "\(dimmed)")
    }
}

@MainActor
struct ThreadBrowserEventsTests {
    final class Calls {
        var outside = 0
        var escape = 0
    }

    func watcher(_ calls: Calls, inside: [CGRect]) -> (ThreadBrowserEvents.Watcher, NSWindow) {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled],
            backing: .buffered, defer: false)
        let watcher = ThreadBrowserEvents.Watcher(frame: NSRect(x: 0, y: 0, width: 400, height: 300))
        window.contentView = watcher
        watcher.inside = inside
        watcher.onOutside = { calls.outside += 1 }
        watcher.onEscape = { calls.escape += 1 }
        return (watcher, window)
    }

    // `y` from the top, as the watcher's space measures it.
    func click(_ x: CGFloat, _ y: CGFloat, in window: NSWindow) -> NSEvent {
        NSEvent.mouseEvent(
            with: .leftMouseDown, location: NSPoint(x: x, y: 300 - y), modifierFlags: [],
            timestamp: 0, windowNumber: window.windowNumber, context: nil, eventNumber: 0,
            clickCount: 1, pressure: 1)!
    }

    @Test
    func aClickOutsideClosesAndStillReachesItsTarget() {
        let calls = Calls()
        let (watcher, window) = watcher(calls, inside: [CGRect(x: 100, y: 50, width: 200, height: 100)])

        let outside = click(20, 20, in: window)
        #expect(watcher.handle(outside) === outside)
        #expect(calls.outside == 1)

        _ = watcher.handle(click(150, 100, in: window))
        #expect(calls.outside == 1)
    }

    @Test
    func escapeClosesAndGoesNoFurtherButOtherKeysPass() throws {
        let calls = Calls()
        let (watcher, window) = watcher(calls, inside: [])
        func key(_ code: UInt16, _ characters: String) throws -> NSEvent {
            try #require(
                NSEvent.keyEvent(
                    with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                    windowNumber: window.windowNumber, context: nil, characters: characters,
                    charactersIgnoringModifiers: characters, isARepeat: false, keyCode: code))
        }

        #expect(watcher.handle(try key(53, "\u{1b}")) == nil)
        #expect(calls.escape == 1)
        let a = try key(0, "a")
        #expect(watcher.handle(a) === a)
        #expect(calls.escape == 1)
    }
}
