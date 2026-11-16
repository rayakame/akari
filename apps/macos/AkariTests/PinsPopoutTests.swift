import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// Discord's stylesheet (canary 671549): 420 wide under the button, as tall as what it shows
// between 200 and 80% of the window; cards with their message 16 inside, shifted 4 left.
@MainActor
struct PinsLayoutTests {
    @Test
    func thePopoutHangsUnderTheButtonWithItsRightEdgeOnTheButtons() {
        let frame = PinsLayout.frame(
            button: CGRect(x: 1052, y: 40, width: 32, height: 32), content: 300,
            area: CGSize(width: 1100, height: 813))

        #expect(frame == CGRect(x: 664, y: 81, width: 420, height: 357))
    }

    @Test
    func itIsAsTallAsWhatItShowsBetween200And80PercentOfTheWindow() {
        #expect(PinsLayout.headerHeight == 57)
        #expect(PinsLayout.height(content: 100, window: 813) == 200)
        #expect(PinsLayout.height(content: 300, window: 813) == 357)
        #expect(PinsLayout.height(content: 2000, window: 813) == 650)
        // CSS: the minimum wins over the maximum.
        #expect(PinsLayout.height(content: 2000, window: 200) == 200)
    }

    // The card's 1 border, 16 inside above and below the message, which starts with the list's 2
    // above a row; 402 wide: 4 left of the card's 406 inside and 24 short of its right.
    @Test
    func aCardHoldsItsMessage16InsideAboveAndBelow() {
        let message = row(1, at: berlinTime(2026, 10, 10, 12, 0), content: "short")
        let preview = MessagePreview.height(of: message.shown, spaced: false, width: 402)

        #expect(PinsLayout.cardWidth == 408)
        #expect(PinsLayout.previewWidth == 402)
        #expect(PinsLayout.cardHeight(message) == 1 + 14 + preview + 16 + 1)
    }

    func pins(_ count: Int, more: Bool = false, state: PinsState = .loaded) throws -> PinsModel {
        let fake = FakeBrowserStore()
        fake.pins = count
        fake.pinsMore = more
        fake.pinsState = state
        let session = fake.session()
        session.open(channel: FakeBrowserStore.parent)
        session.togglePins()
        return try #require(session.pins)
    }

    // The last card's 8 is the room above Load more or the spinner: Load more 8 below the card
    // and 16 above the bottom, the spinner 32 from both.
    @Test
    func loadMoreAndTheNextPagesSpinnerTakeTheirRoomUnderTheCards() throws {
        let more = try pins(2, more: true)
        let cards = PinsLayout.listTop + more.rows.map { PinsLayout.cardHeight($0) + PinsLayout.cardGap }.reduce(0, +)
        #expect(more.offersMore)
        #expect(PinsLayout.contentHeight(more, upTo: 2000) == cards + 32 + 16)

        let loading = try pins(2, more: true, state: .loading)
        #expect(loading.loadingMore)
        #expect(PinsLayout.contentHeight(loading, upTo: 2000) == cards + 32 + 32 + 24)

        #expect(PinsLayout.contentHeight(try pins(2), upTo: 2000) == cards)
    }

    // The first page's spinner 100 under the header and 120 above the bottom; Try Again 16 under
    // the failed state's words and 32 above the bottom.
    @Test
    func theFirstPagesSpinnerAndTheFailedStateTakeTheirRoom() throws {
        #expect(PinsLayout.contentHeight(try pins(0, state: .loading), upTo: 2000) == 252)
        let failed = try pins(0, state: .failed)
        #expect(failed.phase == .failed)
        #expect(
            PinsLayout.contentHeight(failed, upTo: 2000)
                == PinsLayout.emptyHeight(PinsModel.failedText) + 16 + 32 + 32)
    }

    // 8 under the header, the art's 120, 20, and each line of the words 20.
    @Test
    func theEmptyStatesWordsTakeALineEach() {
        #expect(PinsLayout.emptyHeight("a") == 168)
        #expect(PinsLayout.emptyHeight("a\nb") == 188)
    }

    @Test
    func aLongListIsMeasuredOnlyAsFarAsItCanShow() throws {
        let fake = FakeBrowserStore()
        fake.pins = 40
        let session = fake.session()
        session.open(channel: FakeBrowserStore.parent)
        session.togglePins()
        let pins = try #require(session.pins)

        let measured = PinsLayout.contentHeight(pins, upTo: 500)

        #expect(measured > 500)
        #expect(measured < 500 + PinsLayout.cardHeight(try #require(pins.rows.last)) + 200)
        #expect(PinsLayout.frame(of: pins, button: .zero, area: CGSize(width: 1100, height: 813)).height == 650)
    }
}

