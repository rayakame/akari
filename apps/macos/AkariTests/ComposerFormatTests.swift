import AkariKit
import Foundation
import Testing

@testable import Akari

struct ComposerFormatTests {
    @Test
    func countdownsReadLikeAStopwatch() {
        #expect(ComposerStatus.countdown(9) == "0:09")
        #expect(ComposerStatus.countdown(9.2) == "0:10")
        #expect(ComposerStatus.countdown(65) == "1:05")
        #expect(ComposerStatus.countdown(3600) == "1:00:00")
    }

    @Test
    func slowmodeNotesSayHowOften() {
        let note = { (interval: TimeInterval, exempt: Bool) in
            ComposerStatus.note(Slowmode(interval: interval, exempt: exempt, until: nil))
        }
        #expect(note(10, false) == "Slowmode: one message every 10 seconds")
        #expect(note(60, false) == "Slowmode: one message every minute")
        #expect(note(120, false) == "Slowmode: one message every 2 minutes")
        #expect(note(21600, false) == "Slowmode: one message every 6 hours")
        #expect(note(10, true) == "Slowmode is on, but it doesn't apply to you")
    }

    @Test
    func placeholdersNameTheChannelOrPerson() {
        #expect(
            ComposerView.placeholder(guildChannel: true, name: "general")
                == "Write a message in #general")
        #expect(
            ComposerView.placeholder(guildChannel: false, name: "Mira")
                == "Write a message to Mira")
    }
}
