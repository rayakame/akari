import AkariKit
import AppKit
import Testing

@testable import Akari

func pill(_ emoji: String, _ count: UInt32 = 1, mine: Bool = false) -> ReactionPill {
    ReactionPill(emoji: .unicode(emoji), normal: count, mine: mine)
}

func customPill(_ raw: UInt64, _ name: String = "wave", _ count: UInt32 = 1, animated: Bool = false)
    -> ReactionPill
{
    ReactionPill(
        emoji: ReactionEmoji(
            id: EmojiId(rawValue: raw), name: name, animated: animated,
            key: .emoji(emojiId: EmojiId(rawValue: raw), animated: animated)),
        normal: count, mine: false)
}

func reactions(_ pills: ReactionPill..., adds: Bool = true, toggles: Bool = true) -> ReactionRow {
    ReactionRow(pills: pills, adds: adds, toggles: toggles)
}

@MainActor
struct ReactionLayoutTests {
    @Test
    func aPillIsItsEmojiAndTwelvePointsPerDigit() {
        #expect(ReactionPillButton.width(count: 1) == 52)
        #expect(ReactionPillButton.width(count: 27) == 64)
        #expect(ReactionPillButton.width(count: 1234) == 88)
        #expect(ReactionPillButton.width(characters: 0) == 34)
        #expect(ReactionAddButton.width == 34)
    }

    @Test
    func pillsStandSideBySideFourApartOneUnderTheRowsTop() {
        let frames = ReactionRowView.frames([52, 64, 34], width: 400)

        #expect(
            frames == [
                NSRect(x: 0, y: 1, width: 52, height: 30), NSRect(x: 56, y: 1, width: 64, height: 30),
                NSRect(x: 124, y: 1, width: 34, height: 30),
            ])
    }

    @Test
    func aPillThatDoesntFitStartsALineThirtyFourBelow() {
        let frames = ReactionRowView.frames([52, 52, 52, 34], width: 120)

        #expect(frames.map(\.origin) == [
            NSPoint(x: 0, y: 1), NSPoint(x: 56, y: 1), NSPoint(x: 0, y: 35), NSPoint(x: 56, y: 35),
        ])
    }

    @Test
    func aPillExactlyAsWideAsWhatIsLeftStays() {
        let frames = ReactionRowView.frames([52, 52], width: 108)

        #expect(frames.map(\.minY) == [1, 1])
    }

    @Test
    func aPillWiderThanTheColumnHasALineOfItsOwn() {
        let frames = ReactionRowView.frames([52, 88, 52], width: 40)

        #expect(frames.map(\.minY) == [1, 35, 69])
        #expect(frames.map(\.minX) == [0, 0, 0])
    }

    @Test(arguments: [(400.0, 1), (120.0, 2), (60.0, 4)])
    func theRowIsThirtyFourPerLineLessOne(width: Double, lines: Int) {
        let row = reactions(pill("👍"), pill("🎉"), pill("👀"))

        #expect(ReactionRowView.height(row, width: width) == CGFloat(34 * lines - 1))
        #expect(ReactionRowView.height(row, width: width, topInset: 2) == CGFloat(34 * lines + 1))
    }

    @Test
    func theAddButtonCountsOnlyWhereTheUserMayReact() {
        let without = reactions(pill("👍"), pill("🎉"), adds: false)
        let with = reactions(pill("👍"), pill("🎉"))

        #expect(ReactionRowView.height(without, width: 108) == 33)
        #expect(ReactionRowView.height(with, width: 108) == 67)
    }
}

@MainActor
struct ReactionRowViewTests {
    func view(_ row: ReactionRow, width: CGFloat = 400) -> ReactionRowView {
        let view = ReactionRowView()
        view.translatesAutoresizingMaskIntoConstraints = true
        view.preferredWidth = width
        view.configure(row)
        view.frame = NSRect(x: 0, y: 0, width: width, height: view.intrinsicContentSize.height)
        view.layoutSubtreeIfNeeded()
        return view
    }

