import AkariKit
import AppKit
import Testing

@testable import Akari

@MainActor
struct MessageTextTests {
    let times = TimestampText(
        locale: Locale(identifier: "en_US"), timeZone: TimeZone(identifier: "Europe/Berlin")!,
        calendar: Calendar(identifier: .gregorian))
    let now = Date(timeIntervalSince1970: 1_800_000_000 - 300)

    func message(
        _ content: String, edited: Bool = false, names: (inout SpanKind) -> Void = { _ in }
    ) -> Message {
        var message = row(1, at: now, content: content, edited: edited ? now : nil).message
        for block in message.formatted.blocks.indices {
            for span in message.formatted.blocks[block].spans.indices {
                names(&message.formatted.blocks[block].spans[span].kind)
            }
        }
        return message
    }

    func built(
        _ content: String, edited: Bool = false, names: (inout SpanKind) -> Void = { _ in }
    ) -> MessageText.Built {
        MessageText.build(
            message(content, edited: edited, names: names), revealed: [], now: now, times: times)
    }

    func attributes(_ text: NSAttributedString, of shown: String) throws
        -> [NSAttributedString.Key: Any]
    {
        let range = (text.string as NSString).range(of: shown)
        try #require(range.location != NSNotFound, "\(shown) in \(text.string)")
        return text.attributes(at: range.location, effectiveRange: nil)
    }

    func font(_ text: NSAttributedString, of shown: String) throws -> NSFont {
        try #require(try attributes(text, of: shown)[.font] as? NSFont)
    }

    func paragraph(_ text: NSAttributedString, of shown: String) throws -> NSParagraphStyle {
        try #require(try attributes(text, of: shown)[.paragraphStyle] as? NSParagraphStyle)
    }

    @Test
    func stylesUseFontTraitsAndLines() throws {
        let text = built("**b** *i* __u__ ~~s~~ ***x***").text

        #expect(try font(text, of: "b").fontDescriptor.symbolicTraits.contains(.bold))
        #expect(try font(text, of: "i").fontDescriptor.symbolicTraits.contains(.italic))
        let both = try font(text, of: "x").fontDescriptor.symbolicTraits
        #expect(both.contains(.bold) && both.contains(.italic))
        #expect(try attributes(text, of: "u")[.underlineStyle] as? Int == 1)
        #expect(try attributes(text, of: "s")[.strikethroughStyle] as? Int == 1)
        #expect(try font(text, of: "b").pointSize == 16)
        #expect(text.string == "b i u s x")
    }

    @Test
    func headingsAndSubtextHaveTheirSizes() throws {
        let text = built("# one\n## two\n### three\n-# small\nbody").text

        for (shown, size, line) in [
            ("one", 24.0, 32.0), ("two", 20, 28), ("three", 16, 22), ("small", 13, 18),
            ("body", 16, 22),
        ] {
            #expect(try font(text, of: shown).pointSize == size, "\(shown)")
            #expect(try paragraph(text, of: shown).minimumLineHeight == line, "\(shown)")
        }
        #expect(try font(text, of: "two").fontDescriptor.symbolicTraits.contains(.bold))
        #expect(try paragraph(text, of: "one").paragraphSpacingBefore == 0)
        #expect(try paragraph(text, of: "two").paragraphSpacingBefore == 8)
        #expect(try paragraph(text, of: "two").paragraphSpacing == 4)
        #expect(try attributes(text, of: "small")[.foregroundColor] as? NSColor == Palette.textMuted)
    }

    @Test
    func listsIndentAndShowTheirMarkers() throws {
        // The line of text ends the bulleted list; a number right after a bullet would continue it.
        let text = built("- a\n  - b\nx\n1. c\n2. d").text

        #expect(text.string.contains("•\ta"))
        #expect(text.string.contains("◦\tb"))
        #expect(text.string.contains("1.\tc"))
        #expect(try paragraph(text, of: "a").headIndent == 20)
        #expect(try paragraph(text, of: "b").headIndent == 40)
    }

    @Test
    func quotesIndentAndCarryTheBar() throws {
        let text = built("> a\n> b\nc").text

        #expect(try paragraph(text, of: "a").headIndent == 16)
        #expect(try attributes(text, of: "a")[.akariBlock] as? MessageText.Block == .quote)
        #expect(try attributes(text, of: "c")[.akariBlock] == nil)
    }