// The popout's content, hosted as its panel hosts it; its buttons found by their hand.
@MainActor
final class PinsPopoutTests {
    let fake = FakeBrowserStore()
    var shift = false
    private var shown: ShownView?
    let windows = ShownWindows()

    init() {
        fake.lines = true
        fake.pins = 3
    }

    func opened() async throws -> (SessionModel, PinsModel) {
        let session = fake.session()
        session.open(channel: FakeBrowserStore.parent)
        session.togglePins()
        let pins = try #require(session.pins)
        await pins.open()
        return (session, pins)
    }

    func host(_ pins: PinsModel, hovered: MessageId? = nil, height: CGFloat = 600) -> ShownView {
        let size = CGSize(width: PinsLayout.width, height: height)
        let view = ShownView(
            PinsPanel(pins: pins, size: size, hovered: hovered)
                .environment(\.clickModifiers) { [unowned self] in shift ? .shift : [] },
            size: size)
        shown?.close()
        shown = view
        return view
    }

    func previews(in view: ShownView) -> [MessagePreviewView] {
        all(view.host).compactMap { $0 as? MessagePreviewView }
            .sorted { view.host.convert($0.bounds, from: $0).minY < view.host.convert($1.bounds, from: $1).minY }
    }

    func all(_ view: NSView) -> [NSView] {
        view.layoutSubtreeIfNeeded()
        return [view] + view.subviews.flatMap(all)
    }

    func card(_ index: Int, in view: ShownView) throws -> NSRect {
        let preview = try #require(previews(in: view).dropFirst(index).first)
        let frame = view.host.convert(preview.bounds, from: preview)
        return NSRect(
            x: frame.minX - 1 + PinsLayout.messageShift, y: frame.minY - 1 - PinsLayout.messageInset + PinsLayout.rowTop,
            width: PinsLayout.cardWidth, height: frame.height + 2 * PinsLayout.messageInset + 2 - PinsLayout.rowTop)
    }

