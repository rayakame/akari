import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// Akari's logo stands where the official client shows its own: the rail's home button and the
// title bar's home tile draw it as the app icon places it in its square.
@MainActor @Suite
struct AppLogoTests {
    nonisolated static let themes: [NSAppearance.Name] = [.darkAqua, .aqua]

    func session(atHome: Bool) -> SessionModel {
        let session = SessionModel(
            userId: UserId(rawValue: 1), account: CrowdedAccount(),
            memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(atHome ? .home : .guild(CrowdedStore.guildIds[0]))
        return session
    }

    /// How much of each pixel the logo image covers at `side` points and `scale`, from the top left.
    func coverage(side: CGFloat, scale: CGFloat) throws -> [[CGFloat]] {
        let image = try #require(NSImage(named: "AkariLogo"))
        let pixels = Int((side * scale).rounded())
        let rep = try #require(
            NSBitmapImageRep(
                bitmapDataPlanes: nil, pixelsWide: pixels, pixelsHigh: pixels, bitsPerSample: 8,
                samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                bytesPerRow: 0, bitsPerPixel: 0))
        rep.size = NSSize(width: side, height: side)
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        image.draw(in: NSRect(x: 0, y: 0, width: side, height: side))
        NSGraphicsContext.restoreGraphicsState()
        return (0..<pixels).map { y in (0..<pixels).map { x in rep.colorAt(x: x, y: y)?.alphaComponent ?? 0 } }
    }

    /// What's wrong with `tile` (points from the top left, its corners rounded by `radius`) as a
    /// drawing of the logo: every pixel the logo fully covers has to take the logo's color, every
    /// one it leaves the tile's.
    func problems(drawingTheLogo image: Rendered, in tile: NSRect, radius: CGFloat) throws -> [String] {
        let covered = try coverage(side: tile.width, scale: image.scale)
        let left = Int((tile.minX * image.scale).rounded())
        let top = Int((tile.minY * image.scale).rounded())
        let corner = Int((radius * image.scale).rounded(.up))
        let side = covered.count
        var ink: [NSColor] = []
        var ground: [NSColor] = []
        var expected: [(NSColor, Bool)] = []
        for y in 0..<side {
            for x in 0..<side {
                let inCorner = (x < corner || x >= side - corner) && (y < corner || y >= side - corner)
                guard !inCorner, let color = image.pixel(left + x, top + y) else {
                    continue
                }
                if covered[y][x] >= 0.99 {
                    ink.append(color)
                    expected.append((color, true))
                } else if covered[y][x] <= 0.01 {
                    ground.append(color)
                    expected.append((color, false))
                }
            }
        }
        guard let logo = ink.dropFirst(ink.count / 2).first, let back = ground.dropFirst(ground.count / 2).first else {
            return ["the logo covers nothing in the tile"]
        }
        guard distance(logo, back) > 0.3 else {
            return ["no logo: its color \(logo) is the tile's \(back)"]
        }
        let wrong = expected.filter { color, isLogo in
            (distance(color, logo) < distance(color, back)) != isLogo
        }
        // Edges antialias a little differently in a view than in a bitmap.
        return wrong.count * 50 > expected.count
            ? ["\(wrong.count) of \(expected.count) pixels disagree with the logo"] : []
    }

    func distance(_ a: NSColor, _ b: NSColor) -> CGFloat {
        abs(a.redComponent - b.redComponent) + abs(a.greenComponent - b.greenComponent)
            + abs(a.blueComponent - b.blueComponent)
    }

    // The bundle names AppIcon, and the icon the About window, the Dock and Cmd+Tab show is
    // Akari's: the brand color inside its rounded square, clear around it.
    @Test
    func theAppHasItsIcon() throws {
        #expect(Bundle.main.object(forInfoDictionaryKey: "CFBundleIconName") as? String == "AppIcon")
        let icon = try #require(NSApp.applicationIconImage)
        let side = 128
        let rep = try #require(
            NSBitmapImageRep(
                bitmapDataPlanes: nil, pixelsWide: side, pixelsHigh: side, bitsPerSample: 8,
                samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                bytesPerRow: 0, bitsPerPixel: 0))
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        icon.draw(in: NSRect(x: 0, y: 0, width: side, height: side))
        NSGraphicsContext.restoreGraphicsState()

        // Inside the square, above the logo; then the canvas's corner.
        let body = try #require(rep.colorAt(x: side / 2, y: side / 5)?.usingColorSpace(.sRGB))
        #expect(body.blueComponent - body.redComponent > 0.3, "\(body)")
        #expect((rep.colorAt(x: 1, y: 1)?.alphaComponent ?? 1) < 0.1)
    }

    @Test(arguments: themes, [true, false])
    func theHomeButtonShowsTheLogo(appearance: NSAppearance.Name, atHome: Bool) throws {
        let size = NSSize(width: SidebarWidth.rail, height: 120)
        let image = try Rendered(
            ServerRail(session: session(atHome: atHome)).frame(width: size.width, height: size.height),
            appearance: appearance, size: size)
        // The first item under the window's title bar: its tile starts where the middle column
        // first changes.
        let background = try #require(image.color(1, 1))
        let middle = Int(size.width / 2 * image.scale)
        let top = try #require(
            (0..<image.rep.pixelsHigh).first { y in !same(image.pixel(middle, y), background) })
        let tile = NSRect(x: (SidebarWidth.rail - 40) / 2, y: CGFloat(top) / image.scale, width: 40, height: 40)

        let problems = try problems(drawingTheLogo: image, in: tile, radius: RailShape.cornerRadius)

        #expect(problems.isEmpty, "\(appearance.rawValue), at home: \(atHome): \(problems)")
        if atHome {
            let white = try #require(image.color(tile.midX, tile.minY + 15))
            #expect(white.brightnessComponent > 0.9, "selected: the logo is white, \(white)")
        }
    }

    @Test(arguments: themes)
    func theTitlesHomeTileShowsTheLogo(appearance: NSAppearance.Name) throws {
        let size = NSSize(width: 400, height: TitleBar.height)
        let image = try Rendered(
            TitleBar(title: WindowTitle(text: "Direct Messages", icon: .home))
                .frame(width: size.width, height: size.height),
            appearance: appearance, size: size)
        let background = try #require(image.color(2, 2))
        let middle = Int(TitleBar.height / 2 * image.scale)
        let first = try #require(
            (0..<image.rep.pixelsWide).first { x in !same(image.pixel(x, middle), background) })
        let tile = NSRect(
            x: CGFloat(first) / image.scale, y: (TitleBar.height - TitleBar.iconSize) / 2,
            width: TitleBar.iconSize, height: TitleBar.iconSize)

        let problems = try problems(drawingTheLogo: image, in: tile, radius: TitleBar.iconRadius)

        #expect(problems.isEmpty, "\(appearance.rawValue): \(problems)")
    }
}
