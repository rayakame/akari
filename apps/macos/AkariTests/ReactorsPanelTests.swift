import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// The reactions window: where it stands, and what its panel shows.
@MainActor
struct ReactorsPanelTests {
    let account = ReactionsAccount(
        [reaction("👍", 2, me: true), reaction("🎉", 3)],
        people: [
            "🎉": [person(2, "Ray", "ray"), person(3, "Mira Belle", "mira"), person(4, "Kim", "kim")],
            "👍": [person(1, "Chris", "chris"), person(2, "Ray", "ray")],
        ])

    @Test
    func theWindowIsCenteredAndNeverLargerThanTheSessionsWindowAllows() {
        #expect(
            ReactorsLayout.frame(in: CGSize(width: 1200, height: 800))
                == CGRect(x: 300, y: 160, width: 600, height: 480))
        #expect(
            ReactorsLayout.frame(in: CGSize(width: 500, height: 400))
                == CGRect(x: 40, y: 40, width: 420, height: 320))
    }

    @Test
    func theWindowOpensOnTheEmojiWithTheMostReactionsAndLoadsItsUsers() async throws {
        let session = account.window()
        let reactors = try #require(session.reactors)

        #expect(reactors.tabs.map(\.name) == [":tada:", ":thumbsup:"])
        await reactors.open()

        #expect(reactors.reactors.map(\.user.displayName) == ["Ray", "Mira Belle", "Kim"])
        #expect(reactors.reactors.map(\.removable) == [false, false, false])
    }

    @Test
    func thePanelDrawsItsTabsItsDividerAndItsRows() async throws {
        let session = account.window()
        let reactors = try #require(session.reactors)
        await reactors.open()
        let size = NSSize(width: 600, height: 480)

        let image = try Rendered(
            ReactorsPanel(reactors: reactors, size: size, close: {})
                .frame(width: size.width, height: size.height)
                .ignoresSafeArea(),
            size: size)

        let surface = try #require(image.color(300, 30))
        // The chosen tab is filled: at its left edge, in the first tab's line under the header.
        let chosen = try #require(image.color(27, 81))
        let other = try #require(image.color(27, 121))
        #expect(abs(chosen.brightnessComponent - surface.brightnessComponent) > 0.02)
        #expect(abs(other.brightnessComponent - surface.brightnessComponent) < 0.01)
        // The divider: the border's 1, 24 in, the column's 98 and its 16.
        let divider = try #require(image.color(139.5, 200))
        #expect(abs(divider.brightnessComponent - surface.brightnessComponent) > 0.02)
        let beside = try #require(image.color(143, 200))
        #expect(beside == surface)
    }

    // Where the ink differs from the panel's surface: rows (or columns) of points in a box.
    func inked(
        _ image: Rendered, x: ClosedRange<CGFloat>, y: ClosedRange<CGFloat>, surface: NSColor,
        by: CGFloat = 0.25, columns: Bool = false
    ) -> [CGFloat] {
        let outer = columns ? x : y
        let inner = columns ? y : x
        return stride(from: outer.lowerBound, to: outer.upperBound, by: 0.5).filter { a in
            stride(from: inner.lowerBound, to: inner.upperBound, by: 0.5).contains { b in
                let pixel = columns ? image.color(a, b) : image.color(b, a)
                return abs((pixel?.brightnessComponent ?? 0) - surface.brightnessComponent) > by
            }
        }
    }

    // Discord's sheet: tabs 32 high and 8 apart, rows of 52, the avatar's 32 and 10 before the
    // name, the display name over the username, the count of both kinds.
    @Test
    func thePanelHasDiscordsMeasures() async throws {
        let account = ReactionsAccount(
            [Reaction(
                emoji: ReactionEmoji(id: nil, name: "🎉", animated: false, key: nil), normal: 2, burst: 10,
                me: false, meBurst: false),
             reaction("👍", 2)],
            people: ["🎉": [person(3, "Mira Belle", "mira"), person(4, "Kim", "kim")], "👍": [person(2, "Ray", "ray")]])
        let session = account.window()
        let reactors = try #require(session.reactors)
        await reactors.open()
        let size = NSSize(width: 600, height: 480)
        let render = {
            try Rendered(
                ReactorsPanel(reactors: reactors, size: size, close: {})
                    .frame(width: size.width, height: size.height)
                    .ignoresSafeArea(),
                size: size)
        }
        let image = try render()
        let surface = try #require(image.color(300, 30))
        let swatch = try Rendered(
            Color(nsColor: Palette.surfaceHigh).frame(width: 20, height: 20), size: NSSize(width: 20, height: 20))
        #expect(same(surface, swatch.color(10, 10)))

        // Right of the count, away from the rounded corners.
        let chosen = inked(image, x: 100...101, y: 62...160, surface: surface, by: 0.02)
        let first = try #require(chosen.first)
        let last = try #require(chosen.last)
        #expect(abs(first - 65) <= 0.5, "\(chosen)")
        #expect(abs(last + 0.5 - first - 32) <= 0.5, "\(chosen)")
        let count = inked(image, x: 60...120, y: 70...92, surface: surface, columns: true)
        let digits = (count.last ?? 0) - (count.first ?? 0)
        #expect(digits > 12, "two digits: \(count)")

        // The users' column starts at 140.
        let names = inked(image, x: 198...330, y: 66...170, surface: surface)
        let lines = names.reduce(into: [[CGFloat]]()) { lines, y in
            if let last = lines.last?.last, y - last <= 2 {
                lines[lines.count - 1].append(y)
            } else {
                lines.append([y])
            }
        }
        try #require(lines.count >= 3, "\(lines)")
        #expect(abs(lines[2][0] - lines[0][0] - 52) <= 0.5, "\(lines.map { [$0.first ?? 0, $0.last ?? 0] })")
        // After the avatar: 16 in, 32 wide, 10 before the name.
        let avatarEnd: CGFloat = 140 + 16 + 32
        let left = inked(image, x: 150...260, y: lines[0][0]...lines[0][0] + 8, surface: surface, columns: true)
            .filter { $0 > avatarEnd + 2 }
        let nameStart = try #require(left.first)
        #expect(abs(nameStart - (avatarEnd + 10)) <= 1.5, "\(left)")
        let wide = { (line: [CGFloat]) -> CGFloat in
            let columns = self.inked(
                image, x: 198...400, y: line[0]...line[line.count - 1], surface: surface, columns: true)
            return columns.last ?? 0
        }
        let top = wide(lines[0])
        let bottom = wide(lines[1])
        #expect(top > bottom + 20, "Mira Belle over mira: \(top) \(bottom)")

        await reactors.select(ReactionPill.ID(.unicode("👍")))
        let other = inked(try render(), x: 100...101, y: 62...160, surface: surface, by: 0.02)
        let second = try #require(other.first)
        #expect(abs(second - (65 + 32 + 8)) <= 0.5, "\(other)")
    }

    @Test
    func aRowsRemovalAsksTheAccountAndTheRowGoes() async throws {
        let account = ReactionsAccount(
            [reaction("👍", 2)], people: ["👍": [person(2, "Ray", "ray"), person(3, "Mira", "mira")]],
            moderates: true)
        let session = account.window()
        let reactors = try #require(session.reactors)
        await reactors.open()
        #expect(reactors.reactors.map(\.removable) == [true, true])

        await reactors.remove(reactors.reactors[0])

        #expect(account.removed.withLock { $0 } == [UserId(rawValue: 2)])
        #expect(reactors.reactors.map(\.user.displayName) == ["Mira"])
    }

    // With Full Keyboard Access the ✕ takes the focus without the pointer near it.
    @Test
    func theRemovalShowsUnderThePointerAndWithTheKeyboardsFocus() {
        #expect(!ReactorRow.showsRemove(hovered: false, focused: false))
        #expect(ReactorRow.showsRemove(hovered: true, focused: false))
        #expect(ReactorRow.showsRemove(hovered: false, focused: true))
    }

    @Test
    func theWindowClosesWhenTheSessionSaysSo() throws {
        let session = account.window()
        #expect(session.reactors != nil)

        session.closeReactors()

        #expect(session.reactors == nil)
    }
}