    // SwiftUI takes a press and its release over a few run loop turns.
    func click(_ rect: NSRect, in view: ShownView) async throws {
        try settleEvents(in: view.window)
        let location = view.host.convert(NSPoint(x: rect.midX, y: rect.midY), to: nil)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            view.window.sendEvent(
                try #require(
                    NSEvent.mouseEvent(
                        with: type, location: location, modifierFlags: [],
                        timestamp: ProcessInfo.processInfo.systemUptime,
                        windowNumber: view.window.windowNumber, context: nil, eventNumber: 0,
                        clickCount: 1, pressure: type == .leftMouseUp ? 0 : 1)))
            try await Task.sleep(for: .milliseconds(30))
        }
    }

    // Each test closes what it opened, as the app does: a list or a spinner left in a window keeps
    // the main thread busy beside every suite after it.
    func close(_ session: SessionModel) {
        shown?.close()
        shown = nil
        session.close()
        windows.expectNoneLeft()
    }

    func waitUntil(
        _ what: String = "", _ condition: () throws -> Bool,
        sourceLocation: SourceLocation = #_sourceLocation
    ) async throws {
        let deadline = ContinuousClock.now + .seconds(5)
        while ContinuousClock.now < deadline {
            shown?.host.layoutSubtreeIfNeeded()
            if try condition() {
                return
            }
            try await Task.sleep(for: .milliseconds(20))
        }
        Issue.record("not within 5 s: \(what)", sourceLocation: sourceLocation)
    }

    @Test
    func theCardsShowThePinnedMessagesNewestPinFirstAndTakeNoPointer() async throws {
        let (session, pins) = try await opened()
        let view = host(pins)

        let shown = previews(in: view)
        let contents: [String] = shown.compactMap { $0.cell.message?.content }

        #expect(shown.count == 3)
        #expect(contents == FakeBrowserStore.pinTexts)
        for preview in shown {
            let middle = NSPoint(x: preview.bounds.midX, y: preview.bounds.midY)
            #expect(preview.hitTest(preview.convert(middle, to: preview.superview)) == nil)
        }
        close(session)
    }

    // Jump 8 from the card's top and right, the ✕ after it, each 32 high.
    @Test
    func jumpAndTheCrossShowOnlyOverTheirCardAtItsTopRight() async throws {
        let (session, pins) = try await opened()
        #expect(host(pins).hands().isEmpty)

        let view = host(pins, hovered: pins.rows[1].id)
        let card = try card(1, in: view)
        let hands = view.hands().sorted { $0.minX < $1.minX }

        #expect(hands.count == 2)
        let jump = try #require(hands.first)
        let cross = try #require(hands.last)
        #expect(hands.allSatisfy { card.contains($0) })
        #expect(abs(cross.maxX - (card.maxX - 1 - 8)) <= 0.5, "\(cross) in \(card)")
        #expect(abs(cross.minY - (card.minY + 1 + 8)) <= 0.5, "\(cross) in \(card)")
        #expect(cross.width == 32 && cross.height == 32)
        #expect(jump.height == 32 && abs(cross.minX - jump.maxX - 8) <= 0.5)
        close(session)
    }

    @Test
    func aCardWithoutUnpinHasJumpAlone() async throws {
        let (session, pins) = try await opened()
        let row = try #require(pins.rows.first)
        let view = ShownView(
            PinnedCard(row: row, unpins: false, busy: false, hoveredLook: true, jump: { _ in }, unpin: { _ in }),
            size: NSSize(width: PinsLayout.cardWidth, height: PinsLayout.cardHeight(row)))

        #expect(view.hands().count == 1)
        view.close()
        close(session)
    }

    @Test(arguments: [false, true])
    func jumpGoesToTheMessageAndClosesThePopoutUnlessShiftIsHeld(shift: Bool) async throws {
        let (session, pins) = try await opened()
        let target = pins.rows[1].message.id
        self.shift = shift
        let view = host(pins, hovered: pins.rows[1].id)
        let jump = try #require(view.hands().min { $0.minX < $1.minX })

        try await click(jump, in: view)

        try await waitUntil("the jump") { session.messages?.messageJump?.messageId == target }
        #expect((session.pins != nil) == shift)
        close(session)
    }

    @Test(arguments: [false, true])
    func theCrossAsksBeforeItUnpinsUnlessShiftIsHeld(shift: Bool) async throws {
        let (session, pins) = try await opened()
        let target = pins.rows[0].message.id
        self.shift = shift
        let view = host(pins, hovered: pins.rows[0].id)
        let cross = try #require(view.hands().max { $0.minX < $1.minX })

        try await click(cross, in: view)

        if shift {
            try await waitUntil("the unpin") {
                fake.account?.pinCalls.withLock { $0 }.contains("unpin \(target.rawValue)") == true
            }
            #expect(session.pinQuestion == nil)
        } else {
            try await waitUntil("the question") { session.pinQuestion != nil }
            #expect(session.pinQuestion?.row.message.id == target)
            #expect(session.pinQuestion?.pins == false)
            #expect(fake.account?.pinCalls.withLock { $0 }.contains("unpin \(target.rawValue)") == false)
        }
        #expect(session.pins != nil)
        close(session)
    }

    // The protip's label in the official client's green, on the footer's band at the bottom.
    @Test
    func theEmptyStateEndsInItsProtip() async throws {
        fake.pins = 0
        let (session, pins) = try await opened()
        #expect(pins.phase == .empty)
        let height = PinsLayout.height(content: PinsLayout.contentHeight(pins, upTo: 800), window: 1000)
        let size = NSSize(width: PinsLayout.width, height: height)

        let image = try Rendered(
            PinsPanel(pins: pins, size: size).frame(width: size.width, height: size.height).ignoresSafeArea(),
            size: size)

        let footer = PinsLayout.footerHeight(pins)
        let band = try #require(image.color(4, height - 4))
        let surface = try #require(image.color(4, height - footer - 4))
        #expect(band.brightnessComponent > surface.brightnessComponent)
        // The measure is the band's: it starts where the panel's height leaves room for it.
        #expect(same(image.color(4, height - footer + 1), band))
        #expect(same(image.color(4, height - footer - 1), surface))
        var greens = 0
        for x in stride(from: CGFloat(16), to: PinsLayout.width - 16, by: 0.5) {
            for y in stride(from: height - footer + 16, to: height - 16, by: 0.5) {
                if let color = image.color(x, y), color.greenComponent > 0.5, color.redComponent < 0.4,
                    color.blueComponent < 0.5
                {
                    greens += 1
                }
            }
        }
        #expect(greens > 0, "Protip: in green")
        #expect(previews(in: host(pins)).isEmpty)
        close(session)
    }

    // Each line of the words centered: the second, shorter one too.
    @Test
    func theEmptyStatesWordsAreCentered() async throws {
        fake.pins = 0
        let (session, pins) = try await opened()
        let size = NSSize(width: PinsLayout.width, height: 400)

        let image = try Rendered(
            PinsPanel(pins: pins, size: size).frame(width: size.width, height: size.height).ignoresSafeArea(),
            size: size)

        let surface = try #require(image.color(4, 300)).brightnessComponent
        let line = PinsLayout.headerHeight + PinsLayout.emptyTop + PinsLayout.emptyArt + PinsLayout.emptyTextGap
            + TypeScale.pinsEmpty.line
        var ink: [CGFloat] = []
        for y in stride(from: line + 4, to: line + TypeScale.pinsEmpty.line - 4, by: 0.5) {
            for x in stride(from: CGFloat(0), to: size.width, by: 0.5)
            where abs((image.color(x, y)?.brightnessComponent ?? surface) - surface) > 0.2 {
                ink.append(x)
            }
        }
        let left = try #require(ink.min())
        let right = try #require(ink.max())
        #expect(abs((left + right) / 2 - size.width / 2) <= 2, "\(left)…\(right)")
        close(session)
    }

    // Akari: the official client shows a failed first page as an empty channel.
    @Test
    func aFailedFirstPageOffersToTryAgain() async throws {
        fake.pins = 0
        fake.pinsState = .failed
        let (session, pins) = try await opened()
        #expect(pins.phase == .failed)
        let view = host(pins)
        let loads = { self.fake.account?.pinCalls.withLock { $0 }.filter { $0 == "load" }.count ?? 0 }
        // The panel's opening asks too.
        try await waitUntil("opened") { loads() == 2 }
        let retry = try #require(view.hands().first)

        try await click(retry, in: view)

        try await waitUntil("asked again") { loads() == 3 }
        close(session)
    }

    var mores: Int {
        fake.account?.pinCalls.withLock { $0 }.filter { $0 == "more" }.count ?? 0
    }

    // The short list ends in view, so its end asks first; a page that brings nothing spends it, and
    // then only the button asks.
    @Test
    func loadMoreAsksForTheNextPageWhileDiscordHasMore() async throws {
        fake.pinsMore = true
        let (session, pins) = try await opened()
        let height = PinsLayout.contentHeight(pins, upTo: 2000) + PinsLayout.headerHeight
        let view = host(pins, height: height)
        try await waitUntil("the end's page") { mores == 1 && pins.offersMore }
        let more = try #require(view.hands().first)

        #expect(abs(more.width - (PinsLayout.width - PinsLayout.listLeading - 2 * PinsLayout.moreInset)) <= 4, "\(more)")
        try await click(more, in: view)

        try await waitUntil("the button's page") { mores == 2 }
        close(session)
    }

    // Discord asks for the next page within 250 of the list's end.
    @Test
    func scrollingNearTheEndAsksForTheNextPage() async throws {
        fake.pins = 40
        fake.pinsMore = true
        let (session, pins) = try await opened()
        let view = host(pins)
        let scroll = try #require(all(view.host).compactMap { $0 as? NSScrollView }.first)
        let document = try #require(scroll.documentView)
        #expect(document.frame.height > 2000)

        let clip = scroll.contentView
        let above = document.frame.height - clip.bounds.height - 200
        clip.scroll(to: NSPoint(x: 0, y: document.isFlipped ? above : 200))
        scroll.reflectScrolledClipView(clip)

        try await waitUntil("the next page") { mores == 1 }
        close(session)
    }

    @Test
    func aPinTheCoreDoesntLetTheUserUnpinHasJumpAlone() async throws {
        fake.unpinnable = false
        let (session, pins) = try await opened()

        let view = host(pins, hovered: pins.rows[0].id)

        #expect(view.hands().count == 1)
        close(session)
    }

    // While its unpin is on its way the ✕ is a spinner without the hand; Jump keeps its own.
    @Test
    func theCrossIsASpinnerWhileItsUnpinIsOnItsWay() async throws {
        fake.holdsUnpins = true
        let (session, pins) = try await opened()
        shift = true
        let view = host(pins, hovered: pins.rows[0].id)
        let cross = try #require(view.hands().max { $0.minX < $1.minX })

        try await click(cross, in: view)

        try await waitUntil("busy") { pins.unpinning.contains(pins.rows[0].message.id) }
        var spinner: NSView?
        try await waitUntil("the spinner") {
            spinner = all(view.host).first { $0 is NSProgressIndicator }
            return spinner != nil
        }
        let place = view.host.convert(try #require(spinner).bounds, from: spinner)
        #expect(cross.contains(NSPoint(x: place.midX, y: place.midY)), "\(place) in \(cross)")
        #expect(view.hands().count == 1)
        close(session)
    }

    @Test
    func theNextPagesSpinnerShowsUnderTheCards() async throws {
        fake.pinsMore = true
        fake.pinsState = .loading
        let (session, pins) = try await opened()
        #expect(pins.loadingMore)
        let view = host(pins, height: PinsLayout.contentHeight(pins, upTo: 2000) + PinsLayout.headerHeight)

        let last = try card(2, in: view)
        let spinner = try #require(all(view.host).first { $0 is NSProgressIndicator })

        #expect(view.host.convert(spinner.bounds, from: spinner).minY >= last.maxY)
        close(session)
    }

    // 8 under the header's 57, and 8 apart.
    @Test
    func theCardsStartUnderTheHeaderAndStand8Apart() async throws {
        let (session, pins) = try await opened()
        let view = host(pins)

        let first = try card(0, in: view)
        let second = try card(1, in: view)

        #expect(abs(first.minY - (PinsLayout.headerHeight + PinsLayout.listTop)) <= 0.5, "\(first)")
        #expect(abs(second.minY - first.maxY - PinsLayout.cardGap) <= 0.5, "\(first) \(second)")
        close(session)
    }

    // A pinned reply's card has its line, and its images come from the session's cache.
    @Test
    func aCardShowsItsReplyLineWithTheSessionsImages() async throws {
        fake.pinnedReply = true
        let (session, pins) = try await opened()
        let size = CGSize(width: PinsLayout.width, height: 600)
        let view = ShownView(PinsPanel(pins: pins, size: size).imageEnvironment(session), size: size)

        let preview = try #require(previews(in: view).first)

        #expect(!preview.cell.replyLine.isHidden)
        #expect(preview.cell.images === session.images)
        view.close()
        close(session)
    }

    // The panel isn't key yet when its first click comes.
    @Test(.enabled(if: firstClicks))
    func jumpTakesTheFirstClickOfAPanelThatIsntKey() async throws {
        let (session, pins) = try await opened()
        let target = pins.rows[1].message.id
        let size = NSSize(width: PinsLayout.width, height: 600)
        let panel = NSHostingView(
            rootView: AnyView(
                PinsPanel(pins: pins, size: size, hovered: pins.rows[1].id)
                    .frame(width: size.width, height: size.height)))
        let window = TestWindow.make(size: size)
        window.contentView = panel
        window.orderFrontRegardless()
        panel.layoutSubtreeIfNeeded()
        func areas(_ view: NSView) -> [CursorArea.Area] {
            ((view as? CursorArea.Area).map { [$0] } ?? []) + view.subviews.flatMap(areas)
        }
        let jump = try #require(
            areas(panel).filter { $0.cursor == .pointingHand }.map { panel.convert($0.bounds, from: $0) }
                .min { $0.minX < $1.minX })

        try settleEvents(in: window)
        let location = panel.convert(NSPoint(x: jump.midX, y: jump.midY), to: nil)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            window.sendEvent(
                try #require(
                    NSEvent.mouseEvent(
                        with: type, location: location, modifierFlags: [],
                        timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                        context: nil, eventNumber: 0, clickCount: 1, pressure: type == .leftMouseUp ? 0 : 1)))
            try await Task.sleep(for: .milliseconds(30))
        }

        try await waitUntil("the jump") { session.messages?.messageJump?.messageId == target }
        panel.rootView = AnyView(EmptyView())
        window.orderOut(nil)
        close(session)
    }

    @Test
    func theFirstPageShowsTheSpinner() async throws {
        fake.pins = 0
        fake.pinsState = .loading
        let (session, pins) = try await opened()
        let view = host(pins)

        #expect(pins.phase == .loading)
        #expect(all(view.host).contains { $0 is NSProgressIndicator })
        close(session)
    }
}

