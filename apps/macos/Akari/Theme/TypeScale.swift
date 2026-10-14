import AppKit
import SwiftUI

/// One style of the type scale, given in points at the 16 pt base.
nonisolated struct TextStyle: Equatable, Sendable {
    enum Weight: Sendable {
        case regular, medium, semibold, bold, heavy

        var appKit: NSFont.Weight {
            switch self {
            case .regular: .regular
            case .medium: .medium
            case .semibold: .semibold
            case .bold: .bold
            case .heavy: .heavy
            }
        }

        var swiftUI: Font.Weight {
            switch self {
            case .regular: .regular
            case .medium: .medium
            case .semibold: .semibold
            case .bold: .bold
            case .heavy: .heavy
            }
        }
    }

    let points: CGFloat
    let weight: Weight
    /// The line height at the base for styles set on a fixed line; others use the font's own.
    let linePoints: CGFloat?

    init(_ points: CGFloat, _ weight: Weight = .regular, line: CGFloat? = nil) {
        self.points = points
        self.weight = weight
        linePoints = line
    }

    var size: CGFloat { size(base: TypeScale.base) }
    var line: CGFloat { line(base: TypeScale.base) }
    var nsFont: NSFont { .systemFont(ofSize: size, weight: weight.appKit) }
    var font: Font { .system(size: size, weight: weight.swiftUI) }

    func size(base: CGFloat) -> CGFloat {
        TypeScale.size(points, base: base)
    }

    func line(base: CGFloat) -> CGFloat {
        if let linePoints {
            return TypeScale.size(linePoints, base: base)
        }
        let font = NSFont.systemFont(ofSize: size(base: base), weight: weight.appKit)
        return (font.ascender - font.descender + font.leading).rounded(.up)
    }

    func weight(_ weight: Weight) -> TextStyle {
        TextStyle(points, weight, line: linePoints)
    }
}

/// Every text style of the app. Sizes are relative to `base`, so a font size setting like the
/// official client's changes only the base. The values and how they were measured are in
/// docs/ui/typography.md.
nonisolated enum TypeScale {
    static let base: CGFloat = 16

    /// A length given at the 16 pt base, at `base`.
    static func size(_ points: CGFloat, base: CGFloat = TypeScale.base) -> CGFloat {
        points * base / 16
    }

    // Measured against the official client.
    static let body = TextStyle(16, line: 22)
    static let serverHeader = TextStyle(15, .semibold)
    static let category = TextStyle(13, .medium)
    static let channelName = TextStyle(15, .medium)
    static let channelHeader = TextStyle(14, .semibold)
    static let username = TextStyle(15.5, .medium)
    static let messageTime = TextStyle(11, .medium)
    // The line of the 12 pt it had before, so the smaller size doesn't move the list.
    static let dayDivider = TextStyle(11, .semibold, line: 15)
    static let startTitle = TextStyle(30, .heavy)
    static let startSubtitle = TextStyle(15.5)

    // Not measured yet: the values Akari had.
    static let startUsername = TextStyle(18, .medium)
    static let topic = TextStyle(14)
    static let panelName = TextStyle(14, .semibold)
    static let panelUsername = TextStyle(12)
    static let hoverTime = TextStyle(11)
    static let appTag = TextStyle(10, .semibold)
    static let detail = TextStyle(14)
    static let small = TextStyle(12)
    static let jumpBar = TextStyle(14, .medium)
    static let connectionBar = TextStyle(13, .medium)
    static let warning = TextStyle(13)

    static let heading1 = TextStyle(24, .bold, line: 32)
    static let heading2 = TextStyle(20, .bold, line: 28)
    static let heading3 = TextStyle(16, .bold, line: 22)
    static let subtext = TextStyle(13, line: 18)
    static let codeBlock = TextStyle(14, line: 18)
    static let jumbo = TextStyle(48, line: 56)

    static let loginTitle = TextStyle(24, .semibold)
    static let qrTitle = TextStyle(20, .semibold)
    static let sheetTitle = TextStyle(17, .semibold)
    static let fieldLabel = TextStyle(12, .bold)
    static let button = TextStyle(16, .medium)
}
