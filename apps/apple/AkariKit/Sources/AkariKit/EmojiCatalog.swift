import CoreText
import Foundation

/// Unicode's emoji as Akari offers them on this system: never one the system's emoji font can't
/// draw.
public enum EmojiCatalog {
    /// What macOS 14, the oldest system Akari runs on, draws.
    public static let drawnEverywhere = EmojiVersion(major: 15, minor: 0)

    /// The newest emoji version offered here, which every read of Unicode emoji takes:
    /// `drawnEverywhere`, then each later version whose emoji the system's font all draws as one
    /// picture.
    public static let newest = newest(of: catalog.flatMap(\.emojis), draws: drawsAsOnePicture)

    /// The picker's Unicode emoji up to `newest`, by group in Unicode's order.
    public static let groups: [UnicodeEmojiGroup] = catalog.compactMap { group in
        let kept = group.emojis.filter { $0.version <= newest }
        return kept.isEmpty ? nil : UnicodeEmojiGroup(group: group.group, emojis: kept)
    }

    private static let catalog = unicodeEmoji(newest: newestEmojiVersion())

    static func newest(of catalog: [UnicodeEmoji], draws: (String) -> Bool) -> EmojiVersion {
        let later = Dictionary(grouping: catalog.filter { $0.version > drawnEverywhere }, by: \.version)
        var newest = drawnEverywhere
        // A version the font misses ends it: an emoji of a later one may build on its emoji.
        for version in later.keys.sorted() {
            guard later[version]?.allSatisfy({ draws($0.emoji) }) == true else {
                break
            }
            newest = version
        }
        return newest
    }

    /// Whether the system's emoji font draws `emoji` as one picture: every glyph is the emoji
    /// font's and exists, and only one of them advances (Apple layers some sequences).
    public static func drawsAsOnePicture(_ emoji: String) -> Bool {
        let font = CTFontCreateUIFontForLanguage(.system, 16, nil)
        let text = NSAttributedString(
            string: emoji, attributes: [NSAttributedString.Key(kCTFontAttributeName as String): font as Any])
        let line = CTLineCreateWithAttributedString(text)
        var pictures = 0
        for run in CTLineGetGlyphRuns(line) as? [CTRun] ?? [] {
            let attributes = CTRunGetAttributes(run) as NSDictionary
            guard let value = attributes[kCTFontAttributeName] as AnyObject?,
                CFGetTypeID(value) == CTFontGetTypeID(),
                CTFontGetSymbolicTraits(unsafeDowncast(value, to: CTFont.self)).contains(.traitColorGlyphs)
            else {
                return false
            }
            let count = CTRunGetGlyphCount(run)
            var glyphs = [CGGlyph](repeating: 0, count: count)
            var advances = [CGSize](repeating: .zero, count: count)
            CTRunGetGlyphs(run, CFRange(location: 0, length: 0), &glyphs)
            CTRunGetAdvances(run, CFRange(location: 0, length: 0), &advances)
            guard !glyphs.contains(0) else {
                return false
            }
            pictures += advances.filter { $0.width > 0 }.count
        }
        return pictures == 1
    }
}

extension EmojiVersion: Comparable {
    public static func < (lhs: EmojiVersion, rhs: EmojiVersion) -> Bool {
        (lhs.major, lhs.minor) < (rhs.major, rhs.minor)
    }
}
