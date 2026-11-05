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

/// Every text style of the app, relative to `base` (values and method: docs/ui/typography.md).
/// A new base also needs the fonts made once rebuilt and every row remeasured.
nonisolated enum TypeScale {
    static let base: CGFloat = 16

    /// A length given at the 16 pt base, at `base`.
    static func size(_ points: CGFloat, base: CGFloat = TypeScale.base) -> CGFloat {
        points * base / 16
    }

    // Measured against the official client.
    static let body = TextStyle(16, line: 22)
    static let noticeName = TextStyle(16, .medium, line: 22)
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
    static let startOwner = TextStyle(15.5, .semibold)
    static let titleBar = TextStyle(12.5, .medium)

    // Not measured yet: the values Akari had.
    static let startUsername = TextStyle(18, .medium)
    static let topic = TextStyle(14)
    static let panelName = TextStyle(14, .semibold)
    static let panelUsername = TextStyle(12)
    static let hoverTime = TextStyle(11)
    static let appTag = TextStyle(10, .semibold)
    static let newTag = TextStyle(10, .bold)
    static let detail = TextStyle(14)
    static let small = TextStyle(12)
    static let jumpBar = TextStyle(14, .medium)
    static let connectionBar = TextStyle(13, .medium)
    static let warning = TextStyle(13)
    static let mentionBadge = TextStyle(12, .bold)

    static let heading1 = TextStyle(24, .bold, line: 32)
    static let heading2 = TextStyle(20, .bold, line: 28)
    static let heading3 = TextStyle(16, .bold, line: 22)
    static let subtext = TextStyle(13, line: 18)
    static let codeBlock = TextStyle(14, line: 18)
    static let jumbo = TextStyle(48, line: 56)

    static let loginTitle = TextStyle(24, .semibold)
    static let qrTitle = TextStyle(20, .semibold)
    static let gateTitle = TextStyle(20, .semibold)
    static let gateText = TextStyle(16, .medium)
    static let sheetTitle = TextStyle(17, .semibold)
    static let fieldLabel = TextStyle(12, .bold)
    static let button = TextStyle(16, .medium)
    // Discord's heading-md, text-sm and the consolidated section label, in the Threads browser.
    static let browserTitle = TextStyle(16, .semibold, line: 20)
    static let browserLabel = TextStyle(14, .medium)
    static let browserText = TextStyle(14, line: 18)
    static let emptyTitle = TextStyle(20, .semibold, line: 24)
    static let emptyText = TextStyle(16, line: 20)
    // The New Thread form: text-md/medium titles and labels, text-md fields, text-xs notes,
    // text-sm/medium for the starter's error and the small button.
    static let formTitle = TextStyle(16, .medium, line: 20)
    static let formText = TextStyle(16, line: 20)
    static let formNote = TextStyle(12, line: 16)
    static let formAlert = TextStyle(14, .medium, line: 18)
    static let formStrip = TextStyle(12, .medium, line: 16)
}