    @Test
    func inlineCodeIsAMonospacedPill() throws {
        let text = built("x `code` y").text

        let code = try font(text, of: "code")
        #expect(code.pointSize == 14 && code.isFixedPitch)
        #expect(try attributes(text, of: "code")[.akariPill] as? MessageText.Pill == .code)
        #expect(try attributes(text, of: "e")[.kern] as? CGFloat == 2)
        #expect(text.attribute(.kern, at: 1, effectiveRange: nil) as? CGFloat == 2)
    }

    @Test
    func codeBlocksAreMonospacedBoxesWithoutTheirLanguage() throws {
        let text = built("before\n```py\nprint(1)\n```").text

        #expect(!text.string.contains("py\n"))
        let code = try font(text, of: "print(1)")
        #expect(code.pointSize == 14 && code.isFixedPitch)
        #expect(try paragraph(text, of: "print(1)").minimumLineHeight == 18)
        #expect(try paragraph(text, of: "print(1)").headIndent == 8)
        #expect(try attributes(text, of: "print(1)")[.akariBlock] as? MessageText.Block == .code)
    }

    @Test
    func mentionsShowTheirNames() throws {
        let text = built("<@1> <@2> <@3> <@&4> <#5>") { kind in
            switch kind {
            case .user(let id, _) where id.rawValue == 1: kind = .user(id: id, name: .known(name: "mira"))
            case .user(let id, _) where id.rawValue == 2: kind = .user(id: id, name: .missing)
            case .role(let id, _, _): kind = .role(id: id, name: .known(name: "Mods"), color: 0)
            case .channel(let id, _, _):
                kind = .channel(id: id, name: .known(name: "general"), kind: .guildText)
            default: break
            }
        }.text

        let mira = try attributes(text, of: "@mira")
        #expect(mira[.akariPill] as? MessageText.Pill == .mention)
        #expect(mira[.foregroundColor] as? NSColor == Palette.mentionText)
        #expect((mira[.font] as? NSFont)?.fontDescriptor.symbolicTraits.contains(.bold) == false)
        let missing = try attributes(text, of: "<@2>")
        #expect(missing[.akariPill] == nil)
        let placeholder = try attributes(text, of: "@user")
        #expect(placeholder[.foregroundColor] as? NSColor == Palette.textMuted)
        #expect(try attributes(text, of: "@Mods")[.akariPill] as? MessageText.Pill == .mention)
        #expect(try attributes(text, of: "general")[.akariPill] as? MessageText.Pill == .mention)
        #expect(text.string.contains("\u{FFFC}general"))
    }

    @Test
    func aRoleWithAColorTintsItsPill() throws {
        let text = built("<@&4>") { kind in
            if case .role(let id, _, _) = kind {
                kind = .role(id: id, name: .known(name: "Red"), color: 0xff0000)
            }
        }.text

        let red = try attributes(text, of: "@Red")
        let color = try #require((red[.foregroundColor] as? NSColor)?.usingColorSpace(.sRGB))
        #expect(color.redComponent == 1 && color.greenComponent == 0)
        let pill = try #require(red[.akariPill] as? MessageText.Pill)
        #expect(pill == .role(0xff0000))
    }

    @Test
    func aChannelWithoutAccessShowsALock() throws {
        let text = built("<#5>") { kind in
            if case .channel(let id, _, _) = kind {
                kind = .channel(id: id, name: .noAccess, kind: .guildText)
            }
        }.text

        #expect(text.string == "\u{FFFC}No Access")
    }

    // The sidebar doesn't open them either: no message list for forums, voice or categories.
    @Test
    func aChannelWithoutAMessageListIsntALink() throws {
        let text = built("<#5> https://discord.com/channels/1/6") { kind in
            if case .channel(let id, _, _) = kind {
                kind = .channel(id: id, name: .known(name: "help"), kind: .guildForum)
            }
        }.text
        var message = message("https://discord.com/channels/1/6")
        message.formatted.links[0].channel?.name = .known(name: "help")
        message.formatted.links[0].channel?.kind = .guildForum
        let link = MessageText.build(message, revealed: [], now: now, times: times).text

        let mention = try attributes(text, of: "help")
        #expect(mention[.akariPill] as? MessageText.Pill == .mention)
        #expect(mention[.link] == nil)
        #expect(mention[.toolTip] as? String == ChannelSymbols.unsupported(.guildForum))
        #expect(try attributes(link, of: "help")[.link] == nil)
    }

