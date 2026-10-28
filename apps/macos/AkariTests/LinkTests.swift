import AkariKit
import AppKit
import Testing

@testable import Akari

@MainActor
struct LinkTests {
    let list = MessageListFixture()

    // The dialog's answers, given in turn, and what was opened; without an answer in turn the
    // dialog stays open until answered or dismissed.
    final class Recorder {
        var answers: [LinkFollower.Answer] = []
        var asked: [URL] = []
        var opened: [URL] = []
        var open: ((LinkFollower.Answer) -> Void)?
        var dismissed = 0

        func follower(_ trusted: TrustedLinks) -> LinkFollower {
            LinkFollower(
                trusted: trusted, open: { self.opened.append($0) },
                ask: { url, answer in
                    self.asked.append(url)
                    guard self.answers.isEmpty else {
                        answer(self.answers.removeFirst())
                        return {}
                    }
                    self.open = answer
                    return {
                        self.dismissed += 1
                        self.answer(.init(open: false, trust: false))
                    }
                })
        }

        func answer(_ answer: LinkFollower.Answer) {
            let open = self.open
            self.open = nil
            open?(answer)
        }
    }

    func trusted() -> TrustedLinks {
        TrustedLinks(account: UserId(rawValue: 1), defaults: MemoryDefaults())
    }

