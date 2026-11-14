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
        #expect(row.hitTest(NSPoint(x: 130, y: 10)) == nil)
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

        let fill = resolved(mine.colors.fill, name)
        #expect(abs(fill.alphaComponent - 0.24) < 0.001)
        #expect(resolved(mine.colors.border, name) == resolved(NSColor(hex: "#5865f2"), name))
        #expect(
            resolved(mine.colors.count, name)
                == resolved(NSColor(hex: name == .darkAqua ? "#dfe4ff" : "#273478"), name))
        let layer = try #require(mine.layer)
        #expect(layer.borderWidth == 1)
        #expect(layer.cornerRadius == 8)
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
