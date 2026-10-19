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

    let general = ChannelName(kind: .guildChannel(.guildText), name: "general")
    let mira = ChannelName(kind: .direct(author(7, "Mira")), name: "Mira")
    let group = ChannelName(kind: .group, name: "Weekend plans")
    let thread = ChannelName(kind: .thread(owner: nil), name: "release-notes")
    let unknown = ChannelName(kind: .direct(nil), name: "")

    // The official client's wording.
    @Test
    func namesGivePlaceholders() {
        #expect(general.placeholder == "Message #general")
        #expect(mira.placeholder == "Message @Mira")
        #expect(group.placeholder == "Message Weekend plans")
        #expect(thread.placeholder == "Message \"release-notes\"")
        #expect(unknown.placeholder == "Message")
    }

    // Q38: Message "t1".
    @Test
    func aThreadsPlaceholderQuotesItsName() {
        #expect(ChannelName(kind: .thread(owner: "Ray"), name: "t1").placeholder == "Message \"t1\"")
    }

    // The official client's text, for channels and locked threads alike (Q38).
    @Test
    func aDeniedComposerSaysYouDoNotHavePermission() {
        #expect(
            ComposerView.prompt(denied: true, otherwise: "Message #general")
                == "You do not have permission to send messages in this channel.")
        #expect(
            ComposerView.prompt(denied: false, otherwise: "Message #general")
                == "Message #general")
    }

    @Test
    func namesGiveTheChannelsStart() {
        #expect(
            general.start
                == ChannelStart(
                    icon: .symbol("number"), title: "Welcome to #general!",
                    subtitle: "This is the start of the #general channel."))
        #expect(
            mira.start
                == ChannelStart(
                    icon: .avatar(name: "Mira", id: 7), title: "Mira", username: "mira",
                    subtitle: "This is the beginning of your direct message history with Mira."))
        #expect(
            group.start
                == ChannelStart(
                    icon: .symbol("person.2.fill"), title: "Weekend plans",
                    subtitle: "Welcome to the beginning of the Weekend plans group."))
        #expect(thread.start?.icon == .symbol("text.bubble"))
        #expect(unknown.start == nil)
    }
}