    @Test
    func carriageReturnsBreakLinesWithoutNewParagraphs() {
        let text = built("```\na\r\nb\rc\n```").text

        #expect(text.string == "a\u{2028}b\u{2028}c")
    }

    @Test
    func discordLinksToKnownChannelsArePills() throws {
        let content =
            "https://discord.com/channels/1/2 https://discord.com/channels/1/2/3 https://discord.com/channels/1/9"
        var message = message(content)
        for index in message.formatted.links.indices {
            let link = try #require(message.formatted.links[index].channel)
            message.formatted.links[index].channel?.name =
                link.channelId.rawValue == 2 ? .known(name: "general") : .missing
            message.formatted.links[index].channel?.kind = .guildText
        }
        let text = MessageText.build(message, revealed: [], now: now, times: times).text

        #expect(text.string.hasPrefix("\u{FFFC}general \u{FFFC}general\u{FFFC} https://discord.com/channels/1/9"))
        #expect(try attributes(text, of: "general")[.akariPill] as? MessageText.Pill == .mention)
        #expect(try attributes(text, of: "general")[.link] as? URL == URL(string: "akari://channel/2"))
        let unknown = try attributes(text, of: "https://discord.com/channels/1/9")
        #expect(unknown[.akariPill] == nil)
        #expect(unknown[.link] as? URL == URL(string: "https://discord.com/channels/1/9"))
    }

    @Test
    func timestampsShowTheirTextAndFullDate() throws {
        let result = built("at <t:1800000000:R> and <t:1800000000:f>")

        let relative = try attributes(result.text, of: "in 5 minutes")
        #expect(relative[.akariPill] as? MessageText.Pill == .timestamp)
        #expect(relative[.toolTip] as? String == times.fullDate(1_800_000_000))
        #expect(result.text.string.contains(times.text(1_800_000_000, style: .longDateShortTime, now: now)))
        #expect(result.hasRelativeTime)
        #expect(result.nextChange == times.nextChange(1_800_000_000, style: .relative, now: now))
        #expect(built("<t:1800000000:f>").nextChange == nil)
    }

    @Test
    func linksAreColoredAndMaskedOnesShowTheirTitleThenTheURL() throws {
        let text = built("[a page](https://example.com/page \"A title\") https://example.org").text

        let masked = try attributes(text, of: "a page")
        #expect(masked[.toolTip] as? String == "A title\n(https://example.com/page)")
        #expect(masked[.link] as? URL == URL(string: "akari://link/0"))
        #expect(masked[.foregroundColor] as? NSColor == Palette.textLink)
        #expect(masked[.underlineStyle] == nil)
        let plain = try attributes(text, of: "https://example.org")
        #expect(plain[.link] as? URL == URL(string: "https://example.org/"))
        #expect(built("[a page](https://example.com/page)").text.attribute(
            .toolTip, at: 0, effectiveRange: nil) as? String == "https://example.com/page")
    }

    @Test
    func onlyUnicodeEmojiAreJumbo() throws {
        let jumbo = built("😀 🎉").text
        #expect(try font(jumbo, of: "🎉").pointSize == 48)
        #expect(try paragraph(jumbo, of: "🎉").minimumLineHeight == 56)

        let custom = built("😀 <:wave:123456789012345678>").text
        #expect(custom.string.contains(":wave:"))
        #expect(try font(custom, of: "😀").pointSize == 16)
    }

    @Test
    func theEditedMarkFollowsTheTextOrAGoesUnderACodeBlock() {
        #expect(built("a", edited: true).text.string == "a (edited)")
        #expect(built("```\nx\n```", edited: true).text.string == "x\n(edited)")
    }

    @Test
    func everyCharacterKnowsItsSource() throws {
        for content in FormattedSamples.contents {
            let text = built(content, edited: true).text
            let length = (content as NSString).length
            var position = 0
            while position < text.length {
                var range = NSRange()
                let source = text.attribute(.akariSource, at: position, effectiveRange: &range)
                let mapped = try #require(source as? NSRange, "\(content.prefix(20)) at \(position)")
                #expect(mapped.location >= 0 && NSMaxRange(mapped) <= length)
                position = NSMaxRange(range)
            }
        }
    }
}
