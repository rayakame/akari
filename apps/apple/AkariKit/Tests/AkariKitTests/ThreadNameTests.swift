import Foundation
import Testing

@testable import AkariKit

// Discord's client (canary 649224, module 659617) and Chris's TF-O1.
@MainActor
struct ThreadNameTests {
    let times = TimestampText(
        locale: Locale(identifier: "en_US"), timeZone: TimeZone(identifier: "UTC")!,
        calendar: Calendar(identifier: .gregorian))

    func name(_ content: String, embedTitle: String? = nil, known: [UInt64: String] = [:])
        -> String
    {
        var message = message(1, in: 11, content: content)
        message.embedTitle = embedTitle
        message.formatted = Self.naming(message.formatted, known)
        return ThreadName.derived(from: message, times: times)
    }

    // The parser alone knows no names; the store gives them.
    static func naming(_ text: FormattedText, _ known: [UInt64: String]) -> FormattedText {
        var text = text
        for block in text.blocks.indices {
            for span in text.blocks[block].spans.indices {
                switch text.blocks[block].spans[span].kind {
                case .user(let id, _):
                    if let name = known[id.rawValue] {
                        text.blocks[block].spans[span].kind = .user(id: id, name: .known(name: name))
                    }
                case .role(let id, _, let color):
                    if let name = known[id.rawValue] {
                        text.blocks[block].spans[span].kind = .role(
                            id: id, name: .known(name: name), color: color)
                    }
                case .channel(let id, _, let kind):
                    if let name = known[id.rawValue] {
                        text.blocks[block].spans[span].kind = .channel(
                            id: id, name: .known(name: name), kind: kind)
                    }
                default:
                    break
                }
            }
        }
        return text
    }

    @Test
    func wholeWordsWithin40KeepTheirTrailingSpaceAndGetNoEllipsis() {
        #expect(
            name("das ist ein ziemlich langer satz zum testen von threads")
                == "das ist ein ziemlich langer satz zum ")
        #expect(
            name("hello, world! this is a test of the thread naming algorithm")
                == "hello, world! this is a test of the ")
        #expect(name(String(repeating: "a", count: 39) + " b c") == String(repeating: "a", count: 39) + " ")
    }

    @Test
    func aFirstWordLongerThan40IsCutWithAnEllipsis() {
        #expect(
            name("Supercalifragilisticexpialidociousandmorelettershere yes")
                == "Supercalifragilisticexpialidociousandmor...")
        #expect(
            name("https://example.com/some/very/long/path/that/keeps/going")
                == "https://example.com/some/very/long/path/")
    }

    @Test
    func onlyTheFirstLineWithoutLeadingSpacesHashesAndDashes() {
        #expect(name("Erste Zeile\nZweite Zeile ist hier") == "Erste Zeile")
        #expect(name("# Überschrift mit Text") == "Überschrift mit Text")
        #expect(name("- ein Punkt") == "ein Punkt")
        #expect(name("> zitat hier") == "> zitat hier")
        #expect(name("**fett** bleibt") == "**fett** bleibt")
    }

    @Test
    func mentionsEmojiSpoilersAndTimestampsReadAsText() {
        #expect(
            name("<@100> kannst du das prüfen?", known: [100: "Ray"])
                == "@Ray kannst du das prüfen?")
        #expect(name("<@100> ist weg") == "<@100> ist weg")
        #expect(name("frag <@&200>", known: [200: "mods"]) == "frag @mods")
        #expect(name("siehe <#300>", known: [300: "general"]) == "siehe #general")
        #expect(name("<:akari:12345> hallo") == ":akari: hallo")
        #expect(name("||geheim|| text") == "<spoiler> text")
        #expect(name("um <t:0:d>") == "um " + times.text(0, style: .shortDate, now: Date()))
    }

    @Test
    func theFirstEmbedsTitleWinsCutAt40() {
        #expect(name("text", embedTitle: "Release notes") == "Release notes")
        #expect(
            name("text", embedTitle: String(repeating: "x", count: 45))
                == String(repeating: "x", count: 40) + "...")
        #expect(name("text", embedTitle: "") == "text")
    }

    @Test
    func nothingToNameItIsEmpty() {
        #expect(name("") == "")
        #expect(name(String(repeating: "!", count: 44) + " ok") == "")
    }

    @Test
    func discordCountsUTF16AndAnEmojiIsNeverSplit() {
        let emoji = String(repeating: "😀", count: 25)
        #expect(name(emoji) == String(repeating: "😀", count: 20) + "...")
    }
}
