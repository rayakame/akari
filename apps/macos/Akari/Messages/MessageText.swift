import AkariKit
import AppKit

extension NSAttributedString.Key {
    // The NSRange of `content` a character shows.
    nonisolated static let akariSource = NSAttributedString.Key("akariSource")
    nonisolated static let akariPill = NSAttributedString.Key("akariPill")
    nonisolated static let akariBlock = NSAttributedString.Key("akariBlock")
    // The x of a quote's bar, as an NSNumber.
    nonisolated static let akariQuoteBar = NSAttributedString.Key("akariQuoteBar")
    nonisolated static let akariSpoiler = NSAttributedString.Key("akariSpoiler")
    // A custom emoji's box: `MessageText.EmojiBox` on its attachment character.
    nonisolated static let akariEmoji = NSAttributedString.Key("akariEmoji")
}

/// A message's content as the list shows it.
enum MessageText {
    nonisolated enum Pill: Hashable {
        case mention
        /// A role with a color: the color as text, on the color at 10%.
        case role(UInt32)
        case code
        case timestamp
    }

    nonisolated struct Block: OptionSet, Hashable {
        let rawValue: UInt8
        static let quote = Block(rawValue: 1)
        static let code = Block(rawValue: 2)
    }

    nonisolated struct Spoiler: Hashable {
        let index: UInt16
        let hidden: Bool
    }

    /// A custom emoji in the text (D139): a fixed box the layout manager places an image layer
    /// on; `:name:` is its tooltip, its copy and its fallback.
    nonisolated struct EmojiBox: Hashable {
        let key: ImageKey
        let name: String
        let animated: Bool
        /// The box's side in points: 1.375 × its line's font size, 48 in a jumbo message.
        var side: CGFloat = 22
        /// Centered on its line (jumbo); else its bottom on the line's bottom.
        var centered = false

        var id: EmojiId {
            if case .emoji(let id, _) = key { id } else { EmojiId(rawValue: 0) }
        }
    }

    /// A jumbo message's box side at the 16 pt base; inline boxes are 1.375 × their face's size.
    nonisolated static let emojiJumbo: CGFloat = 48

    struct Built {
        let text: NSAttributedString
        /// When a relative timestamp's text changes next.
        let nextChange: Date?
        let hasRelativeTime: Bool
    }

    nonisolated static let codePadding: CGFloat = 8

    /// Where a plain first line's capitals are centered below the text's top, whatever the text
    /// holds: a follow-up's hover time is centered there.
    static let firstLineMiddle: CGFloat = {
        let view = MessageTextView()
        view.frame = NSRect(x: 0, y: 0, width: 400, height: 100)
        let face = Typeface.body
        let font = face.font(bold: false, italic: false)
        view.show(
            NSAttributedString(
                string: "0",
                attributes: [
                    .font: font,
                    .paragraphStyle: ParagraphKey(line: face.line).style,
                    .baselineOffset: face.offset,
                ]))
        return view.firstBaselineOffsetFromTop - font.capHeight / 2
    }()

    static func build(
        _ message: Message, revealed: Set<UInt16>, now: Date, times: TimestampText
    ) -> Built {
        var builder = Builder(message, revealed: revealed, now: now, times: times)
        return builder.build()
    }

    /// The text a selection shows, as plain text: line breaks as newlines, a custom emoji as
    /// `:name:`, other symbols left out.
    nonisolated static func plain(_ text: NSAttributedString, _ range: NSRange) -> String {
        let part = text.attributedSubstring(from: range)
        let out = NSMutableString(string: part.string)
        var emoji: [(NSRange, String)] = []
        part.enumerateAttribute(.akariEmoji, in: NSRange(location: 0, length: part.length)) {
            value, range, _ in
            if let box = value as? EmojiBox {
                emoji.append((range, String(repeating: ":\(box.name):", count: range.length)))
            }
        }
        for (range, name) in emoji.reversed() {
            out.replaceCharacters(in: range, with: name)
        }
        return (out as String)
            .replacingOccurrences(of: "\u{2028}", with: "\n")
            .replacingOccurrences(of: "\u{FFFC}", with: "")
    }

