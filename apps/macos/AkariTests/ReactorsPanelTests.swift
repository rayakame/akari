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

        #expect(reactors.tabs.map(\.name) == ["🎉", "👍"])
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

    @Test
    func theWindowClosesWhenTheSessionSaysSo() throws {
        let session = account.window()
        #expect(session.reactors != nil)

        session.closeReactors()

        #expect(session.reactors == nil)
    }
}