// A SwiftUI view in a window that says it's key, ordered in out of sight, as the user's would
// be: clicks reach its buttons as the window server's would (not under a full-size content
// view's title bar).
@MainActor
final class ShownView {
    let window: ThreadBrowserWindowsTests.KeyWindow
    let host: NSHostingView<AnyView>

    init(_ view: some View, size: NSSize) {
        window = TestWindow.hide(
            ThreadBrowserWindowsTests.KeyWindow(
                contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled],
                backing: .buffered, defer: false))
        window.isReleasedWhenClosed = false
        window.animationBehavior = .none
        host = NSHostingView(rootView: AnyView(view.frame(width: size.width, height: size.height)))
        window.contentView = host
        window.orderFrontRegardless()
        host.layoutSubtreeIfNeeded()
    }

    // A test's instance can go away off the main thread.
    deinit {
        nonisolated(unsafe) let window = window
        DispatchQueue.main.async {
            MainActor.assumeIsolated {
                for child in window.childWindows ?? [] {
                    child.orderOut(nil)
                }
                window.orderOut(nil)
            }
        }
    }

    /// Takes the view out of its window and the window and its children off the screen at once.
    func close() {
        Releases.watch(window)
        host.rootView = AnyView(EmptyView())
        host.layoutSubtreeIfNeeded()
        for child in window.childWindows ?? [] {
            child.orderOut(nil)
        }
        window.orderOut(nil)
    }

    /// The places that show the hand, from the view's top left.
    func hands(_ cursor: NSCursor = .pointingHand) -> [NSRect] {
        func areas(_ view: NSView) -> [CursorArea.Area] {
            ((view as? CursorArea.Area).map { [$0] } ?? []) + view.subviews.flatMap(areas)
        }
        host.layoutSubtreeIfNeeded()
        return areas(host).filter { $0.cursor == cursor }.map { host.convert($0.bounds, from: $0) }
    }

    // No window server tells the views where the pointer is: the tracking areas' owners hear it.
    func hover(at point: NSPoint) throws {
        let location = host.convert(point, to: nil)
        func all(_ view: NSView) -> [NSView] { [view] + view.subviews.flatMap(all) }
        for area in all(host).flatMap(\.trackingAreas) {
            guard let owner = area.owner as? NSResponder else {
                continue
            }
            let entered = NSEvent.enterExitEvent(
                with: .mouseEntered, location: location, modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                context: nil, eventNumber: 0,
                trackingNumber: Int(bitPattern: Unmanaged.passUnretained(area).toOpaque()), userData: nil)
            owner.mouseEntered(with: try #require(entered))
            owner.mouseMoved(
                with: try #require(
                    NSEvent.mouseEvent(
                        with: .mouseMoved, location: location, modifierFlags: [],
                        timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                        context: nil, eventNumber: 0, clickCount: 0, pressure: 0)))
        }
    }

    // SwiftUI takes a press and its release over a few run loop turns.
    func click(at point: NSPoint) async throws {
        try settleEvents(in: window)
        let location = host.convert(point, to: nil)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            window.sendEvent(
                try #require(
                    NSEvent.mouseEvent(
                        with: type, location: location, modifierFlags: [],
                        timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                        context: nil, eventNumber: 0, clickCount: 1, pressure: type == .leftMouseUp ? 0 : 1)))
            try await Task.sleep(for: .milliseconds(30))
        }
    }

    // SwiftUI applies a change over a few run loop turns; at most 5 s.
    func waitUntil(
        _ what: String = "", _ condition: () throws -> Bool,
        sourceLocation: SourceLocation = #_sourceLocation
    ) async throws {
        let deadline = ContinuousClock.now + .seconds(5)
        while ContinuousClock.now < deadline {
            host.layoutSubtreeIfNeeded()
            if try condition() {
                return
            }
            try await Task.sleep(for: .milliseconds(20))
        }
        Issue.record("not within 5 s: \(what)", sourceLocation: sourceLocation)
    }
}

