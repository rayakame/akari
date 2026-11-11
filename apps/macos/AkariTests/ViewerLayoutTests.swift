import AkariKit
import AppKit
import Testing

@testable import Akari

// The viewer's geometry from Discord's numbers (canary 653948: modules 967758, 947056, 591818).
@MainActor
struct ViewerLayoutTests {
    // Module 967758: two boxes, the wider fit wins; never upscaled.
    @Test
    func mediaFitsDiscordsBoxes() {
        let window = NSSize(width: 1280, height: 800)
        #expect(ViewerLayout.fit(NSSize(width: 4000, height: 3000), in: window, several: false) == NSSize(width: 832, height: 624))
        #expect(ViewerLayout.fit(NSSize(width: 300, height: 200), in: window, several: false) == NSSize(width: 300, height: 200))
        // A tall image uses the height beside the author and the buttons.
        #expect(ViewerLayout.fit(NSSize(width: 1000, height: 3000), in: window, several: false) == NSSize(width: 243, height: 728))
        #expect(ViewerLayout.fit(NSSize(width: 1000, height: 3000), in: window, several: true) == NSSize(width: 208, height: 624))
        #expect(ViewerLayout.fit(NSSize(width: 4000, height: 1000), in: window, several: true) == NSSize(width: 1128, height: 282))
        #expect(ViewerLayout.fit(NSSize(width: 4000, height: 1000), in: NSSize(width: 100, height: 100), several: true).width >= 1)
    }

    // Module 947056's grid: 36 and 24 around, 12 apart; the top bar and Close in the first row,
    // the arrows centred at the sides, the strip in the last row between them.
    @Test
    func theChromeSitsInDiscordsGrid() {
        let frames = ViewerLayout.frames(in: NSSize(width: 1280, height: 800), items: 3, buttons: 4)
        #expect(frames.close == NSRect(x: 1216, y: 36, width: 40, height: 40))
        #expect(frames.avatar == NSRect(x: 24, y: 36, width: 40, height: 40))
        #expect(frames.author.minX == 76 && frames.author.width <= 184)
        #expect(frames.actions == NSRect(x: 1204 - 148, y: 36, width: 148, height: 40), "4 × 32, 3 × 4, padding 3 and a 1 border")
        #expect(frames.previous == NSRect(x: 24, y: 380, width: 40, height: 40))
        #expect(frames.next == NSRect(x: 1216, y: 380, width: 40, height: 40))
        #expect(frames.strip == NSRect(x: 570, y: 724, width: 140, height: 40), "3 × 40, 2 × 2 and 8 at each end, centred")

        let one = ViewerLayout.frames(in: NSSize(width: 1280, height: 800), items: 1, buttons: 0)
        #expect(one.previous == nil && one.next == nil && one.strip == nil)
        #expect(one.actions == nil)
    }

    // Module 591818: a click zooms 2.5× keeping the clicked point under the pointer; panning stops
    // where the media's edge meets the window's.
    @Test
    func zoomKeepsThePointAndPanStopsAtTheEdges() {
        let window = NSSize(width: 1000, height: 800)
        let offset = ViewerLayout.zoomOffset(at: NSPoint(x: 600, y: 400), in: window)
        #expect(offset == NSPoint(x: -150, y: 0), "100 right of the centre, 1.5 × 100 back")

        let scaled = NSSize(width: 1600, height: 600)
        #expect(ViewerLayout.clamp(NSPoint(x: -500, y: 90), media: scaled, in: window) == NSPoint(x: -300, y: 0))
        #expect(ViewerLayout.clamp(NSPoint(x: 120, y: 0), media: scaled, in: window) == NSPoint(x: 120, y: 0))
    }

    // The fitted size at the screen's pixels; zoomed, 2.5 × that, never above the source (but never
    // below the unzoomed ask) or 4096.
    @Test
    func theRequestFollowsTheZoom() {
        let fitted = NSSize(width: 832, height: 624)
        let source = NSSize(width: 4000, height: 3000)
        #expect(ViewerLayout.pixels(fitted, scale: 2, source: source, zoomed: false) == NSSize(width: 1664, height: 1248))
        #expect(ViewerLayout.pixels(fitted, scale: 2, source: source, zoomed: true) == NSSize(width: 4000, height: 3000))
        #expect(ViewerLayout.pixels(fitted, scale: 1, source: source, zoomed: true) == NSSize(width: 2080, height: 1560))
        let huge = NSSize(width: 10000, height: 5000)
        #expect(ViewerLayout.pixels(NSSize(width: 1200, height: 600), scale: 2, source: huge, zoomed: true) == NSSize(width: 4096, height: 2048))
        // A source smaller than the screen's pixels: zoomed asks what unzoomed did, never fewer.
        let small = NSSize(width: 800, height: 600)
        #expect(ViewerLayout.pixels(small, scale: 2, source: small, zoomed: true) == NSSize(width: 1600, height: 1200))
    }
}
