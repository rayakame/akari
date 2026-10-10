import AppKit

// The tokens of docs/ui/theme.md; each resolves to its dark or light value.
enum Palette {
    static let frame = token("frame", dark: "#121214", light: "#f3f3f4")
    static let chat = token("chat", dark: "#1a1a1e", light: "#fbfbfb")
    static let panel = token("panel", dark: "#202024", light: "#fbfbfb")
    static let card = token("card", dark: "#28282d", light: "#ffffff")
    static let frameBorder = token(
        "frameBorder", dark: "#97979f", light: "#97979f", alpha: (0.12, 0.28))
    static let borderSubtle = token(
        "borderSubtle", dark: "#97979f", light: "#97979f", alpha: (0.12, 0.28))
    static let borderMuted = token(
        "borderMuted", dark: "#97979f", light: "#97979f", alpha: (0.04, 0.20))
    static let textDefault = token("textDefault", dark: "#efeff1", light: "#2e2e34")
    static let textStrong = token("textStrong", dark: "#fbfbfb", light: "#28282d")
    static let textMuted = token("textMuted", dark: "#96979e", light: "#6c6d76")
    static let chatTextMuted = token("chatTextMuted", dark: "#81828a", light: "#70717a")
    static let textLink = token("textLink", dark: "#4d96ee", light: "#006dd4")
    static let textError = token("textError", dark: "#f87e7a", light: "#b92733")
    static let channelDefault = token("channelDefault", dark: "#81828a", light: "#666770")
    static let interactiveText = token("interactiveText", dark: "#abacb2", light: "#595a63")
    static let interactiveTextActive = token(
        "interactiveTextActive", dark: "#fbfbfb", light: "#28282d")
    static let hoverBackground = token(
        "hoverBackground", dark: "#97979f", light: "#97979f", alpha: (0.12, 0.12))
    static let selectedBackground = token(
        "selectedBackground", dark: "#97979f", light: "#97979f", alpha: (0.20, 0.24))
    static let inputBackground = token(
        "inputBackground", dark: "#000000", light: "#000000", alpha: (0.12, 0.02))
    static let danger = token("danger", dark: "#f23f43", light: "#da373c")
    static let brand = NSColor(hex: "#5865f2")
    static let avatars = ["#5865f2", "#3e8e7e", "#c06c2b", "#a352b5", "#c2445a", "#4f7fba"]
        .map { NSColor(hex: $0) }

    private static func token(
        _ name: String, dark: String, light: String, alpha: (dark: CGFloat, light: CGFloat) = (1, 1)
    ) -> NSColor {
        let darkColor = NSColor(hex: dark, alpha: alpha.dark)
        let lightColor = NSColor(hex: light, alpha: alpha.light)
        return NSColor(name: name) { appearance in
            appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? darkColor : lightColor
        }
    }
}
