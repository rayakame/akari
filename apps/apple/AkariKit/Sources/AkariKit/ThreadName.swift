import Foundation

/// The name a thread started from a message gets when the user leaves the name empty, as
/// Discord's client derives it (canary 649224, module 659617).
public enum ThreadName {
    private static let limit = 40
    // Whitespace, or punctuation Discord's client splits words at.
    private static let separators = try? NSRegularExpression(
        pattern: #"(?:\s|[!@#$%^&*()_\-+={}\[\]:";'<>?,./])+"#)

    /// The first embed's title cut at 40, else the message's first line as text, without its
    /// leading spaces, `#` and `-`, as many whole words as fit in 40 (a separator after the last
    /// one stays); a first word longer than that is cut at 40 with "...". Lengths count UTF-16
    /// units, as Discord does. Empty when nothing's left.
    public static func derived(from message: Message, times: TimestampText = TimestampText(locale: .current, timeZone: .current, calendar: .current))
        -> String
    {
        if let title = message.embedTitle, !title.isEmpty {
            return cut(title)
        }
        let text = plainText(of: message, times: times)
        let line = String(text.prefix { $0 != "\n" })
            .replacingOccurrences(of: "\r", with: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return cut(wholeWords(String(line.drop { " #-".contains($0) })))
    }

    // Discord's client leaves the markdown as typed and spells out mentions, custom emoji,
    // spoilers and timestamps; it wraps mentions in directional isolates, which Akari leaves out.
    static func plainText(of message: Message, times: TimestampText) -> String {
        let content = message.content as NSString
        var replacements: [(range: NSRange, text: String)] = []
        var spoilers: [UInt16: NSRange] = [:]
        for block in message.formatted.blocks {
            for span in block.spans {
                let range = NSRange(
                    location: Int(span.start), length: Int(span.end) - Int(span.start))
                guard NSMaxRange(range) <= content.length else {
                    continue
                }
                if let spoiler = span.spoiler ?? block.spoiler {
                    spoilers[spoiler] = spoilers[spoiler].map { NSUnionRange($0, range) } ?? range
                } else if let text = text(of: span.kind, times: times) {
                    replacements.append((range, text))
                }
            }
        }
        for range in spoilers.values {
            replacements.append((delimited(range, in: content), "<spoiler>"))
        }
        let text = NSMutableString(string: content)
        var applied = content.length
        for replacement in replacements.sorted(by: { $0.range.location > $1.range.location })
        where NSMaxRange(replacement.range) <= applied {
            text.replaceCharacters(in: replacement.range, with: replacement.text)
            applied = replacement.range.location
        }
        return text as String
    }

    private static func text(of kind: SpanKind, times: TimestampText) -> String? {
        switch kind {
        case .user(_, .known(let name)), .role(_, .known(let name), _):
            return "@\(name)"
        case .channel(_, .known(let name), let kind):
            switch kind {
            case .publicThread?, .privateThread?, .newsThread?, .guildVoice?, .guildStageVoice?,
                .guildCategory?:
                return "#\"\(name)\""
            default:
                return "#\(name)"
            }
        case .emoji(let name, _, _):
            return ":\(name):"
        case .timestamp(let seconds, let style):
            return times.text(seconds, style: style, now: Date())
        default:
            return nil
        }
    }

    // A spoiler's spans lie between its `||`.
    private static func delimited(_ range: NSRange, in content: NSString) -> NSRange {
        var range = range
        if range.location >= 2,
            content.substring(with: NSRange(location: range.location - 2, length: 2)) == "||"
        {
            range = NSRange(location: range.location - 2, length: range.length + 2)
        }
        if NSMaxRange(range) + 2 <= content.length,
            content.substring(with: NSRange(location: NSMaxRange(range), length: 2)) == "||"
        {
            range.length += 2
        }
        return range
    }

    private static func wholeWords(_ text: String) -> String {
        let string = text as NSString
        var tokens: [String] = []
        var location = 0
        while true {
            let rest = NSRange(location: location, length: string.length - location)
            guard let match = separators?.firstMatch(in: text, range: rest) else {
                tokens.append(string.substring(with: rest))
                break
            }
            tokens.append(
                string.substring(
                    with: NSRange(location: location, length: match.range.location - location)))
            tokens.append(string.substring(with: match.range))
            location = NSMaxRange(match.range)
        }
        var name = tokens[0]
        for token in tokens.dropFirst() {
            let longer = name + token
            guard (longer as NSString).length <= limit else {
                break
            }
            name = longer
        }
        return name
    }

    // Never inside a character, where Discord's cut could split one.
    private static func cut(_ text: String) -> String {
        let string = text as NSString
        guard string.length > limit else {
            return text
        }
        let end = string.rangeOfComposedCharacterSequence(at: limit).location
        return string.substring(to: end) + "..."
    }
}
