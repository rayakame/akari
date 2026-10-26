import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// The age gate in place of a gated channel's list and composer (D-10): Discord's words, Go Back
// and Continue, Continue only when the user may.
@MainActor @Suite(.timeLimit(.minutes(1)))
struct AgeGateViewTests {
    @Test
    func theGateSaysWhatDiscordSays() {
        let channel = AgeGateView.words(.ask(server: false))
        #expect(channel.title == "Age-Restricted Channel")
        #expect(channel.text == "This channel contains adult content marked as age-restricted. Do you wish to proceed?")
        #expect(channel.continues)
        let server = AgeGateView.words(.ask(server: true))
        #expect(server.title == "Age-Restricted Server")
        #expect(server.continues)
        let young = AgeGateView.words(.tooYoung(server: false))
        #expect(young.title == "Sorry, you're not old enough to view this age-restricted channel")
        #expect(!young.continues)
        #expect(!AgeGateView.words(.tooYoung(server: true)).continues)
    }

    // Continue is the brand-colored button; Go Back is gray.
    func showsBrand(_ gate: AgeGate) throws -> Bool {
        let rendered = try Rendered(
            AgeGateView(gate: gate, agree: {}, back: {}), size: NSSize(width: 800, height: 400))
        for y in stride(from: 0, to: 400, by: 2) {
            for x in stride(from: 0, to: 800, by: 2) {
                guard let color = rendered.color(CGFloat(x), CGFloat(y)) else {
                    continue
                }
                // Discord's blurple; the page and Go Back are gray.
                if color.blueComponent - color.redComponent > 0.4 {
                    return true
                }
            }
        }
        return false
    }

    @Test
    func continueShowsOnlyWhenTheUserMay() throws {
        #expect(try showsBrand(.ask(server: false)))
        #expect(try !showsBrand(.tooYoung(server: false)))
    }
}
