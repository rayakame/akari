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
}