    /// What VoiceOver reads: a hidden spoiler is "Spoiler", not its text.
    nonisolated static func accessible(_ text: NSAttributedString) -> String {
        var parts: [String] = []
        var lastHidden: UInt16?
        text.enumerateAttribute(
            .akariSpoiler, in: NSRange(location: 0, length: text.length)
        ) { value, range, _ in
            if let spoiler = value as? Spoiler, spoiler.hidden {
                if lastHidden != spoiler.index {
                    parts.append("Spoiler")
                }
                lastHidden = spoiler.index
            } else {
                parts.append(plain(text, range))
                lastHidden = nil
            }
        }
        return parts.joined()
    }

    /// The code box's padding above a first line and below a last one, which TextKit 1's
    /// paragraph spacing leaves out there.
    nonisolated static func padding(_ text: NSAttributedString) -> (top: CGFloat, bottom: CGFloat) {
        guard text.length > 0 else {
            return (0, 0)
        }
        func code(_ index: Int) -> Bool {
            (text.attribute(.akariBlock, at: index, effectiveRange: nil) as? Block)?
                .contains(.code) == true
        }
        return (code(0) ? codePadding : 0, code(text.length - 1) ? codePadding : 0)
    }
}

private final class Typeface {
    let size: CGFloat
    let line: CGFloat
    let offset: CGFloat
    // regular, italic, bold, bold italic, medium, medium italic
    private let fonts: [NSFont]
    // regular, italic, bold, bold italic
    private let mono: [NSFont]
    private let alwaysBold: Bool
    private let monospaced: Bool

    // `offset` and `mono` are in points at the type scale's base, like the style.
    init(
        _ style: TextStyle, offset: CGFloat, mono monoPoints: CGFloat, alwaysBold: Bool = false,
        monospaced: Bool = false
    ) {
        let size = style.size
        let monoSize = TypeScale.size(monoPoints)
        self.size = size
        line = style.line
        self.offset = TypeScale.size(offset)
        self.alwaysBold = alwaysBold
        self.monospaced = monospaced
        func italic(_ font: NSFont, _ traits: NSFontDescriptor.SymbolicTraits) -> NSFont {
            NSFont(descriptor: font.fontDescriptor.withSymbolicTraits(traits), size: font.pointSize)
                ?? font
        }
        let regular = NSFont.systemFont(ofSize: size)
        let bold = NSFont.boldSystemFont(ofSize: size)
        let medium = NSFont.systemFont(ofSize: size, weight: .medium)
        fonts = [
            regular, italic(regular, .italic), bold, italic(bold, [.bold, .italic]), medium,
            italic(medium, .italic),
        ]
        let monoRegular = NSFont.monospacedSystemFont(ofSize: monoSize, weight: .regular)
        let monoBold = NSFont.monospacedSystemFont(ofSize: monoSize, weight: .bold)
        mono = [
            monoRegular, italic(monoRegular, .italic), monoBold,
            italic(monoBold, [.bold, .italic]),
        ]
    }

    func font(bold: Bool, italic: Bool, medium: Bool = false, mono: Bool = false) -> NSFont {
        let bold = bold || alwaysBold
        if mono || monospaced {
            return self.mono[(bold ? 2 : 0) + (italic ? 1 : 0)]
        }
        return fonts[(bold ? 2 : medium ? 4 : 0) + (italic ? 1 : 0)]
    }

    static let body = Typeface(TypeScale.body, offset: 3, mono: 14)
    static let headings = [
        Typeface(TypeScale.heading1, offset: 3, mono: 21, alwaysBold: true),
        Typeface(TypeScale.heading2, offset: 3, mono: 18, alwaysBold: true),
        Typeface(TypeScale.heading3, offset: 3, mono: 14, alwaysBold: true),
    ]
    static let subtext = Typeface(TypeScale.subtext, offset: 2, mono: 11)
    static let code = Typeface(TypeScale.codeBlock, offset: 1, mono: 14, monospaced: true)
    static let jumbo = Typeface(TypeScale.jumbo, offset: 0, mono: 42)
}

private enum Marker: Hashable {
    case none
    case bullet
    case number
}

