import AppKit
import Testing

func index(of part: String, in text: NSTextView) throws -> Int {
    let range = (text.string as NSString).range(of: part)
    try #require(range.location != NSNotFound)
    return range.location
}

func attribute(_ key: NSAttributedString.Key, of part: String, in text: NSTextView) throws
    -> Any?
{
    text.textStorage?.attribute(key, at: try index(of: part, in: text), effectiveRange: nil)
}

func changedPixels(in rect: NSRect, _ one: NSBitmapImageRep, _ other: NSBitmapImageRep)
    -> Int
{
    var changed = 0
    for x in Int(rect.minX * 2)..<Int(rect.maxX * 2) {
        for y in Int(rect.minY * 2)..<Int(rect.maxY * 2)
        where one.colorAt(x: x, y: y) != other.colorAt(x: x, y: y) {
            changed += 1
        }
    }
    return changed
}

// The middle of `part`'s glyphs in the text view's coordinates.
func middle(of part: String, in text: NSTextView) throws -> NSPoint {
    let rect = try glyphs(of: part, in: text, within: text)
    return NSPoint(x: rect.midX, y: text.isFlipped ? rect.midY : text.bounds.height - rect.midY)
}
