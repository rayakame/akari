import SwiftUI

/// Discord's primary button: the brand fill, darker while hovered and pressed, a faint white
/// border; small (32) in a header, medium (40) on its own. The secondary one is gray.
struct BrandButton: View {
    enum Size {
        case small, medium

        var height: CGFloat { self == .small ? 32 : 40 }
        var minWidth: CGFloat { self == .small ? 60 : 100 }
        var padding: CGFloat { self == .small ? 12 : 16 }
        var style: TextStyle { self == .small ? TypeScale.formAlert : TypeScale.button }
    }

    let title: String
    var size = Size.medium
    var secondary = false
    let action: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            Text(title)
        }
        .buttonStyle(Style(size: size, secondary: secondary, hovered: hovered))
        .onHover { hovered = $0 }
        .background(CursorArea(.pointingHand))
    }

    /// The width it takes for `title`.
    static func width(_ title: String, size: Size) -> CGFloat {
        let text = (title as NSString).size(withAttributes: [.font: size.style.nsFont]).width
        return max(size.minWidth, text.rounded(.up) + 2 * size.padding)
    }

    private struct Style: ButtonStyle {
        let size: Size
        let secondary: Bool
        let hovered: Bool

        private func fill(pressed: Bool) -> NSColor {
            if secondary {
                return pressed || hovered ? Palette.backgroundModStrong : Palette.backgroundModNormal
            }
            return pressed ? Palette.brandPressed : hovered ? Palette.brandHover : Palette.brand
        }

        func makeBody(configuration: Configuration) -> some View {
            configuration.label
                .font(size.style.font)
                .foregroundStyle(secondary ? Color(nsColor: Palette.textStrong) : .white)
                .lineLimit(1)
                .padding(.horizontal, size.padding)
                .frame(minWidth: size.minWidth, minHeight: size.height, maxHeight: size.height)
                .background(
                    Color(nsColor: fill(pressed: configuration.isPressed)),
                    in: RoundedRectangle(cornerRadius: 8)
                )
                .overlay {
                    RoundedRectangle(cornerRadius: 8)
                        .strokeBorder(Color.white.opacity(0.08), lineWidth: 1)
                }
                .contentShape(RoundedRectangle(cornerRadius: 8))
        }
    }
}
