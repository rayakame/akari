import AppKit
import Testing

@testable import Akari

@MainActor
struct TypeScaleTests {
    // docs/ui/typography.md: the official client's sizes, matched in SF Pro at the 16 pt base.
    @Test(arguments: [
        (TypeScale.body, 16, TextStyle.Weight.regular),
        (TypeScale.serverHeader, 15, .semibold),
        (TypeScale.category, 13, .medium),
        (TypeScale.channelName, 15, .medium),
        (TypeScale.channelHeader, 14, .semibold),
        (TypeScale.username, 15.5, .medium),
        (TypeScale.messageTime, 11, .medium),
        (TypeScale.dayDivider, 11, .semibold),
        (TypeScale.startTitle, 30, .heavy),
        (TypeScale.startSubtitle, 15.5, .regular),
    ])
    func measuredStylesHaveTheOfficialSizes(
        style: TextStyle, points: CGFloat, weight: TextStyle.Weight
    ) {
        #expect(TypeScale.base == 16)
        #expect(style.size == points)
        #expect(style.weight == weight)
        #expect(style.nsFont.pointSize == points)
    }

    @Test
    func everyStyleScalesWithTheBase() {
        let style = TypeScale.username
        #expect(style.size(base: 16) == 15.5)
        #expect(style.size(base: 20) == 19.375)
        #expect(TypeScale.body.line(base: 16) == 22)
        #expect(TypeScale.body.line(base: 12) == 16.5)
        #expect(TypeScale.size(8, base: 24) == 12)
    }

    @Test
    func fontsCarryTheStylesWeight() {
        let weight = { (font: NSFont) in
            let traits = font.fontDescriptor.object(forKey: .traits)
            return (traits as? [NSFontDescriptor.TraitKey: Any])?[.weight] as? CGFloat
        }
        #expect(weight(TypeScale.serverHeader.nsFont) == NSFont.Weight.semibold.rawValue)
        #expect(weight(TypeScale.channelName.nsFont) == NSFont.Weight.medium.rawValue)
        #expect(weight(TypeScale.startTitle.nsFont) == NSFont.Weight.heavy.rawValue)
        #expect(
            weight(TypeScale.channelName.weight(.semibold).nsFont)
                == NSFont.Weight.semibold.rawValue)
    }

    @Test
    func aSmallerDividerFontKeepsTheDividersRowHeight() {
        // 16 above, the 12 pt line it had before, 4 below: the list's rhythm doesn't move.
        #expect(DayDividerCell.height == 35)
    }
}
