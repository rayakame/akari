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
        let (_, pins) = try await opened()
        let view = host(pins)

        let shown = previews(in: view)
        let contents: [String] = shown.compactMap { $0.cell.message?.content }

        #expect(shown.count == 3)
        #expect(contents == FakeBrowserStore.pinTexts)
        for preview in shown {
            let middle = NSPoint(x: preview.bounds.midX, y: preview.bounds.midY)
            #expect(preview.hitTest(preview.convert(middle, to: preview.superview)) == nil)
        }
    }

    // Jump 8 from the card's top and right, the ✕ after it, each 32 high.
    @Test
    func jumpAndTheCrossShowOnlyOverTheirCardAtItsTopRight() async throws {
        let (_, pins) = try await opened()
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
    }

    @Test
    func aCardWithoutUnpinHasJumpAlone() async throws {
        let (_, pins) = try await opened()
        let row = try #require(pins.rows.first)
        let view = ShownView(
            PinnedCard(row: row, unpins: false, busy: false, hoveredLook: true, jump: { _ in }, unpin: { _ in }),
            size: NSSize(width: PinsLayout.cardWidth, height: PinsLayout.cardHeight(row)))

        #expect(view.hands().count == 1)
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
    }

    // The protip's label in the official client's green, on the footer's band at the bottom.
    @Test
    func theEmptyStateEndsInItsProtip() async throws {
        fake.pins = 0
        let (_, pins) = try await opened()
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
    }

    // Akari: the official client shows a failed first page as an empty channel.
    @Test
    func aFailedFirstPageOffersToTryAgain() async throws {
        fake.pins = 0
        fake.pinsState = .failed
        let (_, pins) = try await opened()
        #expect(pins.phase == .failed)
        let view = host(pins)
        let loads = { self.fake.account?.pinCalls.withLock { $0 }.filter { $0 == "load" }.count ?? 0 }
        // The panel's opening asks too.
        try await waitUntil("opened") { loads() == 2 }
        let retry = try #require(view.hands().first)

        try await click(retry, in: view)

        try await waitUntil("asked again") { loads() == 3 }
    }

    @Test
    func loadMoreAsksForTheNextPageWhileDiscordHasMore() async throws {
        fake.pinsMore = true
        let (_, pins) = try await opened()
        let height = PinsLayout.contentHeight(pins, upTo: 2000) + PinsLayout.headerHeight
        let view = host(pins, height: height)
        let more = try #require(view.hands().first)

        #expect(abs(more.width - (PinsLayout.width - PinsLayout.listLeading - 2 * PinsLayout.moreInset)) <= 4, "\(more)")
        try await click(more, in: view)

        try await waitUntil("the next page") {
            fake.account?.pinCalls.withLock { $0 }.contains("more") == true
        }
    }

    @Test
    func theFirstPageShowsTheSpinner() async throws {
        fake.pins = 0
        fake.pinsState = .loading
        let (_, pins) = try await opened()
        let view = host(pins)

        #expect(pins.phase == .loading)
        #expect(all(view.host).contains { $0 is NSProgressIndicator })
    }
}

// The header's button: its dot while the channel has pins the user hasn't seen.
@MainActor
struct PinsButtonTests {
    func rendered(unread: Bool, theme: NSAppearance.Name = .darkAqua) throws -> Rendered {
        try Rendered(
            PinsButton(selected: false, unread: unread) {}
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
}