// The reactions window's clicks: its tabs, a row's ✕, Retry, and the next page.
@MainActor
struct ReactorsPanelClickTests {
    static let size = NSSize(width: 600, height: 480)
    // The users' column starts after the tabs' 24, 98 and 16 and the divider.
    static let users: CGFloat = 1 + 24 + 98 + 16 + 1

    func shown(_ account: ReactionsAccount, open: Bool = true) async throws -> (ReactorsModel, ShownView) {
        let session = account.window()
        let reactors = try #require(session.reactors)
        if open {
            await reactors.open()
        }
        let view = ShownView(ReactorsPanel(reactors: reactors, size: Self.size, close: {}), size: Self.size)
        return (reactors, view)
    }

    func center(_ rect: NSRect) -> NSPoint { NSPoint(x: rect.midX, y: rect.midY) }

    @Test
    func theWindowLoadsItsFirstUsersWhenItShows() async throws {
        let account = ReactionsAccount(
            [reaction("👍", 2)], people: ["👍": [person(2, "Ray", "ray"), person(3, "Mira", "mira")]])

        let (reactors, view) = try await shown(account, open: false)

        try await view.waitUntil("the first page") { reactors.reactors.count == 2 }
    }

    @Test
    func aClickOnATabChoosesItsEmoji() async throws {
        let account = ReactionsAccount(
            [reaction("👍", 2), reaction("🎉", 1)],
            people: ["👍": [person(2, "Ray", "ray")], "🎉": [person(3, "Mira", "mira")]])
        let (reactors, view) = try await shown(account)
        let tabs = view.hands().filter { $0.maxX <= Self.users }.sorted { $0.minY < $1.minY }
        try #require(tabs.count == 2)

        try await view.click(at: center(tabs[1]))

        try await view.waitUntil("the second tab") { reactors.selected == ReactionPill.ID(.unicode("🎉")) }
        try await view.waitUntil("its users") { reactors.reactors.map(\.user.displayName) == ["Mira"] }
    }

