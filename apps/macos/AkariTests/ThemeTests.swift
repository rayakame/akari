import AppKit
import Foundation
import Testing

@testable import Akari

struct ThemeTests {
    nonisolated struct Token: CustomTestStringConvertible, Sendable {
        let name: String
        let dark: String
        let light: String
        let alpha: (dark: Double, light: Double)

        init(_ name: String, _ dark: String, _ light: String, alpha: (Double, Double) = (1, 1)) {
            self.name = name
            self.dark = dark
            self.light = light
            self.alpha = alpha
        }

        var testDescription: String { name }
    }

    // Copied from docs/ui/theme.md.
    nonisolated static let tokens: [Token] = [
        Token("frame", "#121214", "#f3f3f4"),
        Token("chat", "#1a1a1e", "#fbfbfb"),
        Token("panel", "#202024", "#fbfbfb"),
        Token("card", "#28282d", "#ffffff"),
        Token("frameBorder", "#97979f", "#97979f", alpha: (0.12, 0.28)),
        Token("borderSubtle", "#97979f", "#97979f", alpha: (0.12, 0.28)),
        Token("borderMuted", "#97979f", "#97979f", alpha: (0.04, 0.20)),
        Token("textDefault", "#efeff1", "#2e2e34"),
        Token("textStrong", "#fbfbfb", "#28282d"),
        Token("textMuted", "#96979e", "#6c6d76"),
        Token("chatTextMuted", "#81828a", "#70717a"),
        Token("textLink", "#4d96ee", "#006dd4"),
        Token("textError", "#f87e7a", "#b92733"),
        Token("channelDefault", "#81828a", "#666770"),
        Token("interactiveText", "#abacb2", "#595a63"),
        Token("interactiveTextActive", "#fbfbfb", "#28282d"),
        Token("hoverBackground", "#97979f", "#97979f", alpha: (0.12, 0.12)),
        Token("selectedBackground", "#97979f", "#97979f", alpha: (0.20, 0.24)),
        Token("inputBackground", "#000000", "#000000", alpha: (0.12, 0.02)),
        Token("danger", "#f23f43", "#da373c"),
        Token("brand", "#5865f2", "#5865f2"),
        Token("avatar0", "#5865f2", "#5865f2"),
        Token("avatar1", "#3e8e7e", "#3e8e7e"),
        Token("avatar2", "#c06c2b", "#c06c2b"),
        Token("avatar3", "#a352b5", "#a352b5"),
        Token("avatar4", "#c2445a", "#c2445a"),
        Token("avatar5", "#4f7fba", "#4f7fba"),
    ]

    let colors: [String: NSColor] = [
        "frame": Palette.frame, "chat": Palette.chat, "panel": Palette.panel,
        "card": Palette.card, "frameBorder": Palette.frameBorder,
        "borderSubtle": Palette.borderSubtle, "borderMuted": Palette.borderMuted,
        "textDefault": Palette.textDefault, "textStrong": Palette.textStrong,
        "textMuted": Palette.textMuted, "chatTextMuted": Palette.chatTextMuted,
        "textLink": Palette.textLink, "textError": Palette.textError,
        "channelDefault": Palette.channelDefault, "interactiveText": Palette.interactiveText,
        "interactiveTextActive": Palette.interactiveTextActive,
        "hoverBackground": Palette.hoverBackground,
        "selectedBackground": Palette.selectedBackground,
        "inputBackground": Palette.inputBackground, "danger": Palette.danger,
        "brand": Palette.brand, "avatar0": Palette.avatars[0], "avatar1": Palette.avatars[1],
        "avatar2": Palette.avatars[2], "avatar3": Palette.avatars[3],
        "avatar4": Palette.avatars[4], "avatar5": Palette.avatars[5],
    ]

    static func components(_ color: NSColor, in appearance: NSAppearance.Name) -> [Double] {
        var components: [Double] = []
        NSAppearance(named: appearance)?.performAsCurrentDrawingAppearance {
            if let srgb = color.usingColorSpace(.sRGB) {
                components = [
                    srgb.redComponent, srgb.greenComponent, srgb.blueComponent,
                    srgb.alphaComponent,
                ].map(Double.init)
            }
        }
        return components
    }

    static func expected(_ hex: String, alpha: Double) -> [Double] {
        let value = UInt32(hex.dropFirst(), radix: 16) ?? 0
        let channel = { (shift: UInt32) in Double((value >> shift) & 0xff) / 255 }
        return [channel(16), channel(8), channel(0), alpha]
    }

    @Test(arguments: tokens)
    func everyTokenResolvesToTheDocumentedHexPerAppearance(token: Token) throws {
        let color = try #require(colors[token.name])
        let dark = Self.components(color, in: .darkAqua)
        let light = Self.components(color, in: .aqua)

        let tolerance = [1.0 / 255, 1.0 / 255, 1.0 / 255, 0.01]
        let close = { (got: [Double], want: [Double]) in
            got.count == 4 && zip(zip(got, want), tolerance).allSatisfy { abs($0.0 - $0.1) <= $1 }
        }
        #expect(close(dark, Self.expected(token.dark, alpha: token.alpha.dark)), "dark \(dark)")
        #expect(
            close(light, Self.expected(token.light, alpha: token.alpha.light)), "light \(light)")
    }

    @Test
    func everyTokenIsChecked() {
        #expect(Set(colors.keys) == Set(Self.tokens.map(\.name)))
        #expect(Palette.avatars.count == 6)
    }

    @Test
    func darkIsTheDefaultAndChoicesRoundTrip() throws {
        let suite = "app.akari.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }

        #expect(ThemeChoice.stored(in: defaults) == .dark)
        ThemeChoice.light.store(in: defaults)
        #expect(ThemeChoice.stored(in: defaults) == .light)
        #expect(ThemeChoice.dark.appearance?.name == .darkAqua)
        #expect(ThemeChoice.light.appearance?.name == .aqua)
        #expect(ThemeChoice.system.appearance == nil)
    }
}
