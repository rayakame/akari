import AppKit
import SwiftUI
import Testing

@testable import Akari

// The modal's X is Discord's design-system icon-only button (canary 648857, IconButton size sm):
// a 32 square with radius 8; idle a #ABACB2 glyph on nothing, hovered a fill of
// rgba(151,151,159,.161) with a 1 border of .039 and a #FBFBFB glyph, pressed a .2 fill. Chris's
// screenshot of the hovered one: #36363B on the modal's #242429, a #FBFBFB glyph.
@MainActor
struct ModalCloseButtonTests {
    static let size = NSSize(width: 48, height: 48)

    func rendered(_ look: ModalCloseButton.Look, _ appearance: NSAppearance.Name) throws
        -> Rendered
    {
        try Rendered(
            ModalCloseButton(action: {}, look: look)
                .frame(width: Self.size.width, height: Self.size.height)
                .background(Color(nsColor: Palette.surfaceHigh))
                .ignoresSafeArea(),
            appearance: appearance, size: Self.size)
    }

    func drawn(_ token: NSColor, over surface: NSColor, _ appearance: NSAppearance.Name) throws
        -> NSColor?
    {
        try Rendered(
            Color(nsColor: surface).overlay(Color(nsColor: token)).ignoresSafeArea(),
            appearance: appearance, size: NSSize(width: 10, height: 10)
        ).color(5, 5)
    }

    // The glyph's extreme pixel: the brightest on dark, the darkest on light.
    func glyph(_ image: Rendered, _ appearance: NSAppearance.Name) -> NSColor? {
        var best: NSColor?
        for x in stride(from: CGFloat(18), through: 30, by: 0.5) {
            for y in stride(from: CGFloat(18), through: 30, by: 0.5) {
                guard let color = image.color(x, y)?.usingColorSpace(.sRGB) else {
                    continue
                }
                let better =
                    appearance == .darkAqua
                    ? color.brightnessComponent > (best?.brightnessComponent ?? -1)
                    : color.brightnessComponent < (best?.brightnessComponent ?? 2)
                if better {
                    best = color
                }
            }
        }
        return best
    }

    // Within a few levels: antialiasing and the bitmap's color space.
    func close(_ color: NSColor?, _ other: NSColor?) -> Bool {
        guard let color = color?.usingColorSpace(.sRGB), let other = other?.usingColorSpace(.sRGB)
        else {
            return false
        }
        return abs(color.redComponent - other.redComponent) < 0.03
            && abs(color.greenComponent - other.greenComponent) < 0.03
            && abs(color.blueComponent - other.blueComponent) < 0.03
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func hoveredItFillsA32SquareWithRadius8(_ appearance: NSAppearance.Name) throws {
        let image = try rendered(.hovered, appearance)
        let surface = try drawn(.clear, over: Palette.surfaceHigh, appearance)
        let fill = try drawn(Palette.iconOnlyHover, over: Palette.surfaceHigh, appearance)
        let ink = try drawn(Palette.interactiveTextActive, over: Palette.surfaceHigh, appearance)

        // The box is 8...40: its fill inside, the surface outside.
        #expect(same(image.color(12, 24), fill), "\(String(describing: image.color(12, 24)))")
        #expect(same(image.color(24, 12), fill))
        #expect(same(image.color(6, 24), surface))
        #expect(same(image.color(24, 42), surface))
        // Rounded by 8: a corner's tip is the surface, a point inside its arc the fill.
        #expect(same(image.color(9, 9), surface), "\(String(describing: image.color(9, 9)))")
        #expect(same(image.color(12, 12), fill), "\(String(describing: image.color(12, 12)))")
        #expect(
            close(glyph(image, appearance), ink),
            "\(String(describing: glyph(image, appearance))) \(String(describing: ink))")
        if appearance == .darkAqua {
            // Discord's #36363C, Chris's screenshot #36363B.
            let discord = try drawn(NSColor(hex: "#36363c"), over: .black, appearance)
            #expect(close(image.color(12, 24), discord))
        }
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func idleItsGlyphStandsOnTheSurfaceAlone(_ appearance: NSAppearance.Name) throws {
        let image = try rendered(.idle, appearance)
        let surface = try drawn(.clear, over: Palette.surfaceHigh, appearance)
        let ink = try drawn(Palette.interactiveText, over: Palette.surfaceHigh, appearance)

        #expect(same(image.color(12, 24), surface))
        #expect(same(image.color(12, 12), surface))
        #expect(
            close(glyph(image, appearance), ink),
            "\(String(describing: glyph(image, appearance))) \(String(describing: ink))")
    }

