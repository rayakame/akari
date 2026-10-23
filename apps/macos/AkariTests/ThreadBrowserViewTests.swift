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

    // The rows `ys` with ink between `xs`, in points.
    func inkRows(_ image: Rendered, xs: ClosedRange<CGFloat>, ys: ClosedRange<CGFloat>)
        -> ClosedRange<CGFloat>?
    {
        let background = image.color(xs.lowerBound, ys.lowerBound)
        let rows = stride(from: ys.lowerBound, through: ys.upperBound, by: 0.5).filter { y in
            stride(from: xs.lowerBound, through: xs.upperBound, by: 1).contains { x in
                guard let color = image.color(x, y), let background else {
                    return false
                }
                return abs(color.brightnessComponent - background.brightnessComponent) > 0.15
            }
        }
        guard let first = rows.first, let last = rows.last else {
            return nil
        }
        return first...last
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

    // The smallest window (500 high) leaves the popout 400 and the modal's browser 362: the empty
    // state's 400 mustn't push the header out, nor anything taller than the room.
    @Test(arguments: [
        (ThreadBrowserModel.Presentation.popout, NSSize(width: 480, height: 400)),
        (ThreadBrowserModel.Presentation.modal, NSSize(width: 678, height: 362)),
        (ThreadBrowserModel.Presentation.modal, NSSize(width: 678, height: 250)),
    ])
    func theHeaderStaysWhenTheEmptyStateHasLittleRoom(
        presentation: ThreadBrowserModel.Presentation, size: NSSize
    ) throws {
        store.empty = true
        let image = try rendered(presentation, size: size)

        let line = try #require(image.firstChange(at: size.width - 10, below: 20))
        #expect(abs(CGFloat(line) / image.scale - 47) <= 0.5, "line at \(line)")
        let field = try #require(image.firstChange(at: 300, below: 2))
        #expect(abs(CGFloat(field) / image.scale - 8) <= 1, "field's top at \(field)")
        if presentation == .modal {
            #expect(ink(image, xs: (size.width - 48)...(size.width - 4), ys: 12...36) != nil)
        }
        // The symbol and the words, centered in the room below the header as in a tall browser.
        guard size.height >= 300 else {
            return
        }
        let tall = try rendered(presentation, size: NSSize(width: size.width, height: 651))
        let offset = { (image: Rendered, height: CGFloat) throws -> CGFloat in
            let drawn = try #require(
                inkRows(image, xs: 0...(size.width - 1), ys: 49...(height - 1)))
            return (drawn.lowerBound + drawn.upperBound) / 2 - (48 + height) / 2
        }
        let small = try offset(image, size.height)
        let reference = try offset(tall, 651)
        #expect(abs(small - reference) <= 2, "\(small) vs \(reference)")
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

// The browser live in a window, for what changes after it shows.
@MainActor
struct ThreadBrowserLiveTests {
    let store = FakeBrowserStore()

    func settle(until done: () -> Bool = { false }) async {
        let deadline = ContinuousClock.now + .milliseconds(500)
        repeat {
            try? await Task.sleep(for: .milliseconds(10))
        } while !done() && ContinuousClock.now < deadline
    }

    func host(_ session: SessionModel) throws -> (NSWindow, NSHostingView<AnyView>) {
        let browser = try #require(session.threadBrowser)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 480, height: 651),
            styleMask: [.titled, .fullSizeContentView], backing: .buffered, defer: false)
        let host = NSHostingView(
            rootView: AnyView(
                ThreadBrowserView(browser: browser, session: session, now: FakeBrowserStore.now)
                    .frame(width: 480, height: 651)
                    .ignoresSafeArea()))
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        return (window, host)
    }

    func scrollView(in view: NSView) -> NSScrollView? {
        if let scroll = view as? NSScrollView {
            return scroll
        }
        return view.subviews.lazy.compactMap(scrollView(in:)).first
    }

    // How far the list's bottom is below what shows.
    func scroll(_ scroll: NSScrollView, leaving remaining: CGFloat) throws {
        let document = try #require(scroll.documentView)
        let end = document.frame.height - scroll.contentView.bounds.height
        let y = document.isFlipped ? end - remaining : remaining
        scroll.contentView.scroll(to: NSPoint(x: 0, y: y))
        scroll.reflectScrolledClipView(scroll.contentView)
    }

    // Discord loads the next page once the list's bottom comes within 200 of what shows.
    @Test
    func theNextPageLoadsWithin200OfTheListsBottom() async throws {
        store.extra = 21
        store.more = true
        let session = store.session()
        session.openThreadBrowser(of: FakeBrowserStore.parent, as: .popout)
        let (window, host) = try host(session)
        let loads = { store.account?.olderLoads.withLock { $0 } ?? 0 }
        let list = try #require(scrollView(in: host))
        await settle()
        #expect(loads() == 0)

        try scroll(list, leaving: 260)
        await settle()
        #expect(loads() == 0)

        try scroll(list, leaving: 150)
        await settle { loads() > 0 }
        #expect(loads() == 1)
        withExtendedLifetime(window) {}
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
        var closes = 0
        var tabs = 0
    }

    func watcher(_ calls: Calls, modal: Bool = false) -> (ThreadBrowserEvents.Watcher, NSWindow) {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled],
            backing: .buffered, defer: false)
        let watcher = ThreadBrowserEvents.Watcher(frame: NSRect(x: 0, y: 0, width: 400, height: 300))
        window.contentView = watcher
        watcher.modal = modal
        watcher.close = { calls.closes += 1 }
        watcher.onTab = { calls.tabs += 1 }
        return (watcher, window)
    }

    func mouse(_ type: NSEvent.EventType, in window: NSWindow) -> NSEvent {
        NSEvent.mouseEvent(
            with: type, location: NSPoint(x: 20, y: 20), modifierFlags: [], timestamp: 0,
            windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
            pressure: 1)!
    }

    // Discord's popout closes when a press outside it is released; its modal on a left click.
    @Test
    func thePopoutClosesOnTheReleaseOfAnyPressAndTheModalOnALeftClick() {
        let calls = Calls()
        let (popout, window) = watcher(calls)
        popout.mouseDown(with: mouse(.leftMouseDown, in: window))
        #expect(calls.closes == 0)
        popout.mouseUp(with: mouse(.leftMouseUp, in: window))
        #expect(calls.closes == 1)
        popout.rightMouseDown(with: mouse(.rightMouseDown, in: window))
        popout.rightMouseUp(with: mouse(.rightMouseUp, in: window))
        #expect(calls.closes == 2)

        let modalCalls = Calls()
        let (modal, modalWindow) = watcher(modalCalls, modal: true)
        modal.rightMouseDown(with: mouse(.rightMouseDown, in: modalWindow))
        modal.rightMouseUp(with: mouse(.rightMouseUp, in: modalWindow))
        #expect(modalCalls.closes == 0)
        modal.mouseDown(with: mouse(.leftMouseDown, in: modalWindow))
        modal.mouseUp(with: mouse(.leftMouseUp, in: modalWindow))
        #expect(modalCalls.closes == 1)
    }

    // A release with no press on the shield, as after a click that opened the browser.
    @Test
    func aReleaseAloneClosesNothing() {
        let calls = Calls()
        let (popout, window) = watcher(calls)

        popout.mouseUp(with: mouse(.leftMouseUp, in: window))

        #expect(calls.closes == 0)
    }

    @Test
    func theShieldTakesEveryPointInsideItsBounds() {
        let (watcher, _) = watcher(Calls())

        #expect(watcher.hitTest(NSPoint(x: 20, y: 20)) === watcher)
        #expect(watcher.hitTest(NSPoint(x: 420, y: 20)) == nil)
    }

    func key(_ code: UInt16, _ characters: String, in window: NSWindow, _ modifiers: NSEvent.ModifierFlags = [])
        throws -> NSEvent
    {
        try #require(
            NSEvent.keyEvent(
                with: .keyDown, location: .zero, modifierFlags: modifiers, timestamp: 0,
                windowNumber: window.windowNumber, context: nil, characters: characters,
                charactersIgnoringModifiers: characters, isARepeat: false, keyCode: code))
    }

    // A window with the composer focused and the watcher over it, as when the browser opens.
    func composing() -> (ThreadBrowserEvents.Watcher, NSWindow, NSTextView) {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled],
            backing: .buffered, defer: false)
        let content = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 300))
        window.contentView = content
        let composer = NSTextView(frame: NSRect(x: 0, y: 0, width: 400, height: 40))
        content.addSubview(composer)
        window.makeFirstResponder(composer)
        let watcher = ThreadBrowserEvents.Watcher(frame: content.bounds)
        content.addSubview(watcher)
        return (watcher, window, composer)
    }

    @Test
    func escapeWhileTheInputMethodComposesIsTheInputMethods() throws {
        let calls = Calls()
        let (watcher, window, composer) = composing()
        watcher.close = { calls.closes += 1 }
        composer.setMarkedText("か", selectedRange: NSRange(location: 1, length: 0), replacementRange: NSRange(location: NSNotFound, length: 0))
        #expect(composer.hasMarkedText())

        let escape = try key(53, "\u{1b}", in: window)
        #expect(watcher.handle(escape) === escape)
        #expect(calls.closes == 0)
    }

    // Discord's focus lock gives the focus back when nothing else took it; in Akari that's the
    // channel's composer, where Discord's typing goes too.
    @Test
    func closingHandsTheFocusOnWhenNothingElseTookIt() async throws {
        let (watcher, window, _) = composing()
        var handedOn = 0
        watcher.onFocusFree = { handedOn += 1 }
        let field = NSTextField(frame: NSRect(x: 0, y: 200, width: 200, height: 24))
        window.contentView?.addSubview(field)
        window.makeFirstResponder(field)

        field.removeFromSuperview()
        watcher.removeFromSuperview()
        try? await Task.sleep(for: .milliseconds(50))

        #expect(handedOn == 1)
    }

    @Test
    func closingLeavesTheFocusWhereAClickPutIt() async throws {
        let (watcher, window, _) = composing()
        var handedOn = 0
        watcher.onFocusFree = { handedOn += 1 }
        let other = NSTextView(frame: NSRect(x: 0, y: 100, width: 400, height: 40))
        window.contentView?.addSubview(other)

        window.makeFirstResponder(other)
        watcher.removeFromSuperview()
        try? await Task.sleep(for: .milliseconds(50))

        #expect(handedOn == 0)
        #expect(window.firstResponder === other)
    }

    // Keys meant for the modal's field never reach the composer behind it.
    @Test
    func theModalKeepsKeysFromTheComposerBehindIt() throws {
        let (watcher, window, _) = composing()
        watcher.modal = true

        #expect(watcher.handle(try key(0, "a", in: window)) == nil)
        let copy = try key(8, "c", in: window, .command)
        #expect(watcher.handle(copy) === copy)

        let field = NSTextField(frame: NSRect(x: 0, y: 200, width: 200, height: 24))
        window.contentView?.addSubview(field)
        window.makeFirstResponder(field)
        let typed = try key(0, "a", in: window)
        #expect(watcher.handle(typed) === typed)
    }

    @Test
    func escapeClosesAndGoesNoFurtherButOtherKeysPass() throws {
        let calls = Calls()
        let (watcher, window) = watcher(calls)

        #expect(watcher.handle(try key(53, "\u{1b}", in: window)) == nil)
        #expect(calls.closes == 1)
        let a = try key(0, "a", in: window)
        #expect(watcher.handle(a) === a)
        #expect(calls.closes == 1)
    }

    // Tab and Shift+Tab ask for the field and go no further; Cmd+Tab isn't the app's.
    @Test
    func tabAsksForTheFieldAndGoesNoFurther() throws {
        let calls = Calls()
        let (watcher, window) = watcher(calls)

        #expect(watcher.handle(try key(48, "\t", in: window)) == nil)
        #expect(watcher.handle(try key(48, "\t", in: window, .shift)) == nil)
        #expect(calls.tabs == 2)
        let switching = try key(48, "\t", in: window, .command)
        #expect(watcher.handle(switching) === switching)
        #expect(calls.tabs == 2)
    }
}
