import Foundation
import Testing

@testable import AkariKit

@MainActor
struct TrustedLinksTests {
    let defaults = MemoryDefaults()

    func url(_ string: String) -> URL {
        URL(string: string)!
    }

    @Test
    func aTrustedHostIsRemembered() {
        TrustedLinks(account: id(1), defaults: defaults).trust(url("https://example.com/a"))

        let again = TrustedLinks(account: id(1), defaults: defaults)
        #expect(again.isTrusted(url("https://example.com/b?c")))
        #expect(!again.isTrusted(url("https://other.example/")))
    }

    @Test
    func trustIsPerAccount() {
        TrustedLinks(account: id(1), defaults: defaults).trust(url("https://example.com/"))

        #expect(!TrustedLinks(account: id(2), defaults: defaults).isTrusted(url("https://example.com/")))
    }

    @Test
    func trustCoversOnlyThatHost() {
        let links = TrustedLinks(account: id(1), defaults: defaults)
        links.trust(url("https://docs.example.com/"))

        #expect(!links.isTrusted(url("https://example.com/")))
        #expect(!links.isTrusted(url("https://a.docs.example.com/")))
        #expect(links.isTrusted(url("http://docs.example.com/other")))
    }

    @Test
    func hostsCompareWithoutCase() {
        let links = TrustedLinks(account: id(1), defaults: defaults)
        links.trust(url("https://Example.COM/"))

        #expect(links.isTrusted(url("https://example.com/")))
    }

    @Test
    func userinfoIsntTheHost() {
        let links = TrustedLinks(account: id(1), defaults: defaults)
        links.trust(url("https://example.com/"))

        #expect(!links.isTrusted(url("https://example.com@evil.example/")))

        links.trust(url("https://example.com@evil.example/"))
        #expect(links.isTrusted(url("https://evil.example/")))
        #expect(!links.isTrusted(url("https://other.example/")))
    }

    @Test
    func trustCoversEveryPortOfTheHost() {
        let links = TrustedLinks(account: id(1), defaults: defaults)
        links.trust(url("https://example.com:8443/a"))

        #expect(links.isTrusted(url("https://example.com/")))
        #expect(links.isTrusted(url("https://example.com:9000/")))
    }

    @Test
    func aTrailingDotNamesTheSameHost() {
        let links = TrustedLinks(account: id(1), defaults: defaults)
        links.trust(url("https://example.com./"))

        #expect(links.isTrusted(url("https://example.com/")))
        #expect(!links.isTrusted(url("https://example.com.evil.example/")))
    }

    @Test
    func anInternationalHostIsTrustedInBothSpellings() {
        let links = TrustedLinks(account: id(1), defaults: defaults)
        links.trust(url("https://bücher.example/"))

        #expect(links.isTrusted(url("https://xn--bcher-kva.example/")))
        #expect(links.isTrusted(url("https://BÜCHER.example/")))
        // A Cyrillic "а" in place of the Latin one.
        links.trust(url("https://example.com/"))
        #expect(!links.isTrusted(url("https://ex\u{0430}mple.com/")))
    }
}
