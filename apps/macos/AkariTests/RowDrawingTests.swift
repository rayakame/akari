import AkariKit
import AppKit
import Testing

@testable import Akari

// Rows draw with layers rather than bitmaps: a drawn bitmap of the placeholder block, at
// least one and a half views tall, was most of the launch's memory peak.
@MainActor
struct RowDrawingTests {
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 800, height: 700),
        styleMask: [.titled], backing: .buffered, defer: false)

    func shown(_ view: NSView, appearance: NSAppearance.Name = .darkAqua) -> NSView {
        window.appearance = NSAppearance(named: appearance)
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 800, height: 700))
        container.wantsLayer = true
        window.contentView = container
        view.frame = NSRect(x: 0, y: 0, width: 800, height: 600)
        container.addSubview(view)
        window.layoutIfNeeded()
        view.layoutSubtreeIfNeeded()
        window.displayIfNeeded()
        return view
    }

    func placeholders(failure: String? = nil) -> PlaceholderCell {
        let cell = PlaceholderCell()
        cell.translatesAutoresizingMaskIntoConstraints = true
        cell.show(.older, height: 600, failure: failure)
        return cell
    }

    // The layer tree as Core Animation composites it, read by pixel from the top left.
    struct LayerImage {
        let rep: NSBitmapImageRep

        init(_ view: NSView) throws {
            let layer = try #require(view.layer)
            let size = view.bounds.size
            rep = try #require(
                NSBitmapImageRep(
                    bitmapDataPlanes: nil, pixelsWide: Int(size.width),
                    pixelsHigh: Int(size.height),
                    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0))
            let context = try #require(NSGraphicsContext(bitmapImageRep: rep))
            view.effectiveAppearance.performAsCurrentDrawingAppearance {
                layer.render(in: context.cgContext)
            }
        }

        func alpha(_ x: Int, _ y: Int) -> CGFloat {
            rep.colorAt(x: x, y: y)?.alphaComponent ?? 0
        }
    }

    // A view that implements draw(_:) gets a bitmap backing as large as itself.
    static func drawsItsOwnBitmap(_ type: NSView.Type, over base: NSView.Type) -> Bool {
        let draw = #selector(NSView.draw(_:))
        return class_getMethodImplementation(type, draw)
            != class_getMethodImplementation(base, draw)
    }

    @Test
    func placeholdersDrawNoBitmap() {
        #expect(!Self.drawsItsOwnBitmap(PlaceholderCell.self, over: NSTableCellView.self))
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func placeholdersShowTheirShapes(_ appearance: NSAppearance.Name) throws {
        let image = try LayerImage(shown(placeholders(), appearance: appearance))

        // The first row (66 tall) has a circle at 16...56 and a name bar from 72, 20 below its
        // top; nothing between them. The shapes are hoverBackground, which is translucent.
        #expect(image.alpha(36, 38) > 0.05)
        #expect(image.alpha(100, 26) > 0.05)
        #expect(image.alpha(64, 38) < 0.01)
        #expect(image.alpha(36, 5) < 0.01)
    }

    @Test
    func aFailureLeavesItsBandEmpty() throws {
        let plain = try LayerImage(shown(placeholders()))
        let failed = try LayerImage(shown(placeholders(failure: "Couldn't load older messages.")))

        // An older block's band is its bottom 48 (552...600); the 26 row from 544 has its bar at
        // 551...563, which the band hides.
        #expect(plain.alpha(100, 557) > 0.05)
        #expect(failed.alpha(100, 557) < 0.01)
        #expect(failed.alpha(36, 516) > 0.05)
    }
}