private struct ParagraphKey: Hashable {
    var line: CGFloat
    var before: CGFloat = 0
    var after: CGFloat = 0
    var head: CGFloat = 0
    var tail: CGFloat = 0
    var marker = Marker.none
    var column: CGFloat = 20

    static var made: [ParagraphKey: NSParagraphStyle] = [:]

    var style: NSParagraphStyle {
        if let style = Self.made[self] {
            return style
        }
        let style = NSMutableParagraphStyle()
        style.minimumLineHeight = line
        style.maximumLineHeight = line
        style.paragraphSpacingBefore = before
        style.paragraphSpacing = after
        style.headIndent = head
        style.firstLineHeadIndent = marker == .none ? head : head - column
        style.tailIndent = tail
        switch marker {
        case .none:
            break
        case .bullet:
            style.tabStops = [
                NSTextTab(textAlignment: .center, location: head - column / 2),
                NSTextTab(textAlignment: .left, location: head),
            ]
        case .number:
            style.tabStops = [
                NSTextTab(textAlignment: .right, location: head - 4),
                NSTextTab(textAlignment: .left, location: head),
            ]
        }
        Self.made[self] = style
        return style
    }
}

private struct BlockStyle {
    let face: Typeface
    let flags: MessageText.Block
    let attributes: [NSAttributedString.Key: Any]
    let marker: String?
    let isCode: Bool
}

private enum Symbols {
    static var made: [String: NSImage] = [:]

    static func image(_ name: String, size: CGFloat, color: NSColor) -> NSImage? {
        let key = "\(name) \(size) \(ObjectIdentifier(color).hashValue)"
        if let image = made[key] {
            return image
        }
        let configuration = NSImage.SymbolConfiguration(pointSize: size * 0.8, weight: .semibold)
            .applying(NSImage.SymbolConfiguration(paletteColors: [color]))
        let image = NSImage(systemSymbolName: name, accessibilityDescription: nil)?
            .withSymbolConfiguration(configuration)
        made[key] = image
        return image
    }
}

private struct Builder {
    private let message: Message
    private let source: NSString
    private let revealed: Set<UInt16>
    private let now: Date
    private let times: TimestampText
    private let color: NSColor
    private let jumbo: Bool
    private let out = NSMutableAttributedString()
    private var nextChange: Date?
    private var hasRelativeTime = false
    // A Discord link shown as a pill: its other spans are left out.
    private var pillLink: UInt16?
    // A list's numbers from 10 up need more than the usual 20 of marker column.
    private let columns: [CGFloat]

    // The channel shows its spoilers (`render_spoilers`), or the user clicked this one.
    private func isRevealed(_ spoiler: UInt16) -> Bool {
        message.spoilersShown || revealed.contains(spoiler)
    }

    init(_ message: Message, revealed: Set<UInt16>, now: Date, times: TimestampText) {
        self.message = message
        source = message.content as NSString
        self.revealed = revealed
        self.now = now
        self.times = times
        color =
            switch message.delivery {
            case .pending: Palette.textMuted
            case .failed: Palette.danger
            default: Palette.textDefault
            }
        columns = Self.columns(message.formatted.blocks)
        jumbo = message.formatted.jumbo
    }

    private static func columns(_ blocks: [TextBlock]) -> [CGFloat] {
        var widest: [UInt32] = []
        for block in blocks {
            var depth = 0
            for container in block.containers {
                guard case .listItem(let ordered, let number, _) = container else {
                    continue
                }
                if widest.count <= depth {
                    widest.append(0)
                }
                if ordered {
                    widest[depth] = max(widest[depth], number)
                }
                depth += 1
            }
        }
        let font = Typeface.body.font(bold: false, italic: false)
        return widest.map { number in
            guard number >= 10 else {
                return 20
            }
            let width = ("\(number)." as NSString).size(withAttributes: [.font: font]).width
            return max(20, ceil(width) + 8)
        }
    }

