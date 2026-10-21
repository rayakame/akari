import AppKit

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
    // The dark value is Discord's; the light one keeps its 2.06:1 contrast on the light sidebar.
    static let threadSpine = token("threadSpine", dark: "#474850", light: "#aaabb3")
    // The box under a message with a thread, measured from the user's screenshots (amendment 175).
    static let threadBoxFill = token("threadBoxFill", dark: "#242429", light: "#ffffff")
    static let threadBoxBorder = token("threadBoxBorder", dark: "#323237", light: "#e2e2e5")
    // System lines' icons, from Discord's client: its arrow and call assets carry their colors.
    static let noticeIn = token("noticeIn", dark: "#3ba55c", light: "#3ba55c")
    static let noticeOut = token("noticeOut", dark: "#ed4245", light: "#ed4245")
    static let missedCall = token("missedCall", dark: "#99aab5", light: "#99aab5")
    static let boost = token("boost", dark: "#ff73fa", light: "#ff73fa")
    static let warning = token("warning", dark: "#ea9800", light: "#ea9800")
    static let statusPositive = token("statusPositive", dark: "#3d9e60", light: "#269153")
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
    static let composer = token("composer", dark: "#222327", light: "#ffffff")
    static let mentionText = token("mentionText", dark: "#c9cdfb", light: "#4752c4")
    static let mentionBackground = token(
        "mentionBackground", dark: "#5865f2", light: "#5865f2", alpha: (0.30, 0.15))
    static let mentionedBackground = token(
        "mentionedBackground", dark: "#f8a300", light: "#f8a300", alpha: (0.08, 0.10))
    static let mentionedBar = token("mentionedBar", dark: "#f8a300", light: "#f8a300")
    // Discord's --message-highlight-background-default, the same in both themes.
    static let messageHighlight = token(
        "messageHighlight", dark: "#5865f2", light: "#5865f2", alpha: (41 / 255, 41 / 255))
    static let codeBackground = token(
        "codeBackground", dark: "#97979f", light: "#97979f", alpha: (0.08, 0.10))
    static let codeBorder = token(
        "codeBorder", dark: "#97979f", light: "#97979f", alpha: (0.16, 0.24))
    static let quoteBar = token(
        "quoteBar", dark: "#97979f", light: "#97979f", alpha: (0.40, 0.40))
    // The message layout manager draws it; it isn't isolated to the main actor.
    nonisolated static let spoilerHidden = token(
        "spoilerHidden", dark: "#121214", light: "#d6d7dc")
    static let spoilerRevealed = token(
        "spoilerRevealed", dark: "#97979f", light: "#97979f", alpha: (0.16, 0.16))
    static let timestampBackground = token(
        "timestampBackground", dark: "#97979f", light: "#97979f", alpha: (0.12, 0.12))
    static let folder = token("folder", dark: "#97979f", light: "#97979f", alpha: (0.16, 0.20))
    static let mentionBadge = token("mentionBadge", dark: "#d22d39", light: "#d22d39")
    static let mentionBadgeText = token("mentionBadgeText", dark: "#ffffff", light: "#ffffff")
    static let newMessages = token("newMessages", dark: "#d22d39", light: "#d22d39")
    static let newMessagesText = token("newMessagesText", dark: "#ffffff", light: "#ffffff")
    static let startIcon = token(
        "startIcon", dark: "#97979f", light: "#97979f", alpha: (0.08, 0.10))
    static let avatars = ["#5865f2", "#3e8e7e", "#c06c2b", "#a352b5", "#c2445a", "#4f7fba"]
        .map { NSColor(hex: $0) }

    // AppKit can resolve colors off the main thread; MainActor isolation would trap there.
    nonisolated private static func token(
        _ name: String, dark: String, light: String, alpha: (dark: CGFloat, light: CGFloat) = (1, 1)
    ) -> NSColor {
        let darkColor = NSColor(hex: dark, alpha: alpha.dark)
        let lightColor = NSColor(hex: light, alpha: alpha.light)
        return NSColor(name: name) { appearance in
            appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? darkColor : lightColor
        }
    }
}