    @Test
    func aPlainLinkOpens() throws {
        list.show([row(1, at: list.noon, content: "see https://example.org for more")])

        try list.click("https://example.org", in: 1)
        #expect(list.actions.calls == ["link https://example.org/"])

        let recorder = Recorder()
        recorder.follower(trusted()).follow(try #require(URL(string: "https://example.org/")), masked: false)
        #expect(recorder.asked.isEmpty)
        #expect(recorder.opened == [URL(string: "https://example.org/")])
    }

    @Test
    func aMaskedLinkAsksFirst() throws {
        list.show([row(1, at: list.noon, content: "[the guide](https://example.com/a/b?c=d)")])
        try list.click("the guide", in: 1)
        #expect(list.actions.calls == ["link https://example.com/a/b?c=d masked"])

        let url = try #require(URL(string: "https://example.com/a/b?c=d"))
        let alert = LinkConfirmation.alert(for: url)
        #expect(alert.messageText == "Open This Link?")
        #expect(alert.informativeText.contains("This link goes to the following website:"))
        #expect(alert.informativeText.contains("https://example.com/a/b?c=d"))
        #expect(alert.buttons.map(\.title) == ["Open Link", "Back"])
        #expect(alert.suppressionButton?.title == "Trust links to example.com from now on")

        let recorder = Recorder()
        let follower = recorder.follower(trusted())
        recorder.answers = [.init(open: false, trust: false), .init(open: true, trust: false)]
        follower.follow(url, masked: true)
        #expect(recorder.asked == [url])
        #expect(recorder.opened.isEmpty)
        follower.follow(url, masked: true)
        #expect(recorder.asked == [url, url])
        #expect(recorder.opened == [url])
    }

    // Trust is by host; these would ask again however the box was set.
    @Test(arguments: ["mailto:someone@example.com", "tel:+15551234567", "sms:+15551234567"])
    func aLinkWithoutAHostOffersNoTrust(_ string: String) throws {
        let alert = LinkConfirmation.alert(for: try #require(URL(string: string)))

        #expect(!alert.showsSuppressionButton)
        #expect(LinkConfirmation.alert(for: try #require(URL(string: "https://example.com/")))
            .showsSuppressionButton)
    }

    @Test
    func theDialogNamesTheHostAsItIsTrusted() throws {
        let url = try #require(URL(string: "https://bücher.example/a"))

        let alert = LinkConfirmation.alert(for: url)

        #expect(alert.suppressionButton?.title == "Trust links to xn--bcher-kva.example from now on")
    }

    @Test
    func aConfirmationAnsweredAfterTheAccountWasForgottenTrustsNothing() throws {
        let defaults = MemoryDefaults()
        let account = UserId(rawValue: 1)
        let recorder = Recorder()
        let url = try #require(URL(string: "https://example.com/a"))
        recorder.follower(TrustedLinks(account: account, defaults: defaults))
            .follow(url, masked: true)

        AccountMemory(defaults: defaults).forget(account)
        recorder.answer(.init(open: true, trust: true))

        #expect(!TrustedLinks(account: account, defaults: defaults).isTrusted(url))
    }

    @Test
    func closingTheFollowerEndsItsConfirmationAsBackWould() throws {
        let recorder = Recorder()
        let follower = recorder.follower(trusted())
        follower.follow(try #require(URL(string: "https://example.com/a")), masked: true)

        follower.close()

        #expect(recorder.dismissed == 1)
        #expect(recorder.opened.isEmpty)
        follower.close()
        #expect(recorder.dismissed == 1)
    }

    @Test
    func theDialogEndsWhenDismissed() async throws {
        let window = TestWindow.make(size: NSSize(width: 400, height: 300))
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }
        var answers: [LinkFollower.Answer] = []
        let dismiss = LinkConfirmation.ask(
            try #require(URL(string: "https://example.com/a")), in: window
        ) { answers.append($0) }
        #expect(window.attachedSheet != nil)

        dismiss()
        for _ in 0..<50 where answers.isEmpty || window.attachedSheet != nil {
            try await Task.sleep(for: .milliseconds(20))
        }

        #expect(answers.map(\.open) == [false])
        #expect(window.attachedSheet == nil)
        dismiss()
        #expect(answers.count == 1)
    }

    @Test
    func trustingTheHostOpensItsLinksDirectly() throws {
        let recorder = Recorder()
        let trusted = trusted()
        let follower = recorder.follower(trusted)
        let first = try #require(URL(string: "https://example.com/one"))
        recorder.answers = [.init(open: true, trust: true), .init(open: false, trust: false)]

        follower.follow(first, masked: true)
        follower.follow(try #require(URL(string: "https://EXAMPLE.com/two")), masked: true)
        follower.follow(try #require(URL(string: "https://example.org/")), masked: true)

        #expect(trusted.isTrusted(first))
        #expect(recorder.asked.map(\.host) == ["example.com", "example.org"])
        #expect(recorder.opened.map(\.path) == ["/one", "/two"])
    }

    @Test(arguments: [
        "http://example.com/", "https://example.com/", "HTTPS://example.com/",
        "discord://-/channels/1/2", "steam://run/440", "mailto:someone@example.com",
        "tel:+15551234567", "sms:+15551234567",
    ])
    func theParsersSchemesOpen(_ string: String) throws {
        let recorder = Recorder()
        let url = try #require(URL(string: string))

        recorder.follower(trusted()).follow(url, masked: false)

        #expect(recorder.opened == [url])
    }

    // The custom scheme's host is trusted, so only the scheme stops it.
    @Test(arguments: ["file:///etc/hosts", "javascript:alert(1)", "akari-test-app://example.com/open"])
    func aLinkWithAnotherSchemeOpensNothing(_ string: String) throws {
        let recorder = Recorder()
        let trusted = trusted()
        trusted.trust(try #require(URL(string: "https://example.com/")))
        recorder.answers = [.init(open: true, trust: false)]
        let follower = recorder.follower(trusted)
        let url = try #require(URL(string: string))

        follower.follow(url, masked: false)
        follower.follow(url, masked: true)

        #expect(recorder.asked.isEmpty)
        #expect(recorder.opened.isEmpty)
    }

    @Test
    func aChannelLinkPillOpensTheChannel() throws {
        list.show([named(row(1, at: list.noon, content: "https://discord.com/channels/1/2"))])

        try list.click("general", in: 1)

        #expect(list.actions.calls == ["channel 2"])
    }

    @Test
    func aMessageLinkPillOpensItsChannel() throws {
        list.show([named(row(1, at: list.noon, content: "https://discord.com/channels/1/2/3"))])

        try list.click("general", in: 1)

        #expect(list.actions.calls == ["channel 2"])
    }

    @Test
    func aLinkToAnUnknownChannelOpensTheBrowser() throws {
        list.show([named(row(1, at: list.noon, content: "https://discord.com/channels/1/9"))])

        try list.click("https://discord.com/channels/1/9", in: 1)

        #expect(list.actions.calls == ["link https://discord.com/channels/1/9"])
    }

    @Test
    func aChannelMentionOpensTheChannel() throws {
        list.show([named(row(1, at: list.noon, content: "over in <#5>"))])

        try list.click("general", in: 1)

        #expect(list.actions.calls == ["channel 5"])
    }

    @Test
    func userAndRolePillsDoNothing() throws {
        list.show([named(row(1, at: list.noon, content: "<@1> and <@&4>"))])
        let text = try list.text(of: 1)

        for shown in ["@mira", "@Mods"] {
            let index = (text.string as NSString).range(of: shown).location
            #expect(text.textStorage?.attribute(.link, at: index, effectiveRange: nil) == nil)
        }
        #expect(list.actions.calls.isEmpty)
    }
}