    @Test
    func theRowShowsItsPillsAndTheAddButtonAtItsEnd() {
        let row = view(reactions(pill("👍", 2), pill("🎉", 12, mine: true)))

        #expect(row.pills.map(\.countText) == ["2", "12"])
        #expect(row.pills.map(\.frame) == [
            NSRect(x: 0, y: 1, width: 52, height: 30), NSRect(x: 56, y: 1, width: 64, height: 30),
        ])
        #expect(!row.add.isHidden)
        #expect(row.add.frame == NSRect(x: 124, y: 1, width: 34, height: 30))
        #expect(row.intrinsicContentSize.height == 33)
    }

    @Test
    func withoutThePermissionThereIsNoAddButtonAndThePillsOnlyShow() {
        let row = view(reactions(pill("👍"), adds: false, toggles: false))

        #expect(row.add.isHidden)
        #expect(row.pills.map(\.isEnabled) == [false])
        #expect(row.pills[0].accessibilityLabel() == "👍, 1 reaction", "no press is promised")
        #expect(row.hitTest(NSPoint(x: 130, y: 10)) == nil)

        row.configure(reactions(pill("👍"), adds: false, toggles: true))
        #expect(row.pills[0].accessibilityLabel() == "👍, 1 reaction, press to react")
    }

    @Test
    func aPillThatStaysKeepsItsViewAndANewOneComesAtItsPlace() {
        let row = view(reactions(pill("👍", 2), pill("🎉")))
        let thumbs = row.pills[0]

        row.configure(reactions(pill("👀"), pill("👍", 3)))
        row.layoutSubtreeIfNeeded()

        #expect(row.pills[1] === thumbs)
        #expect(row.pills.map(\.countText) == ["1", "3"])
        #expect(row.pills.map(\.frame.minX) == [0, 56])
        #expect(row.subviews.filter { $0 is ReactionPillButton && $0 !== row.offer }.count == 2)
    }

    @Test
    func theSpaceBetweenPillsIsNotThePills() {
        let row = view(reactions(pill("👍"), pill("🎉")))

        #expect(row.hitTest(NSPoint(x: 10, y: 10)) === row.pills[0])
        #expect(row.hitTest(NSPoint(x: 54, y: 10)) == nil)
        #expect(row.hitTest(NSPoint(x: 60, y: 10)) === row.pills[1])
        #expect(row.hitTest(NSPoint(x: 115, y: 10)) === row.add)
    }

    @Test
    func aClickNamesItsPillAndTheAddButtonItself() {
        let row = view(reactions(pill("👍"), pill("🎉", mine: true)))
        var clicked: [String] = []
        var added: [NSView] = []
        row.onPill = { clicked.append("\($0.name)\($0.mine ? " mine" : "")") }
        row.onAdd = { added.append($0) }

        row.pills[1].performClick(nil)
        row.add.performClick(nil)

        #expect(clicked == ["🎉 mine"])
        #expect(added.first === row.add)
    }

    @Test
    func aUnicodeEmojiIsDrawnAndACustomOneComesFromTheImageTier() {
        let row = view(reactions(pill("👍"), customPill(7, "party", animated: true)))

        #expect(row.pills[0].picture.isHidden)
        #expect(row.pills[0].picture.content == nil)
        let custom = row.pills[1].picture
        #expect(!custom.isHidden)
        #expect(custom.content?.key == .emoji(emojiId: EmojiId(rawValue: 7), animated: true))
        #expect(custom.fallbackText == ":party:")
        #expect(custom.frame == NSRect(x: 7, y: 5, width: 20, height: 20))
    }

    @Test
    func customEmojiPlayWhenTheListSaysSo() {
        let row = view(reactions(customPill(7, animated: true)))
        #expect(!row.pills[0].picture.animates)

        row.animatesEmoji = true
        #expect(row.pills[0].picture.animates)

        row.configure(reactions(customPill(7, animated: true), customPill(8, animated: true)))
        #expect(row.pills.map(\.picture.animates) == [true, true])
    }

    @Test
    func tooltipsAreReadWhenTheRowShowsAndWhenAsked() {
        let row = ReactionRowView()
        var text = "👍 reacted by 2 people"
        row.tooltip = { _ in text }
        row.configure(reactions(pill("👍", 2)))
        #expect(row.pills[0].toolTip == "👍 reacted by 2 people")

        text = "👍 reacted by Ann and Bo"
        row.refreshTooltips()
        #expect(row.pills[0].toolTip == "👍 reacted by Ann and Bo")
    }

    @Test
    func aKeptPillShowsItsEmojiAsItIsNow() {
        let row = view(reactions(customPill(7, "party")))
        let kept = row.pills[0]

        row.configure(reactions(customPill(7, "party", animated: true)))

        #expect(row.pills[0] === kept)
        #expect(kept.picture.content?.key == .emoji(emojiId: EmojiId(rawValue: 7), animated: true))
    }

    @Test
    func aPostBarsRestComesAfterItsPillsAndSaysHowManyMore() {
        let row = view(reactions(pill("👍"), pill("🎉"), adds: false))
        row.configure(row.row, offer: nil, more: 3, animated: false)
        row.layoutSubtreeIfNeeded()

        #expect(row.more.frame.minX == row.pills[1].frame.maxX + 4)
        #expect(abs(row.more.frame.width - (2 * 7 + 2 * 12)) < 0.01, "+3 is two characters")
        #expect(row.more.accessibilityLabel() == "3 more reactions")
        #expect(row.contentWidth == row.more.frame.maxX)
    }

    @Test
    func theForumsDefaultReactionPromisesAPress() {
        let row = view(reactions(pill("👍"), adds: false))

        row.configure(nil, offer: .unicode("🔥"), more: 0, animated: false)

        #expect(!row.offer.isHidden)
        #expect(row.offer.accessibilityLabel() == "🔥, press to react")
    }

    @Test
    func voiceOverReachesThePillsAndTheAddButton() {
        let row = view(reactions(pill("👍"), pill("🎉")))

        let children = row.accessibilityChildren()?.compactMap { $0 as? NSView } ?? []

        #expect(children.count == 3)
        #expect(children.first === row.pills[0])
        #expect(children.last === row.add)
    }

    @Test
    func aTopInsetSetLaterMovesThePills() {
        let row = view(reactions(pill("👍")))

        row.topInset = 2
        row.layoutSubtreeIfNeeded()

        #expect(row.pills[0].frame.minY == 3)
        #expect(row.intrinsicContentSize.height == 35)
    }

    @Test
    func voiceOverReadsEachPillAndTheGroup() {
        let row = view(reactions(pill("👍", 2), pill("🎉", mine: true)))

        #expect(row.accessibilityRole() == .group)
        #expect(row.pills.map { $0.accessibilityLabel() } == [
            "👍, 2 reactions, press to react", "🎉, 1 reaction, press to remove your reaction",
        ])
        #expect(row.add.accessibilityLabel() == "Add Reaction")
        #expect(row.add.toolTip == "Add Reaction")
    }

    // MARK: Colors

    func resolved(_ color: NSColor, _ name: NSAppearance.Name) -> NSColor {
        var resolved = color
        NSAppearance(named: name)?.performAsCurrentDrawingAppearance {
            resolved = color.usingColorSpace(.sRGB) ?? color
        }
        return resolved
    }

    // As sRGB components: the layer's color, or a color resolved in a view's appearance.
    func parts(_ color: CGColor?) -> [CGFloat] {
        guard let color, let shown = NSColor(cgColor: color)?.usingColorSpace(.sRGB) else {
            return []
        }
        return [shown.redComponent, shown.greenComponent, shown.blueComponent, shown.alphaComponent]
    }

    func parts(_ color: NSColor, in name: NSAppearance.Name) -> [CGFloat] {
        var shown = CGColor.clear
        NSAppearance(named: name)?.performAsCurrentDrawingAppearance { shown = color.cgColor }
        return parts(shown)
    }

    func near(_ one: [CGFloat], _ other: [CGFloat]) -> Bool {
        one.count == 4 && other.count == 4 && zip(one, other).allSatisfy { abs($0 - $1) < 0.01 }
    }

    // Discord's colors: #97979f at the alpha of each state, the brand's #5865f2.
    func grey(_ alpha: CGFloat) -> NSColor { NSColor(hex: "#97979f", alpha: alpha) }

    func drawn(_ pill: ReactionPillButton) -> [[CGFloat]] {
        [parts(pill.layer?.backgroundColor), parts(pill.layer?.borderColor), parts(pill.countLayer.foregroundColor)]
    }

    func shown(_ colors: (fill: NSColor, border: NSColor, count: NSColor), _ name: NSAppearance.Name) -> [[CGFloat]] {
        [parts(colors.fill, in: name), parts(colors.border, in: name), parts(colors.count, in: name)]
    }

    func same(_ one: [[CGFloat]], _ other: [[CGFloat]]) -> Bool {
        one.count == other.count && zip(one, other).allSatisfy { near($0, $1) || ($0.last == 0 && $1.last == 0) }
    }

    // The layers follow the pointer, the user's own reaction and the permission as they change.
    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func thePillsLayersFollowEveryChangeOfItsState(_ name: NSAppearance.Name) throws {
        let row = view(reactions(pill("👍"), pill("🎉")))
        row.appearance = NSAppearance(named: name)
        row.pills.forEach { $0.viewDidChangeEffectiveAppearance() }
        let plain = row.pills[0]
        let dark = name == .darkAqua
        let rest = shown((grey(0.12), .clear, NSColor(hex: dark ? "#abacb2" : "#595a63")), name)
        #expect(same(drawn(plain), rest))

        plain.isHovered = true
        let lit = shown(
            (grey(0.16), grey(dark ? 0.04 : 0.20), NSColor(hex: dark ? "#fbfbfb" : "#28282d")), name)
        #expect(same(drawn(plain), lit))

        row.configure(reactions(pill("👍"), pill("🎉"), toggles: false))
        #expect(same(drawn(plain), rest), "a pill that only shows doesn't light up")
        row.configure(reactions(pill("👍"), pill("🎉")))
        #expect(same(drawn(plain), lit))

        row.configure(reactions(pill("👍", mine: true), pill("🎉")))
        let mine = shown(
            (NSColor(hex: "#5865f2", alpha: 0.24), NSColor(hex: "#5865f2"), NSColor(hex: dark ? "#dfe4ff" : "#273478")),
            name)
        #expect(same(drawn(plain), mine))
    }

    func mouse(_ type: NSEvent.EventType, at point: NSPoint, in window: NSWindow) throws -> NSEvent {
        try #require(
            NSEvent.mouseEvent(
                with: type, location: point, modifierFlags: [], timestamp: 0,
                windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                pressure: type == .leftMouseUp ? 0 : 1))
    }

    // AppKit tracks a button's press in a loop of its own: the release is waiting in the queue.
    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func aPressedPillHasThePressedLookUntilItIsReleased(_ name: NSAppearance.Name) throws {
        let window = TestWindow.make(size: NSSize(width: 400, height: 40))
        window.appearance = NSAppearance(named: name)
        let row = view(reactions(pill("👍")))
        window.contentView?.addSubview(row)
        let thumbs = row.pills[0]
        var pressed: [[CGFloat]] = []
        row.onPill = { _ in pressed = drawn(thumbs) }
        let inside = thumbs.convert(NSPoint(x: 10, y: 10), to: nil)

        NSApp.postEvent(try mouse(.leftMouseUp, at: inside, in: window), atStart: false)
        thumbs.mouseDown(with: try mouse(.leftMouseDown, at: inside, in: window))

        let dark = name == .darkAqua
        let look = shown(
            (grey(dark ? 0.20 : 0.24), grey(dark ? 0.44 : 0.52), NSColor(hex: dark ? "#fbfbfb" : "#28282d")), name)
        #expect(same(pressed, look), "\(pressed)")
        #expect(near(drawn(thumbs)[0], parts(grey(0.12), in: name)), "back at rest")
    }

    @Test
    func aPillThatOnlyShowsTakesNoPress() throws {
        let window = TestWindow.make(size: NSSize(width: 400, height: 40))
        let row = view(reactions(pill("👍"), toggles: false))
        window.contentView?.addSubview(row)
        let thumbs = row.pills[0]
        var clicked = 0
        row.onPill = { _ in clicked += 1 }

        thumbs.mouseDown(with: try mouse(.leftMouseDown, at: thumbs.convert(NSPoint(x: 10, y: 10), to: nil), in: window))

        #expect(clicked == 0)
        #expect(thumbs.layer?.animation(forKey: "backgroundColor") == nil, "no pressed look flashes")
    }

    @Test
    func aNewPillsCountDoesntRollIn() {
        let row = view(reactions(pill("👍", 2)))
        row.rolls = { true }

        row.configure(reactions(pill("👍", 2), pill("🎉", 3)), animated: true)

        #expect(row.pills[1].countText == "3")
        #expect(!row.pills[1].isRolling)
    }

    // Discord's count: 14 semibold, 6 after the emoji, its digits' middle on the pill's.
    @Test
    func theCountIsFourteenSemiboldBesideTheEmoji() throws {
        let row = view(reactions(pill("👍", 2)))
        let thumbs = row.pills[0]
        let font = ReactionPillButton.countFont
        let traits = font.fontDescriptor.object(forKey: .traits) as? [NSFontDescriptor.TraitKey: Any]
        let weight = try #require(traits?[.weight] as? CGFloat)

        #expect(font.pointSize == TypeScale.size(14))
        #expect(abs(weight - NSFont.Weight.semibold.rawValue) < 0.01)
        let count = try #require(thumbs.layer).convert(thumbs.countLayer.frame, from: thumbs.countLayer.superlayer)
        #expect(abs(count.minX - (7 + 20 + 6)) < 0.01)
        let digits = count.minY + (font.ascender - font.capHeight) + font.capHeight / 2
        #expect(abs(digits - thumbs.bounds.midY) < 0.5, "\(digits)")
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func aPillHasTheOfficialColorsInEachState(_ name: NSAppearance.Name) throws {
        let row = view(reactions(pill("👍"), pill("🎉", mine: true)))
        row.appearance = NSAppearance(named: name)
        let plain = row.pills[0]
        let mine = row.pills[1]

        #expect(plain.colors.fill == Palette.hoverBackground)
        #expect(plain.colors.border == .clear)
        #expect(plain.colors.count == Palette.interactiveText)
        plain.isHovered = true
        #expect(plain.colors.fill == Palette.interactiveBackgroundActive)
        #expect(plain.colors.border == Palette.borderMuted)
        #expect(plain.colors.count == Palette.interactiveTextActive)

        #expect(near(parts(mine.colors.fill, in: name), parts(NSColor(hex: "#5865f2", alpha: 0.24), in: name)))
        #expect(resolved(mine.colors.border, name) == resolved(NSColor(hex: "#5865f2"), name))
        #expect(
            resolved(mine.colors.count, name)
                == resolved(NSColor(hex: name == .darkAqua ? "#dfe4ff" : "#273478"), name))
        let layer = try #require(mine.layer)
        #expect(layer.borderWidth == 1)
        #expect(layer.cornerRadius == 8)
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theAddButtonsBorderUnderThePointerShowsInBothThemes(_ name: NSAppearance.Name) throws {
        let row = view(reactions(pill("👍")))
        row.appearance = NSAppearance(named: name)
        row.add.viewDidChangeEffectiveAppearance()
        #expect(row.add.layer?.borderColor?.alpha == 0)

        row.add.isHovered = true

        let border = try #require(row.add.layer?.borderColor.flatMap(NSColor.init(cgColor:)))
        let shown = try #require(border.usingColorSpace(.sRGB))
        let token = resolved(Palette.borderNormal, name)
        #expect(abs(shown.alphaComponent - token.alphaComponent) < 0.01)
        #expect(abs(shown.redComponent - token.redComponent) < 0.01)
        #expect(shown.alphaComponent >= 0.2)
        // Against the button's own fill: white on the light theme's fill shows nothing.
        let fill = try #require(
            row.add.layer?.backgroundColor.flatMap(NSColor.init(cgColor:))?.usingColorSpace(.sRGB))
        #expect(abs(shown.brightnessComponent - fill.brightnessComponent) > 0.2)
    }

    @Test
    func theAddButtonIsASmilingFaceThatLightsUnderThePointer() throws {
        let row = view(reactions(pill("👍")))
        row.appearance = NSAppearance(named: .darkAqua)
        row.add.viewDidChangeEffectiveAppearance()
        let face = NSImage(systemSymbolName: "face.smiling", accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 15, weight: .medium))
        #expect(row.add.image?.tiffRepresentation == face?.tiffRepresentation)
        #expect(near(parts(row.add.layer?.backgroundColor), parts(NSColor(hex: "#1a1a1e"), in: .darkAqua)))
        #expect(row.add.contentTintColor == Palette.interactiveText)

        row.add.isHovered = true

        #expect(near(parts(row.add.layer?.backgroundColor), parts(NSColor(hex: "#202024"), in: .darkAqua)))
        #expect(row.add.contentTintColor == Palette.interactiveTextActive)
    }

    @Test
    func myOwnPillDoesntChangeUnderThePointer() {
        let row = view(reactions(pill("🎉", mine: true)))
        let mine = row.pills[0]
        let before = mine.colors

        mine.isHovered = true

        #expect(mine.colors.fill == before.fill)
        #expect(mine.colors.border == before.border)
        #expect(mine.colors.count == before.count)
    }

    @Test
    func aPillThatOnlyShowsDoesntLightUp() {
        let row = view(reactions(pill("👍"), toggles: false))

        row.pills[0].isHovered = true

        #expect(row.pills[0].colors.fill == Palette.hoverBackground)
    }

    // MARK: The count's roll

    @Test
    func aHigherCountRollsUpAndALowerOneDown() throws {
        let row = view(reactions(pill("👍", 2)))
        row.rolls = { true }
        let thumbs = row.pills[0]

        row.configure(reactions(pill("👍", 3)), animated: true)
        let up = try #require(thumbs.countLayer.animation(forKey: ReactionPillButton.rollKey) as? CABasicAnimation)
        let leaving = try #require(thumbs.ghostLayer.animation(forKey: ReactionPillButton.rollKey) as? CABasicAnimation)
        let rest = thumbs.countLayer.position.y
        #expect(up.duration == 0.3)
        // Flipped: the new number comes from below, the old one leaves upward.
        #expect(try #require(up.fromValue as? CGFloat) > rest)
        #expect(try #require(up.toValue as? CGFloat) == rest)
        #expect(try #require(leaving.toValue as? CGFloat) < rest)
        #expect(thumbs.ghostLayer.string as? String == "2")
        #expect(thumbs.countText == "3")

        row.configure(reactions(pill("👍", 2)), animated: true)
        let down = try #require(thumbs.countLayer.animation(forKey: ReactionPillButton.rollKey) as? CABasicAnimation)
        #expect(try #require(down.fromValue as? CGFloat) < rest)
    }

    @Test
    func nothingRollsWhereItMayNotOrWhenTheRowIsJustShown() {
        let row = view(reactions(pill("👍", 2)))
        let thumbs = row.pills[0]

        row.configure(reactions(pill("👍", 3)), animated: true)
        #expect(thumbs.countLayer.animation(forKey: ReactionPillButton.rollKey) == nil)

        row.rolls = { true }
        row.configure(reactions(pill("👍", 4)))
        #expect(thumbs.countLayer.animation(forKey: ReactionPillButton.rollKey) == nil)
        #expect(thumbs.countText == "4")
    }

    @Test
    func aCountThatDidntChangeDoesntRoll() {
        let row = view(reactions(pill("👍", 2), pill("🎉", 1)))
        row.rolls = { true }

        row.configure(reactions(pill("👍", 2), pill("🎉", 2)), animated: true)

        #expect(row.pills[0].countLayer.animation(forKey: ReactionPillButton.rollKey) == nil)
        #expect(row.pills[1].countLayer.animation(forKey: ReactionPillButton.rollKey) != nil)
    }

    // MARK: The emoji's picture

    // A 2x bitmap of 28 × 28 points with `emoji` drawn in the 20-point box at (4, 4): where its
    // picture's pixels are.
    func ink(_ emoji: String) throws -> (rep: NSBitmapImageRep, columns: ClosedRange<Int>, rows: ClosedRange<Int>) {
        let side = 56
        let rep = try #require(
            NSBitmapImageRep(
                bitmapDataPlanes: nil, pixelsWide: side, pixelsHigh: side, bitsPerSample: 8,
                samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                bytesPerRow: 0, bitsPerPixel: 0))
        let context = try #require(NSGraphicsContext(bitmapImageRep: rep)).cgContext
        // As a flipped view at 2x draws: the origin at the top left.
        context.translateBy(x: 0, y: CGFloat(side))
        context.scaleBy(x: 2, y: -2)
        EmojiGlyphView.draw(
            emoji, in: NSRect(x: 4, y: 4, width: 20, height: 20), context: context, flipped: true)
        var (left, right, top, bottom) = (side, -1, side, -1)
        for y in 0..<side {
            for x in 0..<side where (rep.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0.02 {
                (left, right, top, bottom) = (min(left, x), max(right, x), min(top, y), max(bottom, y))
            }
        }
        try #require(right >= left && bottom >= top, "nothing drawn")
        return (rep, left...right, top...bottom)
    }

    // The box is pixels 8…47 and a face is round: its picture drawn part of a pixel off (and
    // so resampled) would cover one column or row more on one side.
    @Test
    func aFaceStandsOnItsBoxesPixels() throws {
        let face = try ink("😀")

        #expect(face.columns.lowerBound + face.columns.upperBound == 55, "\(face.columns)")
        #expect(face.rows.lowerBound + face.rows.upperBound == 55, "\(face.rows)")
        #expect(face.columns.lowerBound >= 8 && face.columns.count >= 38, "\(face.columns)")
        #expect(face.rows.lowerBound >= 8 && face.rows.count >= 38, "\(face.rows)")
    }

    @Test(arguments: ["👍", "🙏", "🎉", "❤️", "🏳️‍🌈", "1️⃣"])
    func everyEmojiStaysInsideItsBox(_ emoji: String) throws {
        let drawn = try ink(emoji)

        #expect(drawn.columns.lowerBound >= 8 && drawn.columns.upperBound <= 47, "\(drawn.columns)")
        #expect(drawn.rows.lowerBound >= 8 && drawn.rows.upperBound <= 47, "\(drawn.rows)")
    }

    // What the system's font has no single picture for is drawn as several.
    @Test
    func severalGlyphsAreMadeSmallerToFitTheBox() throws {
        let two = try ink("👍🎉")

        #expect(two.columns.lowerBound >= 8 && two.columns.upperBound <= 47, "\(two.columns)")
        #expect(two.columns.count >= 36, "as wide as the box allows: \(two.columns)")
        #expect(abs((two.rows.lowerBound + two.rows.upperBound) - 55) <= 2, "in the middle: \(two.rows)")
    }

    @Test
    func aUnicodeEmojiFillsItsBoxAndIsCentered() throws {
        let side = 40
        let rep = try #require(
            NSBitmapImageRep(
                bitmapDataPlanes: nil, pixelsWide: side, pixelsHigh: side, bitsPerSample: 8,
                samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                bytesPerRow: 0, bitsPerPixel: 0))
        let context = try #require(NSGraphicsContext(bitmapImageRep: rep)).cgContext
        // As a flipped view at 2x draws: the origin at the top left.
        context.translateBy(x: 0, y: CGFloat(side))
        context.scaleBy(x: 2, y: -2)
        EmojiGlyphView.draw("😀", in: NSRect(x: 0, y: 0, width: 20, height: 20), context: context, flipped: true)

        var (left, right, top, bottom) = (side, -1, side, -1)
        for y in 0..<side {
            for x in 0..<side where (rep.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0.2 {
                (left, right, top, bottom) = (min(left, x), max(right, x), min(top, y), max(bottom, y))
            }
        }
        #expect(left <= 2 && right >= side - 3, "the face is as wide as the box: \(left)…\(right)")
        #expect(top <= 2 && bottom >= side - 3, "and as high: \(top)…\(bottom)")
        #expect(abs((left + right) - (side - 1)) <= 2)
        #expect(abs((top + bottom) - (side - 1)) <= 2)
        // Upright: the eyes are in the upper half, so it is darker there than at the chin.
        let eyes = try #require(rep.colorAt(x: 13, y: 14)?.usingColorSpace(.sRGB))
        let chin = try #require(rep.colorAt(x: 20, y: 34)?.usingColorSpace(.sRGB))
        #expect(eyes.brightnessComponent < chin.brightnessComponent)
    }
}