    @Test
    func pressedItFillsMore() throws {
        let image = try rendered(.pressed, .darkAqua)
        let fill = try drawn(Palette.iconOnlyPressed, over: Palette.surfaceHigh, .darkAqua)
        let ink = try drawn(Palette.interactiveTextActive, over: Palette.surfaceHigh, .darkAqua)

        #expect(same(image.color(12, 24), fill))
        #expect(close(glyph(image, .darkAqua), ink))
    }
}

// Discord's other browser and header buttons change color only: the search field's clear
// button #ABACB2, #FBFBFB hovered; all of them show the pointing hand (`cursor: pointer`).
@MainActor
struct BrowserButtonTests {
    func rendered(hovered: Bool) throws -> Rendered {
        try Rendered(
            SearchClearButton(clear: {}, hovered: hovered)
                .frame(width: 40, height: 40)
                .background(Color(nsColor: Palette.surfaceHigh))
                .ignoresSafeArea(),
            size: NSSize(width: 40, height: 40))
    }

    func brightest(_ image: Rendered) -> NSColor? {
        var best: NSColor?
        for x in stride(from: CGFloat(10), through: 30, by: 0.5) {
            for y in stride(from: CGFloat(10), through: 30, by: 0.5) {
                if let color = image.color(x, y)?.usingColorSpace(.sRGB),
                    color.brightnessComponent > (best?.brightnessComponent ?? -1)
                {
                    best = color
                }
            }
        }
        return best
    }

    func drawn(_ token: NSColor) throws -> NSColor? {
        try Rendered(
            Color(nsColor: Palette.surfaceHigh).overlay(Color(nsColor: token)).ignoresSafeArea(),
            size: NSSize(width: 10, height: 10)
        ).color(5, 5)?.usingColorSpace(.sRGB)
    }

    @Test
    func theClearButtonBrightensOnHover() throws {
        let idle = try #require(brightest(try rendered(hovered: false)))
        let hovered = try #require(brightest(try rendered(hovered: true)))
        let idleInk = try #require(try drawn(Palette.interactiveText))
        let hoveredInk = try #require(try drawn(Palette.interactiveTextActive))

        #expect(abs(idle.brightnessComponent - idleInk.brightnessComponent) < 0.03, "\(idle)")
        #expect(
            abs(hovered.brightnessComponent - hoveredInk.brightnessComponent) < 0.03, "\(hovered)")
    }

    func handAreas<Content: View>(in view: Content, size: NSSize) -> [NSRect] {
        let host = NSHostingView(rootView: view.frame(width: size.width, height: size.height))
        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        func areas(_ view: NSView) -> [CursorArea.Area] {
            ((view as? CursorArea.Area).map { [$0] } ?? []) + view.subviews.flatMap(areas)
        }
        return areas(host).filter { $0.cursor == .pointingHand }.map {
            $0.convert($0.bounds, to: nil)
        }
    }

    @Test
    func theHeaderAndSplitButtonsAndTheXShowThePointingHand() {
        let size = NSSize(width: 60, height: 60)
        let center = NSPoint(x: 30, y: 30)
        for (name, areas) in [
            ("Threads button", handAreas(in: ThreadsButton(selected: false) {}, size: size)),
            (
                "split's X",
                handAreas(in: HeaderIconButton(symbol: "xmark", label: "Close") {}, size: size)
            ),
            ("modal's X", handAreas(in: ModalCloseButton(action: {}), size: size)),
            ("clear button", handAreas(in: SearchClearButton(clear: {}), size: size)),
        ] {
            #expect(areas.contains { $0.contains(center) }, "\(name): \(areas)")
        }
    }
}