// The header's button: its dot while the channel has pins the user hasn't seen, dimmed and without
// the hand behind the age gate, no tooltip while its popout is open.
@MainActor
struct PinsButtonTests {
    let windows = ShownWindows()

    func rendered(
        unread: Bool, selected: Bool = false, disabled: Bool = false, theme: NSAppearance.Name = .darkAqua
    ) throws -> Rendered {
        try Rendered(
            PinsButton(selected: selected, unread: unread, disabled: disabled) {}
                .frame(width: 48, height: 48)
                .background(Color(nsColor: Palette.chat))
                .ignoresSafeArea(),
            appearance: theme, size: NSSize(width: 48, height: 48))
    }

    func reddish(_ color: NSColor?) -> Bool {
        guard let color else {
            return false
        }
        return color.redComponent > 0.75 && color.redComponent - color.greenComponent > 0.35
            && color.redComponent - color.blueComponent > 0.35
    }

    // The 32 box at 8, 8; the pin's 20 in its middle, from 14 to 34; the dot and its ring end 3
    // past the pin's corner.
    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theDotShowsAtThePinsBottomRightOnlyWhileUnread(theme: NSAppearance.Name) throws {
        let dot = try rendered(unread: true, theme: theme)
        let plain = try rendered(unread: false, theme: theme)
        let middle = 34 + PinsButton.dotRing + 1 - PinsButton.dotRing - PinsButton.dot / 2

        #expect(reddish(dot.color(middle, middle)))
        #expect(!reddish(plain.color(middle, middle)))
        // The ring: the header's color between the dot and the pin.
        let ring = middle - PinsButton.dot / 2 - PinsButton.dotRing / 2
        #expect(same(dot.color(ring, middle), plain.color(4, 4)))
    }