    mutating func build() -> MessageText.Built {
        let blocks = message.formatted.blocks
        var previous: (block: TextBlock, style: BlockStyle)?
        for (index, block) in blocks.enumerated() {
            let style = style(block, first: index == 0)
            if let previous {
                separate(previous.block, previous.style, style)
            }
            if let marker = style.marker {
                append(marker, style.attributes, source: NSRange(location: Int(block.start), length: 0))
            }
            for span in block.spans {
                append(span, in: block, style)
            }
            previous = (block, style)
        }
        if message.editedTimestamp != nil, !message.content.isEmpty {
            var mark: NSAttributedString = CellText.editedMark()
            if let previous, previous.style.isCode {
                separate(previous.block, previous.style, nil)
                mark = mark.attributedSubstring(from: NSRange(location: 1, length: mark.length - 1))
            }
            out.append(mark)
            out.addAttribute(
                .akariSource, value: NSRange(location: source.length, length: 0),
                range: NSRange(location: out.length - mark.length, length: mark.length))
        }
        return MessageText.Built(
            text: out, nextChange: nextChange, hasRelativeTime: hasRelativeTime)
    }

    private func style(_ block: TextBlock, first: Bool) -> BlockStyle {
        var flags: MessageText.Block = []
        var head: CGFloat = 0
        var bar: CGFloat?
        var level = 0
        var column: CGFloat = 20
        var marker: (ordered: Bool, number: UInt32)?
        for container in block.containers {
            switch container {
            case .quote:
                flags.insert(.quote)
                bar = head
                head += 16
            case .listItem(let ordered, let number, let shows):
                column = level < columns.count ? columns[level] : 20
                level += 1
                head += column
                marker = shows ? (ordered, number) : nil
            }
        }
        let face: Typeface
        var key = ParagraphKey(line: 0)
        var isCode = false
        var color = color
        switch block.kind {
        case .paragraph:
            face = jumbo ? .jumbo : .body
        case .heading(let level):
            face = Typeface.headings[min(max(Int(level), 1), 3) - 1]
            key.before = first ? 0 : 8
            key.after = 4
        case .subtext:
            face = .subtext
            color = Palette.textMuted
        case .code:
            face = .code
            isCode = true
            flags.insert(.code)
            head += MessageText.codePadding
            key.tail = -MessageText.codePadding
            key.before = first ? 0 : MessageText.codePadding + 4
            key.after = MessageText.codePadding + 4
        }
        key.line = face.line
        key.head = head
        var shown: String?
        if let marker {
            key.marker = marker.ordered ? .number : .bullet
            key.column = column
            shown = marker.ordered ? "\t\(marker.number).\t" : level > 1 ? "\t◦\t" : "\t•\t"
        }
        var attributes: [NSAttributedString.Key: Any] = [
            .font: face.font(bold: false, italic: false),
            .foregroundColor: color,
            .paragraphStyle: key.style,
            .baselineOffset: face.offset,
        ]
        if !flags.isEmpty {
            attributes[.akariBlock] = flags
        }
        if let bar {
            attributes[.akariQuoteBar] = bar
        }
        if let spoiler = block.spoiler {
            attributes[.akariSpoiler] = MessageText.Spoiler(
                index: spoiler, hidden: !isRevealed(spoiler))
        }
        return BlockStyle(
            face: face, flags: flags, attributes: attributes, marker: shown, isCode: isCode)
    }

    // The newline ends the earlier block's paragraph; a quote's bar runs on across it.
    private func separate(_ block: TextBlock, _ style: BlockStyle, _ next: BlockStyle?) {
        var attributes = style.attributes
        attributes[.akariSpoiler] = nil
        let quote = style.flags.contains(.quote) && next?.flags.contains(.quote) == true
        attributes[.akariBlock] = quote ? MessageText.Block.quote : nil
        if !quote {
            attributes[.akariQuoteBar] = nil
        }
        append("\n", attributes, source: NSRange(location: Int(block.end), length: 0))
    }

    private func append(
        _ string: String, _ attributes: [NSAttributedString.Key: Any], source: NSRange
    ) {
        var attributes = attributes
        attributes[.akariSource] = source
        out.append(NSAttributedString(string: string, attributes: attributes))
    }