    @Test
    func onlyTheUsersOwnRowHasItsXAndItRemovesTheReaction() async throws {
        let account = ReactionsAccount(
            [reaction("👍", 3, me: true)],
            people: ["👍": [person(1, "Chris", "chris"), person(2, "Ray", "ray"), person(3, "Mira", "mira")]])
        let (reactors, view) = try await shown(account)
        let xs = view.hands().filter { $0.minX >= Self.users && $0.minY >= ReactorsLayout.headerHeight }
        #expect(xs.count == 1, "\(xs)")
        let x = try #require(xs.first)
        #expect(x.midY < 64 + 52, "in the first row")

        // The ✕ shows, and takes a click, once SwiftUI has drawn it under the pointer.
        try view.hover(at: center(x))
        try await Task.sleep(for: .milliseconds(100))
        try await view.click(at: center(x))

        try await view.waitUntil("the removal") { account.removed.withLock { $0 } == [UserId(rawValue: 1)] }
        #expect(reactors.reactors.map(\.user.displayName) == ["Ray", "Mira"])
    }

    @Test
    func aPageThatDidntComeSaysSoAndRetryAsksAgain() async throws {
        let account = ReactionsAccount([reaction("👍", 1)], people: ["👍": [person(2, "Ray", "ray")]])
        account.fails.withLock { $0 = true }
        let (reactors, view) = try await shown(account)
        try await view.waitUntil("the failure") { reactors.failed && !reactors.loading }
        let retry = try #require(
            view.hands().first { $0.minX >= Self.users && $0.minY >= ReactorsLayout.headerHeight })
        account.fails.withLock { $0 = false }

