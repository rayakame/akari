import AppKit

// TextKit 1, so the pills, boxes and covers can be drawn around glyph ranges.
nonisolated final class MessageLayoutManager: NSLayoutManager {
    // Covers go over the glyphs, the pills and the selection.
    override func drawGlyphs(forGlyphRange glyphsToShow: NSRange, at origin: NSPoint) {
        super.drawGlyphs(forGlyphRange: glyphsToShow, at: origin)
        guard let storage = textStorage, let container = textContainers.first else {
            return
        }
        let characters = characterRange(forGlyphRange: glyphsToShow, actualGlyphRange: nil)
        storage.enumerateAttribute(.akariSpoiler, in: characters) { value, range, _ in
            guard let spoiler = value as? MessageText.Spoiler, spoiler.hidden else {
                return
            }
            Palette.spoilerHidden.setFill()
            for rect in rects(range, storage, container, padded: true) {
                NSBezierPath(
                    roundedRect: rect.offsetBy(dx: origin.x, dy: origin.y), xRadius: 3,
                    yRadius: 3
                ).fill()
            }
        }
    }

    private func isCode(_ storage: NSTextStorage, _ index: Int) -> Bool {
        (storage.attribute(.akariBlock, at: index, effectiveRange: nil) as? MessageText.Block)?
            .contains(.code) == true
    }

    /// What goes behind the text: code boxes, quote bars, the covers of hidden code blocks,
    /// revealed spoilers' tint and pills, in the text container's coordinates.
    func blocks() -> [MessageBlockShape] {
        guard let storage = textStorage, let container = textContainers.first else {
            return []
        }
        var shapes: [MessageBlockShape] = []
        var quote: (range: NSRange, bar: CGFloat)?
        storage.enumerateAttribute(
            .akariBlock, in: NSRange(location: 0, length: storage.length)
        ) { value, range, _ in
            let block = value as? MessageText.Block ?? []
            if block.contains(.code), let box = codeBox(range, storage, container) {
                shapes.append(.codeBox(box))
                let spoiler = storage.attribute(.akariSpoiler, at: range.location, effectiveRange: nil)
                if (spoiler as? MessageText.Spoiler)?.hidden == true {
                    shapes.append(.cover(box))
                }
            }
            if block.contains(.quote) {
                let bar = storage.attribute(.akariQuoteBar, at: range.location, effectiveRange: nil)
                quote = quote.map { (NSUnionRange($0.range, range), $0.bar) }
                    ?? (range, bar as? CGFloat ?? 0)
            } else if let ended = quote {
                shapes.append(quoteBar(ended.range, x: ended.bar, storage, container))
                quote = nil
            }
        }
        if let quote {
            shapes.append(quoteBar(quote.range, x: quote.bar, storage, container))
        }
        let whole = NSRange(location: 0, length: storage.length)
        storage.enumerateAttribute(.akariSpoiler, in: whole) { value, range, _ in
            // A code block's spoiler shows its box; no tint over it.
            guard let spoiler = value as? MessageText.Spoiler, !spoiler.hidden,
                !isCode(storage, range.location)
            else {
                return
            }
            for rect in rects(range, storage, container, padded: false) {
                shapes.append(.revealed(rect))
            }
        }
        storage.enumerateAttribute(.akariPill, in: whole) { value, range, _ in
            guard let pill = value as? MessageText.Pill else {
                return
            }
            for rect in rects(range, storage, container, padded: true) {
                shapes.append(.pill(rect, pill))
            }
        }
        return shapes.filter { !$0.rect.isEmpty }
    }

    private func codeBox(_ range: NSRange, _ storage: NSTextStorage, _ container: NSTextContainer)
        -> NSRect?
    {
        let glyphs = glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        guard glyphs.length > 0 else {
            return nil
        }
        let first = lineFragmentUsedRect(forGlyphAt: glyphs.location, effectiveRange: nil)
        let last = lineFragmentUsedRect(forGlyphAt: NSMaxRange(glyphs) - 1, effectiveRange: nil)
        let padding = MessageText.codePadding
        let style = storage.attribute(.paragraphStyle, at: range.location, effectiveRange: nil)
        let left = ((style as? NSParagraphStyle)?.headIndent ?? padding) - padding
        return NSRect(
            x: left, y: first.minY - padding, width: container.size.width - left,
            height: last.maxY - first.minY + 2 * padding)
    }

    private func quoteBar(
        _ range: NSRange, x: CGFloat, _ storage: NSTextStorage, _ container: NSTextContainer
    ) -> MessageBlockShape {
        let glyphs = glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        guard glyphs.length > 0 else {
            return .quoteBar(.zero)
        }
        var top = lineFragmentUsedRect(forGlyphAt: glyphs.location, effectiveRange: nil).minY
        var bottom = lineFragmentUsedRect(forGlyphAt: NSMaxRange(glyphs) - 1, effectiveRange: nil)
            .maxY
        // A quote that starts or ends with a code block reaches the box's edge.
        for index in [range.location, NSMaxRange(range) - 1] {
            var effective = NSRange()
            let block = storage.attribute(.akariBlock, at: index, effectiveRange: &effective)
            if (block as? MessageText.Block)?.contains(.code) == true,
                let box = codeBox(effective, storage, container)
            {
                top = min(top, box.minY)
                bottom = max(bottom, box.maxY)
            }
        }
        return .quoteBar(NSRect(x: x, y: top, width: 4, height: bottom - top))
    }

    // One rect per line the range is on, around its glyphs' ascent and descent. Padded rects
    // take the 2 of kerning before the range; the kerning after it is in the glyphs' advance.
    private func rects(
        _ range: NSRange, _ storage: NSTextStorage, _ container: NSTextContainer, padded: Bool
    ) -> [NSRect] {
        let glyphs = glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        var rects: [NSRect] = []
        enumerateLineFragments(forGlyphRange: glyphs) { line, used, _, lineGlyphs, _ in
            let part = NSIntersectionRange(glyphs, lineGlyphs)
            guard part.length > 0 else {
                return
            }
            let bounds = self.boundingRect(forGlyphRange: part, in: container)
            let baseline = line.minY + self.location(forGlyphAt: part.location).y
            let character = self.characterIndexForGlyph(at: part.location)
            let font = storage.attribute(.font, at: character, effectiveRange: nil) as? NSFont
            let ascent = font?.ascender ?? 0
            let descent = -(font?.descender ?? 0)
            let pad: CGFloat = padded ? 1 : 0
            let top = max(used.minY, baseline - ascent - pad)
            let bottom = min(used.maxY, baseline + descent + pad)
            let left = max(0, bounds.minX - (padded ? 2 : 0))
            let right = min(container.size.width, bounds.maxX)
            rects.append(NSRect(x: left, y: top, width: right - left, height: bottom - top))
        }
        return rects
    }
}

nonisolated enum MessageBlockShape: Equatable {
    case codeBox(NSRect)
    case quoteBar(NSRect)
    case cover(NSRect)
    case revealed(NSRect)
    case pill(NSRect, MessageText.Pill)

    var rect: NSRect {
        switch self {
        case .codeBox(let rect), .quoteBar(let rect), .cover(let rect), .revealed(let rect),
            .pill(let rect, _):
            rect
        }
    }
}