    // The most any pixel of the pin's 20 box stands out from the header's color.
    func ink(_ image: Rendered) -> CGFloat {
        let header = image.color(4, 4)?.brightnessComponent ?? 0
        var most: CGFloat = 0
        for x in stride(from: CGFloat(14), to: 34, by: 0.5) {
            for y in stride(from: CGFloat(14), to: 34, by: 0.5) {
                most = max(most, abs((image.color(x, y)?.brightnessComponent ?? header) - header))
            }
        }
        return most
    }

    // Dimmed by its disabled state alone: an opacity over it would leave a quarter.
    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func behindTheAgeGateItIsDimmedToAboutHalfWithoutTheHand(theme: NSAppearance.Name) throws {
        let enabled = try rendered(unread: false, theme: theme)
        let disabled = try rendered(unread: false, disabled: true, theme: theme)

        let share = ink(disabled) / ink(enabled)
        #expect(share > 0.4 && share < 0.7, "\(share)")
        let size = NSSize(width: 48, height: 48)
        let shown = ShownView(PinsButton(selected: false, unread: false) {}, size: size)
        let gated = ShownView(PinsButton(selected: false, unread: false, disabled: true) {}, size: size)
        #expect(shown.hands().count == 1)
        #expect(gated.hands().isEmpty)
        shown.close()
        gated.close()
        windows.expectNoneLeft()
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func whileThePopoutIsOpenItIsSelected(theme: NSAppearance.Name) throws {
        let rest = try rendered(unread: false, theme: theme)
        let selected = try rendered(unread: false, selected: true, theme: theme)

        #expect(ink(selected) > ink(rest) + 0.1, "\(ink(selected)) against \(ink(rest))")
    }

    // The ring is the header's color between the dot and the pin, also where it covers the pin.
    @Test
    func theDotsRingCoversThePin() throws {
        let plain = try rendered(unread: false)
        let dot = try rendered(unread: true)
        let header = plain.color(4, 4)
        let middle = 34 + PinsButton.dotRing + 1 - PinsButton.dotRing - PinsButton.dot / 2
        var covered = 0
        for x in stride(from: middle - 6, through: middle + 6, by: 0.5) {
            for y in stride(from: middle - 6, through: middle + 6, by: 0.5) {
                let distance = hypot(x - middle, y - middle)
                guard distance > PinsButton.dot / 2 + 0.5, distance < PinsButton.dot / 2 + PinsButton.dotRing - 0.5,
                    let pin = plain.color(x, y), let head = header,
                    abs(pin.brightnessComponent - head.brightnessComponent) > 0.15
                else {
                    continue
                }
                covered += 1
                let ring = dot.color(x, y)?.brightnessComponent ?? 1
                #expect(abs(ring - head.brightnessComponent) < 0.05, "\(x), \(y): \(ring)")
            }
        }
        #expect(covered > 0, "the ring lies over the pin")
    }

    // Hover lights an enabled button, not one behind the age gate.
    @Test
    func hoverLightsTheButtonOnlyWhereItCanBeUsed() async throws {
        let size = NSSize(width: 48, height: 48)
        func drawn(_ view: ShownView) throws -> NSBitmapImageRep {
            let rep = try #require(view.host.bitmapImageRepForCachingDisplay(in: view.host.bounds))
            view.host.cacheDisplay(in: view.host.bounds, to: rep)
            return rep
        }
        func brightest(_ rep: NSBitmapImageRep) -> CGFloat {
            let scale = CGFloat(rep.pixelsWide) / size.width
            var most: CGFloat = 0
            for x in Int(14 * scale)..<Int(34 * scale) {
                for y in Int(14 * scale)..<Int(34 * scale) {
                    most = max(most, rep.colorAt(x: x, y: y)?.usingColorSpace(.sRGB)?.brightnessComponent ?? 0)
                }
            }
            return most
        }
        var looks: [Bool: (rest: CGFloat, hovered: CGFloat)] = [:]
        for disabled in [false, true] {
            let view = ShownView(
                PinsButton(selected: false, unread: false, disabled: disabled) {}
                    .frame(width: 48, height: 48)
                    .background(Color(nsColor: Palette.chat)),
                size: size)
            view.window.appearance = NSAppearance(named: .darkAqua)
            let rest = brightest(try drawn(view))
            try view.hover(at: NSPoint(x: 24, y: 24))
            var hovered = rest
            let deadline = ContinuousClock.now + .seconds(2)
            repeat {
                try await Task.sleep(for: .milliseconds(20))
                hovered = brightest(try drawn(view))
            } while !disabled && hovered <= rest + 0.05 && ContinuousClock.now < deadline
            looks[disabled] = (rest, hovered)
            view.close()
        }

        let enabled = try #require(looks[false])
        let gated = try #require(looks[true])
        #expect(enabled.hovered > enabled.rest + 0.05, "\(looks)")
        #expect(abs(gated.hovered - gated.rest) < 0.02, "\(looks)")
        windows.expectNoneLeft()
    }

    @Test
    func itsTooltipGoesWhileThePopoutIsOpen() {
        #expect(PinsButton.tooltip(selected: false) == "Pinned Messages")
        #expect(PinsButton.tooltip(selected: true) == "")
    }
}
