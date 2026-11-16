import Foundation
import Testing

@testable import AkariKit

func version(_ major: UInt16, _ minor: UInt16 = 0) -> EmojiVersion {
    EmojiVersion(major: major, minor: minor)
}

struct EmojiCatalogTests {
    let catalog = unicodeEmoji(newest: newestEmojiVersion()).flatMap(\.emojis)

    @Test
    func versionsAreOrderedByMajorThenMinor() {
        #expect(version(15) < version(15, 1))
        #expect(version(15, 1) < version(16))
        #expect(version(9, 9) < version(10))
        #expect(!(version(16) < version(16)))
    }

    @Test
    func everyEmojiOfferedIsOnePictureOfTheSystemsEmojiFont() {
        let offered = EmojiCatalog.groups.flatMap(\.emojis)

        #expect(offered.count > 1800)
        #expect(offered.filter { !EmojiCatalog.drawsAsOnePicture($0.emoji) }.map(\.emoji) == [])
        #expect(offered.allSatisfy { $0.version <= EmojiCatalog.newest })
    }

    @Test
    func whatTheOldestSupportedMacDrawsIsAlwaysOffered() {
        #expect(EmojiCatalog.newest >= version(15))
        #expect(EmojiCatalog.newest(of: catalog) { _ in false } == version(15))
    }

    @Test
    func aLaterVersionCountsOnlyWhenTheFontDrawsEveryEmojiOfIt() throws {
        let sixteen = try #require(catalog.first { $0.version == version(16) })

        let newest = EmojiCatalog.newest(of: catalog) { $0 != sixteen.emoji }

        #expect(newest == version(15, 1))
    }

    @Test
    func aVersionAfterOneTheFontMissesIsLeftOutToo() throws {
        let fifteenOne = try #require(catalog.first { $0.version == version(15, 1) })

        #expect(EmojiCatalog.newest(of: catalog) { $0 != fifteenOne.emoji } == version(15))
    }

    @Test
    func aFontThatDrawsEverythingGivesTheCatalogsNewest() {
        #expect(EmojiCatalog.newest(of: catalog) { _ in true } == newestEmojiVersion())
    }

    @Test(arguments: [
        ("😀", true), ("🇩🇪", true), ("👍🏽", true), ("1️⃣", true), ("❤️", true),
        // Apple draws a couple's picture as two layered glyphs, one of them advancing.
        ("👩‍❤️‍💋‍👨", true),
        ("😀😀", false), ("ab", false), ("\u{E000}", false), ("", false),
    ])
    func onePictureIsOneAdvancingGlyphOfTheEmojiFont(emoji: String, one: Bool) {
        #expect(EmojiCatalog.drawsAsOnePicture(emoji) == one)
    }

    @Test
    func theGroupsAreTheCatalogsUpToTheNewestDrawnInItsOrder() {
        let groups = EmojiCatalog.groups

        #expect(groups.map(\.group) == unicodeEmoji(newest: newestEmojiVersion()).map(\.group))
        #expect(groups.count == 9)
        #expect(groups.first?.emojis.first?.emoji == "😀")
        #expect(groups.flatMap(\.emojis).map(\.emoji) == catalog.filter { $0.version <= EmojiCatalog.newest }.map(\.emoji))
    }
}

struct EmojiNameTests {
    @Test(arguments: [
        ("👍", ":thumbsup:"), ("😀", ":grinning:"), ("👍🏽", ":thumbsup_tone3:"), ("🇩🇪", ":flag_de:"),
        ("❤️", ":heart:"), ("💯", ":100:"),
        // No short name: Discord leaves © as text.
        ("©️", "©️"),
    ])
    func aUnicodeEmojiIsNamedByItsShortName(emoji: String, name: String) {
        #expect(ReactionPill.name(of: .unicode(emoji)) == name)
    }

    @Test
    func aCustomEmojiIsNamedByItsOwnName() {
        #expect(ReactionPill.name(of: custom(7, "party_parrot")) == ":party_parrot:")
        #expect(ReactionPill.name(of: custom(7, nil)) == ":emoji:")
    }

    @Test(arguments: [
        ("👍", "thumbsup"), ("😍", "heart eyes"), ("🇩🇪", "flag de"), ("©️", "©️"),
    ])
    func voiceOverReadsTheNameWithoutColonsOrUnderscores(emoji: String, spoken: String) {
        #expect(ReactionPill.spokenName(of: .unicode(emoji)) == spoken)
    }

    @Test
    func aReactionsWindowTabReadsTheNameAndTheCount() {
        let pill = ReactionPill(emoji: .unicode("👍"), normal: 2, burst: 1, mine: false)

        #expect(ReactionWords.tab(pill) == "thumbsup 3 reactions")
        #expect(ReactionWords.tab(ReactionPill(emoji: custom(7, "party_parrot"), normal: 1, mine: false)) == "party parrot 1 reaction")
    }

    @Test
    func aQuickReactionIsNamedAndReadByItsShortName() {
        let quick = QuickReaction(emoji: .unicode("😆"), mine: true)

        #expect(quick.name == ":laughing:")
        #expect(quick.accessibilityLabel == "Click to remove laughing")
        #expect(QuickReaction(emoji: .unicode("💯"), mine: false).accessibilityLabel == "Click to react with 100")
    }
}
