import AVKit
import AkariKit
import AppKit

/// The viewer's media over the scrim (D146): fitted by Discord's two boxes, zoomed 2.5 × at the
/// pointer, panned by dragging and scrolling within the window's edges, a video in the Mac's
/// player. Clicks beside the media close the viewer.
final class ViewerStage: NSView {
    let tile = MediaTileView()
    private(set) var item: ViewerModel.Item?
    private(set) var zoomed = false
    private(set) var offset = NSPoint.zero
    private var several = false
    private var preloads: [ImageLayerView] = []
    private var preloadItems: [ViewerModel.Item] = []
    private var player: AVPlayerView?
    private var playerTask: Task<Void, Never>?
    private var readiness: NSKeyValueObservation?
    private var press: (point: NSPoint, offset: NSPoint, onMedia: Bool)?
    private var pinch: CGFloat = 0
    var images: ImageCache? {
        didSet {
            tile.images = images
            for view in preloads {
                view.images = images
            }
        }
    }
    var placeholders: PlaceholderCache? {
        didSet { tile.placeholders = placeholders }
    }
    var onClose: () -> Void = {}
    var onZoom: (Bool) -> Void = { _ in }
    /// What plays: a video's or a GIFV's original.
    var playSource: (ViewerModel.Item) async -> URL? = { _ in nil }

    static let clickSlop: CGFloat = 20
    /// A mouse wheel's line, in points (NSScrollView's).
    static let lineScroll: CGFloat = 10

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        tile.cornerRadius = 0
        tile.picture.fadesIn = true
        addSubview(tile)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    // Even the first click of a window that wasn't key zooms.
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    var zoomable: Bool { item.map { $0.kind != .video } ?? false }

    /// The media's box on the stage: fitted, 2.5 × zoomed, moved by the pan.
    var mediaFrame: NSRect {
        guard let item else {
            return .zero
        }
        let fitted = Self.fitted(item, in: bounds.size, several: several)
        let factor = zoomed ? ViewerLayout.zoom : 1
        let size = NSSize(width: fitted.width * factor, height: fitted.height * factor)
        return NSRect(
            x: bounds.midX + offset.x - size.width / 2, y: bounds.midY + offset.y - size.height / 2,
            width: size.width, height: size.height)
    }

    static func fitted(_ item: ViewerModel.Item, in size: NSSize, several: Bool) -> NSSize {
        ViewerLayout.fit(source(of: item), in: size, several: several)
    }

    // Without its size the media takes the whole box Discord's fit allows.
    private static func source(of item: ViewerModel.Item) -> NSSize {
        guard item.width > 0, item.height > 0 else {
            return NSSize(width: ViewerLayout.maxPixels, height: ViewerLayout.maxPixels)
        }
        return NSSize(width: CGFloat(item.width), height: CGFloat(item.height))
    }

    /// Shows `item`, unzoomed, with `preloads` asked at their fitted pixels; nil lets everything go.
    func show(
        _ item: ViewerModel.Item?, preloads: [ViewerModel.Item] = [], several: Bool = false, animates: Bool
    ) {
        let changed = item != self.item
        self.item = item
        self.several = several
        if changed {
            setZoomed(false, at: nil)
            stopPlayer()
            tile.configure(item.map { MediaTileView.Content(media: Self.media($0)) })
            tile.loopSource = item.map { item in { [weak self] in await self?.playSource(item) } }
            if let item, item.kind == .video {
                startPlayer(item)
            }
        }
        tile.animates = animates
        preload(preloads)
        needsLayout = true
    }

    static func media(_ item: ViewerModel.Item) -> MediaTileView.Media {
        var media = MediaTileView.Media(key: item.key)
        media.video = item.kind == .video
        media.ownPlay = false
        media.loops = item.kind == .gifv
        media.animated = item.animated
        media.placeholder = item.placeholder
        media.description = item.description
        return media
    }

    private func preload(_ items: [ViewerModel.Item]) {
        while preloads.count < items.count {
            let view = ImageLayerView()
            view.images = images
            view.tier = .media
            view.isHidden = true
            view.maxPixels = NSSize(width: ViewerLayout.maxPixels, height: ViewerLayout.maxPixels)
            addSubview(view, positioned: .below, relativeTo: tile)
            preloads.append(view)
        }
        while preloads.count > items.count {
            preloads.removeLast().removeFromSuperview()
        }
        preloadItems = items
        for (view, item) in zip(preloads, items) {
            view.show(ImageLayerView.Content(key: item.key, canAnimate: false))
        }
    }

    var preloadKeys: [ImageKey] { preloads.compactMap { $0.content?.key } }

    /// Zooms in keeping `point` under the pointer, or around the centre without one; out returns
    /// to the fitted box.
    func setZoomed(_ zoomed: Bool, at point: NSPoint?) {
        let zoomed = zoomed && zoomable
        guard zoomed != self.zoomed else {
            return
        }
        self.zoomed = zoomed
        offset = .zero
        if zoomed, let point {
            offset = clamped(ViewerLayout.zoomOffset(at: point, in: bounds.size))
        }
        layoutMedia()
        onZoom(zoomed)
    }

    private func clamped(_ offset: NSPoint) -> NSPoint {
        ViewerLayout.clamp(offset, media: mediaFrame.size, in: bounds.size)
    }

    func stop() {
        show(nil, animates: false)
        preload([])
    }