    private mutating func append(_ span: TextSpan, in block: TextBlock, _ style: BlockStyle) {
        let range = NSRange(location: Int(span.start), length: Int(span.end - span.start))
        var attributes = style.attributes
        attributes[.akariSource] = range
        attributes[.font] = style.face.font(bold: span.bold, italic: span.italic)
        if span.underline {
            attributes[.underlineStyle] = NSUnderlineStyle.single.rawValue
        }
        if span.strike {
            attributes[.strikethroughStyle] = NSUnderlineStyle.single.rawValue
        }
        if let spoiler = span.spoiler {
            attributes[.akariSpoiler] = MessageText.Spoiler(
                index: spoiler, hidden: !isRevealed(spoiler))
        }
        let start = out.length
        defer {
            if let spoiler = span.spoiler ?? block.spoiler, !isRevealed(spoiler) {
                hide(NSRange(location: start, length: out.length - start), spoiler)
            }
        }
        if let index = span.link {
            if index == pillLink {
                return
            }
            let link = message.formatted.links[Int(index)]
            if !link.masked, let channel = link.channel, case .known(let name) = channel.name {
                pillLink = index
                channelPill(
                    name, kind: channel.kind, id: channel.channelId,
                    message: channel.messageId != nil, span: span, style, attributes)
                return
            }
            attributes[.foregroundColor] = Palette.textLink
            attributes[.link] = target(link, index: index)
            if link.masked {
                attributes[.toolTip] = link.title.map { "\($0)\n(\(link.url))" } ?? link.url
            }
        }
        switch span.kind {
        case .text(let start, let end):
            append(shown(start, end), attributes)
        case .break:
            append("\u{2028}", attributes)
        case .code(let start, let end):
            if style.isCode {
                append(shown(start, end), attributes)
            } else {
                attributes[.font] = style.face.font(bold: span.bold, italic: span.italic, mono: true)
                pill(shown(start, end), .code, attributes)
            }
        case .user(_, let name):
            if let text = MentionText.user(name) {
                mention(text, placeholder: MentionText.isPlaceholder(name), span, style, attributes)
            } else {
                append(shown(span.start, span.end), attributes)
            }
        case .role(_, let name, let roleColor):
            let text = MentionText.role(name)
            if roleColor != 0, case .known = name {
                attributes[.font] = style.face.font(bold: span.bold, italic: span.italic, medium: true)
                attributes[.foregroundColor] = NSColor(rgb: roleColor)
                pill(text, .role(roleColor), attributes)
            } else {
                mention(text, placeholder: MentionText.isPlaceholder(name), span, style, attributes)
            }
        case .channel(let id, let name, let kind):
            switch name {
            case .known(let shown):
                channelPill(
                    shown, kind: kind, id: id, message: false, span: span, style, attributes)
            case .notLoaded:
                var muted = attributes
                muted[.foregroundColor] = Palette.textMuted
                append(symbol("number", style.face, Palette.textMuted, muted))
                append(MentionText.channel(name), muted)
            case .noAccess, .missing:
                attributes[.font] = style.face.font(bold: span.bold, italic: span.italic, medium: true)
                attributes[.foregroundColor] = Palette.mentionText
                let start = out.length
                append(
                    symbol(
                        name == .noAccess ? "lock" : "number", style.face, Palette.mentionText,
                        attributes))
                append(MentionText.channel(name), attributes)
                pillEnds(start, .mention)
            }
        case .everyone:
            mention("@everyone", placeholder: false, span, style, attributes)
        case .here:
            mention("@here", placeholder: false, span, style, attributes)
        case .command(let name, _):
            mention(MentionText.command(name), placeholder: false, span, style, attributes)
        case .emoji(let name, _, let animated, let key):
            let side = jumbo ? TypeScale.size(MessageText.emojiJumbo) : (style.face.size * 1.375).rounded()
            attributes[.akariEmoji] = MessageText.EmojiBox(
                key: key, name: name, animated: animated, side: side, centered: jumbo)
            attributes[.toolTip] = ":\(name):"
            append(emojiBox(side, style.face, attributes))
        case .timestamp(let seconds, let timestampStyle):
            attributes[.toolTip] = times.fullDate(seconds)
            pill(times.text(seconds, style: timestampStyle, now: now), .timestamp, attributes)
            if timestampStyle == .relative {
                hasRelativeTime = true
                if let next = times.nextChange(seconds, style: timestampStyle, now: now) {
                    nextChange = min(nextChange ?? next, next)
                }
            }
        case .navigation(let target):
            mention(MentionText.navigation(target), placeholder: false, span, style, attributes)
        }
    }