        try await view.click(at: center(retry))

        try await view.waitUntil("the page asked again") { reactors.reactors.count == 1 }
        #expect(!reactors.failed)
    }

    // The spinner's row is on screen under the first page: it asks for the next.
    @Test
    func theSpinnerOnScreenAsksForTheNextPage() async throws {
        let account = ReactionsAccount([reaction("👍", 9)], people: ["👍": [person(2, "Ray", "ray")]])
        account.more.withLock { $0 = true }

        let (_, view) = try await shown(account)

        try await view.waitUntil("the next page") {
            account.asked.withLock { $0 } == [nil, UserId(rawValue: 2)]
        }
    }
}

// The session's window: the reactions window over a dimmed session, the removals' question,
// and the sound for a refused reaction.
@MainActor
struct ReactionsSessionWindowTests {
    let account = FormattedAccount(contents: ["test"], readUpTo: 5)
    let session: SessionModel
    let view: ShownView

    init() {
        account.formattedStore.reactable.withLock { $0 = true }
        account.formattedStore.shownReactions.withLock { $0 = [reaction("👍", 1)] }
        session = SessionScreenTests().formattedSession(account)
        view = ShownView(SessionScreen(session: session, splitWidth: SplitWidth()), size: NSSize(width: 1100, height: 700))
    }

    func row() throws -> MessageListModel.Row {
        try #require(session.messages?.rows.last)
    }

    @Test
    func theReactionsWindowOpensOverTheDimmedSession() async throws {
        let message = try row().message

        session.viewReactions(channelId: message.channelId, messageId: message.id)

        try await view.waitUntil("the window") {
            view.window.childWindows?.first { $0 is ShieldWindow }?.childWindows?.contains { $0 is ContainerPanel }
                == true
        }
        let shield = try #require(view.window.childWindows?.first { $0 is ShieldWindow })
        #expect(shield.backgroundColor.alphaComponent > 0.5)
        session.close()
    }

    func answer(_ button: String) async throws {
        try await view.waitUntil("the question") { view.window.attachedSheet != nil }
        let sheet = try #require(view.window.attachedSheet?.contentView)
        let buttons = MessageMenuAreaTests.views(NSButton.self, in: sheet)
        try #require(buttons.first { $0.title == button }).performClick(nil)
        try await view.waitUntil("the answer") { view.window.attachedSheet == nil }
    }

    @Test
    func theQuestionsNoRemovesNothingAndItsYesRemoves() async throws {
        let list = try #require(session.messages)
        let actions = session.messageActions(for: list) { _ in }
        let message = try row()

        actions.removeReactions(of: nil, on: message, shift: false)
        try await answer("No")
        #expect(session.messageConfirmation == nil)

        let pill = try #require(actions.removableReactions(for: message).first)
        actions.removeReactions(of: pill, on: message, shift: false)
        try await answer("Yes")

        try await view.waitUntil("the removal") { !account.reactionCalls.withLock { $0 }.isEmpty }
        #expect(account.reactionCalls.withLock { $0 } == ["remove 👍 of \(message.message.id.rawValue)"])
        session.close()
    }

    @Test
    func aRefusedReactionPlaysTheAlertSound() async throws {
        final class Sounds {
            var played = 0
        }
        let sounds = Sounds()
        view.host.rootView = AnyView(
            SessionScreen(session: session, splitWidth: SplitWidth())
                .environment(\.refusalSound) { sounds.played += 1 }
                .frame(width: 1100, height: 700))
        view.host.layoutSubtreeIfNeeded()
        account.reactionError.withLock { $0 = .Blocked }
        let list = try #require(session.messages)

        await list.reactions.toggle(.unicode("💯"), on: try row(), from: .hoverBar)

        try await view.waitUntil("the sound") { sounds.played == 1 }
        session.close()
    }
}