    // The cap is in pixels: another display's scale asks again.
    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        needsLayout = true
    }

    override func layout() {
        super.layout()
        for (view, item) in zip(preloads, preloadItems) {
            view.frame = NSRect(origin: .zero, size: Self.fitted(item, in: bounds.size, several: several))
        }
        if zoomed {
            offset = clamped(offset)
        }
        layoutMedia()
    }

    private func layoutMedia() {
        let frame = mediaFrame
        tile.frame = frame
        player?.frame = frame
        if let item {
            let fitted = Self.fitted(item, in: bounds.size, several: several)
            let pixels = ViewerLayout.pixels(
                fitted, scale: window?.backingScaleFactor ?? 1, source: Self.source(of: item), zoomed: zoomed)
            tile.picture.maxPixels = pixels
        }
    }

    // A video's player takes its own clicks; the rest of the stage is the media's or the scrim's.
    override func hitTest(_ point: NSPoint) -> NSView? {
        guard !isHidden, frame.contains(point) else {
            return nil
        }
        if let player, !player.isHidden, player.frame.contains(convert(point, from: superview)) {
            return player.hitTest(convert(point, from: superview)) ?? player
        }
        return self
    }

    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        press = (point, offset, mediaFrame.contains(point))
    }

    override func mouseDragged(with event: NSEvent) {
        guard let press, press.onMedia, zoomed else {
            return
        }
        let point = convert(event.locationInWindow, from: nil)
        offset = clamped(
            NSPoint(x: press.offset.x + point.x - press.point.x, y: press.offset.y + point.y - press.point.y))
        layoutMedia()
    }

    override func mouseUp(with event: NSEvent) {
        guard let press else {
            return
        }
        self.press = nil
        let point = convert(event.locationInWindow, from: nil)
        let moved = hypot(point.x - press.point.x, point.y - press.point.y)
        guard press.onMedia else {
            if !mediaFrame.contains(point), moved < Self.clickSlop {
                onClose()
            }
            return
        }
        if zoomed {
            if moved < Self.clickSlop {
                setZoomed(false, at: nil)
            }
        } else if tile.picture.failed {
            tile.picture.retry()
        } else {
            setZoomed(true, at: point)
        }
    }

    // Module 591818 pans with the wheel and the trackpad while zoomed; ⌃ scrolls do nothing.
    override func scrollWheel(with event: NSEvent) {
        guard zoomed, !event.modifierFlags.contains(.control) else {
            return
        }
        let factor = event.hasPreciseScrollingDeltas ? 1 : Self.lineScroll
        offset = clamped(
            NSPoint(x: offset.x + event.scrollingDeltaX * factor, y: offset.y + event.scrollingDeltaY * factor))
        layoutMedia()
    }

    // The Mac's pinch zooms in and out at the pointer.
    override func magnify(with event: NSEvent) {
        if event.phase == .began {
            pinch = 0
        }
        pinch += event.magnification
        let point = convert(event.locationInWindow, from: nil)
        if pinch > 0.1, !zoomed, mediaFrame.contains(point) {
            setZoomed(true, at: point)
            pinch = 0
        } else if pinch < -0.1, zoomed {
            setZoomed(false, at: nil)
            pinch = 0
        }
    }

    /// Over zoomable media the zoom cursors (the arrow before macOS 15, which has neither).
    func cursor(at point: NSPoint) -> NSCursor {
        zoomable && mediaFrame.contains(point) ? (zoomed ? Self.zoomOut : Self.zoomIn) : .arrow
    }

    static var zoomIn: NSCursor {
        if #available(macOS 15, *) { .zoomIn } else { .arrow }
    }

    static var zoomOut: NSCursor {
        if #available(macOS 15, *) { .zoomOut } else { .arrow }
    }

    // F-6: autoplay with sound, no loop, the Mac's controls; the poster shows until the first
    // frame does.
    private func startPlayer(_ item: ViewerModel.Item) {
        playerTask = Task { [weak self] in
            guard let url = await self?.playSource(item), let self, !Task.isCancelled,
                self.item == item
            else {
                return
            }
            let view = AVPlayerView()
            view.controlsStyle = .floating
            view.showsFullScreenToggleButton = false
            view.videoGravity = .resizeAspect
            let player = AVPlayer(url: url)
            player.preventsDisplaySleepDuringVideoPlayback = true
            view.player = player
            view.frame = self.mediaFrame
            // The initial call comes at once, before the view shows; later ones may come from
            // AVFoundation's queue.
            self.readiness = view.observe(\.isReadyForDisplay, options: [.initial, .new]) { view, _ in
                nonisolated(unsafe) let view = view
                guard Thread.isMainThread else {
                    DispatchQueue.main.async {
                        MainActor.assumeIsolated { view.isHidden = !view.isReadyForDisplay }
                    }
                    return
                }
                MainActor.assumeIsolated { view.isHidden = !view.isReadyForDisplay }
            }
            self.addSubview(view, positioned: .above, relativeTo: self.tile)
            self.player = view
            player.play()
        }
    }

    private func stopPlayer() {
        playerTask?.cancel()
        playerTask = nil
        readiness = nil
        player?.player?.pause()
        // The item holds the download.
        player?.player?.replaceCurrentItem(with: nil)
        player?.player = nil
        player?.removeFromSuperview()
        player = nil
    }

    var playerView: AVPlayerView? { player }
}