    // Laid out as shown, so revealing moves nothing; the cover is drawn over it, and a click
    // anywhere on it reveals it.
    private func hide(_ range: NSRange, _ spoiler: UInt16) {
        guard range.length > 0 else {
            return
        }
        var attributes: [NSAttributedString.Key: Any] = [
            .foregroundColor: NSColor.clear, .underlineColor: NSColor.clear,
            .strikethroughColor: NSColor.clear,
        ]
        attributes[.link] = URL(string: "akari://spoiler/\(spoiler)")
        out.addAttributes(attributes, range: range)
        out.removeAttribute(.toolTip, range: range)
    }

    private func target(_ link: TextLink, index: UInt16) -> URL? {
        if let channel = link.channel, case .known = channel.name,
            channel.kind?.opensMessageList ?? true
        {
            return URL(string: "akari://channel/\(channel.channelId.rawValue)")
        }
        return link.masked ? URL(string: "akari://link/\(index)") : URL(string: link.url)
    }

    private func shown(_ start: UInt32, _ end: UInt32) -> String {
        let text = source.substring(
            with: NSRange(location: Int(start), length: Int(end) - Int(start)))
        // One paragraph per block, so a block's lines share its indents and spacing.
        guard text.utf8.contains(where: { $0 == 0x0A || $0 == 0x0D || $0 == 0xE2 }) else {
            return text
        }
        return text.replacingOccurrences(of: "\r\n", with: "\u{2028}")
            .replacingOccurrences(of: "\r", with: "\u{2028}")
            .replacingOccurrences(of: "\n", with: "\u{2028}")
            .replacingOccurrences(of: "\u{2029}", with: "\u{2028}")
    }

    private func append(_ string: String, _ attributes: [NSAttributedString.Key: Any]) {
        out.append(NSAttributedString(string: string, attributes: attributes))
    }

    private func append(_ attachment: NSAttributedString) {
        out.append(attachment)
    }

    private func mention(
        _ text: String, placeholder: Bool, _ span: TextSpan, _ style: BlockStyle,
        _ attributes: [NSAttributedString.Key: Any]
    ) {
        var attributes = attributes
        if placeholder {
            attributes[.foregroundColor] = Palette.textMuted
            append(text, attributes)
            return
        }
        attributes[.font] = style.face.font(bold: span.bold, italic: span.italic, medium: true)
        attributes[.foregroundColor] = Palette.mentionText
        pill(text, .mention, attributes)
    }

    private func channelPill(
        _ name: String, kind: ChannelType?, id: ChannelId, message: Bool, span: TextSpan,
        _ style: BlockStyle, _ attributes: [NSAttributedString.Key: Any]
    ) {
        var attributes = attributes
        attributes[.font] = style.face.font(bold: span.bold, italic: span.italic, medium: true)
        attributes[.foregroundColor] = Palette.mentionText
        if kind?.opensMessageList ?? true {
            attributes[.link] = URL(string: "akari://channel/\(id.rawValue)")
            attributes[.toolTip] = nil
        } else {
            attributes[.link] = nil
            attributes[.toolTip] = kind.map(ChannelSymbols.unsupported)
        }
        let start = out.length
        let symbolName = kind.map(ChannelSymbols.name(for:)) ?? "number"
        append(symbol(symbolName, style.face, Palette.mentionText, attributes))
        append(name, attributes)
        if message {
            addKern(at: out.length - 1)
            append(symbol("text.bubble", style.face, Palette.mentionText, attributes))
        }
        pillEnds(start, .mention)
    }

    private func pill(
        _ text: String, _ pill: MessageText.Pill, _ attributes: [NSAttributedString.Key: Any]
    ) {
        let start = out.length
        append(text, attributes)
        pillEnds(start, pill)
    }

