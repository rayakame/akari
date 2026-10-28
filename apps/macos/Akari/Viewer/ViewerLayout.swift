import AkariKit
import AppKit

/// The media viewer's geometry from Discord's numbers (canary 653948): the grid of module 947056,
/// the fit of 967758 and the zoom of 591818. Frames are top-left based, in the window's points.
nonisolated enum ViewerLayout {
    struct Frames: Equatable {
        var avatar: NSRect
        var author: NSRect
        var actions: NSRect?
        var close: NSRect
        var previous: NSRect?
        var next: NSRect?
        var strip: NSRect?
    }

    static let marginX: CGFloat = 24
    static let marginY: CGFloat = 36
    static let gap: CGFloat = 12
    static let row: CGFloat = 40
    static let side: CGFloat = 184
    static let button: CGFloat = 32
    static let buttonGap: CGFloat = 4
    static let barPadding: CGFloat = 3
    static let barBorder: CGFloat = 1
    static let thumbnail: CGFloat = 40
    static let thumbnailGap: CGFloat = 2
    static let stripEnd: CGFloat = 8
    static let zoom: CGFloat = 2.5
    static let maxPixels: CGFloat = 4096

    /// The media's box: the wider of Discord's two fits, never upscaled.
    static func fit(_ media: NSSize, in window: NSSize, several: Bool) -> NSSize {
        // Between the arrows, the first and last rows left free.
        let wide = NSSize(
            width: window.width - (several ? 76 : 24) * 2, height: window.height - 176)
        // The centre column, beside the author and the buttons.
        let tall = NSSize(
            width: window.width - 544, height: window.height - (several ? 88 : 36) * 2)
        let a = contain(media, in: wide)
        let b = contain(media, in: tall)
        return a.width >= b.width ? a : b
    }

    private static func contain(_ size: NSSize, in box: NSSize) -> NSSize {
        let fitted = mediaContain(
            source: MediaSize(width: points(size.width), height: points(size.height)),
            max: MediaSize(width: points(box.width), height: points(box.height)))
        return NSSize(width: CGFloat(fitted.width), height: CGFloat(fitted.height))
    }

    private static func points(_ value: CGFloat) -> UInt32 {
        UInt32(max(1, min(value.rounded(), CGFloat(UInt32.max))))
    }

    static func frames(in window: NSSize, items: Int, buttons: Int) -> Frames {
        let top = marginY
        let right = window.width - marginX
        let close = NSRect(x: right - row, y: top, width: row, height: row)
        let avatar = NSRect(x: marginX, y: top, width: row, height: row)
        let authorX = avatar.maxX + gap
        // The bar keeps 52 for Close (`padding-inline-end`).
        let barEnd = right - row - gap
        var actions: NSRect?
        if buttons > 0 {
            let count = CGFloat(buttons)
            let width = count * button + (count - 1) * buttonGap + 2 * (barPadding + barBorder)
            actions = NSRect(x: barEnd - width, y: top, width: width, height: row)
        }
        let authorEnd = min(authorX + side, (actions?.minX ?? barEnd) - gap)
        let author = NSRect(x: authorX, y: top, width: max(0, authorEnd - authorX), height: row)
        guard items > 1 else {
            return Frames(avatar: avatar, author: author, actions: actions, close: close)
        }
        let middle = (window.height / 2 - row / 2).rounded()
        let count = CGFloat(items)
        let width = count * thumbnail + (count - 1) * thumbnailGap + 2 * stripEnd
        let room = window.width - 2 * (marginX + row + gap)
        let shown = min(width, max(0, room))
        let strip = NSRect(
            x: ((window.width - shown) / 2).rounded(), y: window.height - marginY - row, width: shown,
            height: row)
        return Frames(
            avatar: avatar, author: author, actions: actions, close: close,
            previous: NSRect(x: marginX, y: middle, width: row, height: row),
            next: NSRect(x: right - row, y: middle, width: row, height: row), strip: strip)
    }

    /// The offset that keeps the clicked point under the pointer once zoomed (`point` in the
    /// window, top-left based).
    static func zoomOffset(at point: NSPoint, in window: NSSize) -> NSPoint {
        NSPoint(
            x: -(point.x - window.width / 2) * (zoom - 1), y: -(point.y - window.height / 2) * (zoom - 1))
    }

    /// Panning stops where the media's edge meets the window's; media smaller than the window
    /// along an axis stays centred on it.
    static func clamp(_ offset: NSPoint, media: NSSize, in window: NSSize) -> NSPoint {
        func axis(_ value: CGFloat, _ length: CGFloat, _ room: CGFloat) -> CGFloat {
            guard length > room else {
                return 0
            }
            let bound = (length - room) / 2
            return min(bound, max(-bound, value))
        }
        return NSPoint(
            x: axis(offset.x, media.width, window.width), y: axis(offset.y, media.height, window.height))
    }

    /// The pixels the media is asked at: the fitted box on this screen, 2.5 × that zoomed, never
    /// above the source or 4096 a side.
    static func pixels(_ fitted: NSSize, scale: CGFloat, source: NSSize, zoomed: Bool) -> NSSize {
        let factor = scale * (zoomed ? zoom : 1)
        var wanted = NSSize(
            width: (fitted.width * factor).rounded(.up), height: (fitted.height * factor).rounded(.up))
        if zoomed, wanted.width > source.width || wanted.height > source.height {
            wanted = source
        }
        let cap = min(1, maxPixels / max(wanted.width, 1), maxPixels / max(wanted.height, 1))
        return NSSize(width: (wanted.width * cap).rounded(), height: (wanted.height * cap).rounded())
    }
}