    // Kerning on the character before a pill and on its last one makes room for its padding.
    private func pillEnds(_ start: Int, _ pill: MessageText.Pill) {
        guard out.length > start else {
            return
        }
        out.addAttribute(.akariPill, value: pill, range: NSRange(location: start, length: out.length - start))
        if start > 0, ![0x0A, 0x2028].contains(out.mutableString.character(at: start - 1)) {
            addKern(at: start - 1)
        }
        addKern(at: out.length - 1)
    }

    private func addKern(at index: Int) {
        let kern = out.attribute(.kern, at: index, effectiveRange: nil) as? CGFloat ?? 0
        out.addAttribute(.kern, value: kern + 2, range: NSRange(location: index, length: 1))
    }

    // The attachment only makes room: `side` wide, no taller than the font's ascent and on the
    // baseline, so no line grows for it and string drawing measures what the view lays out. The
    // layout manager places the box itself on the line (`MessageLayoutManager.blocks`).
    private func emojiBox(_ side: CGFloat, _ face: Typeface, _ attributes: [NSAttributedString.Key: Any])
        -> NSAttributedString
    {
        let attachment = NSTextAttachment()
        let font = face.font(bold: false, italic: false)
        attachment.bounds = NSRect(x: 0, y: 0, width: side, height: side)
        attachment.attachmentCell = EmojiCell(
            size: NSSize(width: side, height: min(side, font.ascender.rounded(.down))))
        let box = NSMutableAttributedString(attachment: attachment)
        box.addAttributes(attributes, range: NSRange(location: 0, length: box.length))
        return box
    }

    private func symbol(
        _ name: String, _ face: Typeface, _ color: NSColor,
        _ attributes: [NSAttributedString.Key: Any]
    ) -> NSAttributedString {
        let attachment = NSTextAttachment()
        if let image = Symbols.image(name, size: face.size, color: color) {
            attachment.image = image
            let font = face.font(bold: false, italic: false)
            attachment.bounds = NSRect(
                x: 0, y: ((font.capHeight - image.size.height) / 2).rounded(),
                width: image.size.width, height: image.size.height)
        }
        let symbol = NSMutableAttributedString(attachment: attachment)
        symbol.addAttributes(attributes, range: NSRange(location: 0, length: symbol.length))
        return symbol
    }
}

// TextKit 1 gives an attachment without an image or a cell a 1 pt advance; this cell takes the
// box's size and draws nothing, so the text moves over and string drawing measures the same.
private final class EmojiCell: NSTextAttachmentCell {
    nonisolated private let size: NSSize

    init(size: NSSize) {
        self.size = size
        super.init(textCell: "")
    }

    required init(coder: NSCoder) {
        size = .zero
        super.init(coder: coder)
    }

    nonisolated override func cellSize() -> NSSize {
        size
    }

    nonisolated override func cellBaselineOffset() -> NSPoint {
        .zero
    }

    nonisolated override func draw(withFrame cellFrame: NSRect, in controlView: NSView?) {}

    nonisolated override func draw(
        withFrame cellFrame: NSRect, in controlView: NSView?, characterIndex charIndex: Int
    ) {}

    nonisolated override func draw(
        withFrame cellFrame: NSRect, in controlView: NSView?, characterIndex charIndex: Int,
        layoutManager: NSLayoutManager
    ) {}

    nonisolated override func highlight(_ flag: Bool, withFrame cellFrame: NSRect, in controlView: NSView?) {}

    // A drag from the emoji selects text, as from any character, instead of dragging the
    // attachment out.
    nonisolated override func wantsToTrackMouse() -> Bool {
        false
    }

    nonisolated override func wantsToTrackMouse(
        for theEvent: NSEvent, in cellFrame: NSRect, of controlView: NSView?, atCharacterIndex charIndex: Int
    ) -> Bool {
        false
    }
}

extension NSColor {
    nonisolated convenience init(rgb: UInt32, alpha: CGFloat = 1) {
        self.init(
            srgbRed: CGFloat((rgb >> 16) & 0xff) / 255, green: CGFloat((rgb >> 8) & 0xff) / 255,
            blue: CGFloat(rgb & 0xff) / 255, alpha: alpha)
    }
}
