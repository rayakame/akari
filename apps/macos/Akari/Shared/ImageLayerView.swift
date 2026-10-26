import AkariKit
import AppKit

/// A box's image as a layer (D135): asked from the cache at the box's pixels on the window's
/// backing scale, set as layer contents at that scale on whole backing pixels, clipped by the
/// layer. It asks when shown in a window and sized, cancels when it shows something else or
/// leaves, asks again when the backing scale changes or the cache says failed boxes may retry,
/// and plays the animated form through a `FramePlayer` while `animates` is on. It draws nothing
/// and tracks no pointer: the list stays the authority.
final class ImageLayerView: NSView {
    struct Content: Equatable {
        let key: ImageKey
        /// An animated form exists (`ImageKey.canAnimate`); the owner decides when it plays.
        var canAnimate = false

        init(key: ImageKey, canAnimate: Bool? = nil) {
            self.key = key
            self.canAnimate = canAnimate ?? key.canAnimate
        }
    }

    enum Placeholder {
        /// Nothing: what lies behind shows (a server's initials).
        case none
        /// Discord's disc while an avatar loads or without one, kept when it fails.
        case disc
    }

    var images: ImageCache? {
        didSet {
            if images !== oldValue {
                cancelLoads()
                reload()
            }
        }
    }
    var placeholder = Placeholder.none {
        didSet { updatePlaceholder() }
    }
    var cornerRadius: CGFloat = 0 {
        didSet {
            imageLayer.cornerRadius = cornerRadius
            placeholderLayer.cornerRadius = cornerRadius
        }
    }
    /// Covering the box (avatars, icons) or the whole picture inside it (emoji, stickers).
    var fit = ImageFit.cover {
        didSet {
            guard fit != oldValue else {
                return
            }
            imageLayer.contentsGravity = fit == .cover ? .resizeAspectFill : .resizeAspect
            if content != nil {
                cancelLoads()
                clearImage()
                reload()
            }
        }
    }
    /// Whether the owner wants the animation (the motion policy's say); nothing without an
    /// animated form.
    var animates = false {
        didSet {
            if animates != oldValue {
                updateAnimation()
            }
        }
    }
    /// Told when the image landed, failed or was replaced; owners that show something of their
    /// own around a failure (a server's acronym tile, a sticker's "Uh Oh!") listen.
    var onStateChange: ((ImageLayerView) -> Void)?
    /// Shown in the box when the image failed (a custom emoji's `:name:`).
    var fallbackText: String? {
        didSet { updateFallback() }
    }
    private(set) var content: Content?
    /// The still failed to load; the placeholder stays until a retry.
    private(set) var failed = false
    let imageLayer = CALayer()
    let placeholderLayer = CALayer()
    let fallbackLayer = CATextLayer()
    private(set) var player: FramePlayer?
    private var still: CGImage?
    private var stillTask: Task<Void, Never>?
    private var animationTask: Task<Void, Never>?
    private var pinned: ImageSpec?
    private var askedPixels: (Int, Int)?

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layerContentsRedrawPolicy = .never
        for layer in [placeholderLayer, imageLayer] {
            layer.actions = [
                "contents": NSNull(), "hidden": NSNull(), "bounds": NSNull(), "position": NSNull(),
                "cornerRadius": NSNull(), "backgroundColor": NSNull(),
            ]
            layer.masksToBounds = true
            layer.isHidden = true
            self.layer?.addSublayer(layer)
        }
        imageLayer.contentsGravity = .resizeAspectFill
        fallbackLayer.actions = ["contents": NSNull(), "hidden": NSNull(), "bounds": NSNull(), "position": NSNull()]
        fallbackLayer.isHidden = true
        fallbackLayer.alignmentMode = .center
        fallbackLayer.truncationMode = .end
        fallbackLayer.isWrapped = false
        fallbackLayer.font = NSFont.systemFont(ofSize: TypeScale.size(11))
        fallbackLayer.fontSize = TypeScale.size(11)
        layer?.addSublayer(fallbackLayer)
        updatePlaceholder()
        // A selector observer goes away with the view.
        NotificationCenter.default.addObserver(
            self, selector: #selector(retryRequested(_:)), name: ImageCache.retryFailedNotification,
            object: nil)
    }

    required init?(coder: NSCoder) {
        nil
    }

    /// Shows `content`: the cached image at once, else the placeholder while it loads. The same
    /// subject's new picture (a changed avatar or icon) keeps the old one until the new has
    /// decoded (D137); another subject clears at once. `nil` clears the box.
    func show(_ content: Content?) {
        guard content != self.content else {
            return
        }
        let sameSubject =
            if let content, let old = self.content { content.key.sameSubject(as: old.key) } else { false }
        self.content = content
        failed = false
        cancelLoads()
        if !sameSubject {
            clearImage()
        }
        updatePlaceholder()
        reload()
        onStateChange?(self)
    }

    // Clicks and the pointer belong to what lies around the box.
    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        cancelLoads()
        if window != nil {
            needsLayout = true
            reload()
        }
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        let scale = window?.backingScaleFactor ?? 1
        imageLayer.contentsScale = scale
        placeholderLayer.contentsScale = scale
        needsLayout = true
    }

    // The box's size (a table cell's subviews get theirs after joining the window) and the
    // backing scale are known here: the ask happens, or happens again, at the pixels they give,
    // the old image staying until the new one decoded.
    override func layout() {
        super.layout()
        let scale = window?.backingScaleFactor ?? 1
        var frame = Self.alignedFrame(bounds, scale: scale)
        if window != nil {
            let inWindow = convert(frame.origin, to: nil)
            frame.origin.x += (inWindow.x * scale).rounded() / scale - inWindow.x
            frame.origin.y += (inWindow.y * scale).rounded() / scale - inWindow.y
        }
        imageLayer.frame = frame
        placeholderLayer.frame = frame
        fallbackLayer.frame = frame
        fallbackLayer.contentsScale = scale
        askIfPixelsChanged()
    }

    // SwiftUI sizes its representables outside a layout pass.
    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        askIfPixelsChanged()
    }

    private func askIfPixelsChanged() {
        guard content != nil, let pixels = pixelsIfSized else {
            return
        }
        if let askedPixels, askedPixels == pixels {
            return
        }
        cancelLoads()
        reload()
    }

    /// `bounds` moved onto whole backing pixels at `scale`: the origin to the nearest pixel,
    /// the size rounded up to whole pixels, so a bitmap decoded at the box's pixels draws 1:1.
    /// `layout` then shifts the result onto the window's pixel grid.
    static func alignedFrame(_ bounds: NSRect, scale: CGFloat) -> NSRect {
        NSRect(
            x: (bounds.minX * scale).rounded() / scale, y: (bounds.minY * scale).rounded() / scale,
            width: (bounds.width * scale).rounded(.up) / scale,
            height: (bounds.height * scale).rounded(.up) / scale)
    }

    /// The pixels a box of `size` points covers at `scale`, each side rounded up.
    static func pixels(_ size: NSSize, scale: CGFloat) -> (Int, Int) {
        (Int((size.width * scale).rounded(.up)), Int((size.height * scale).rounded(.up)))
    }

    private var pixelsIfSized: (Int, Int)? {
        let pixels = Self.pixels(bounds.size, scale: window?.backingScaleFactor ?? 1)
        return pixels.0 > 0 && pixels.1 > 0 ? pixels : nil
    }

    private func spec(animated: AnimatedFormat? = nil) -> ImageSpec? {
        guard let content, let pixels = pixelsIfSized else {
            return nil
        }
        return ImageSpec(
            key: content.key, width: pixels.0, height: pixels.1, animated: animated, fit: fit)
    }

    private func reload() {
        guard window != nil, let images, stillTask == nil, !failed, let spec = spec() else {
            return
        }
        askedPixels = (spec.width, spec.height)
        pin(spec)
        if let cached = images.cached(spec) {
            set(still: cached)
            updateAnimation()
            return
        }
        stillTask = Task { [weak self] in
            let outcome: Result<CGImage, Error>
            do {
                outcome = .success(try await images.image(spec))
            } catch {
                outcome = .failure(error)
            }
            guard let self, !Task.isCancelled else {
                return
            }
            self.stillTask = nil
            switch outcome {
            case .success(let image):
                self.set(still: image)
                self.updateAnimation()
            case .failure(is CancellationError):
                break
            case .failure:
                self.failed = true
                self.clearImage()
                self.updatePlaceholder()
                self.onStateChange?(self)
            }
        }
    }

    // The cache says the session is back: a failed box asks once more.
    @objc private func retryRequested(_ notification: Notification) {
        guard failed, let images, notification.object as AnyObject === images else {
            return
        }
        failed = false
        cancelLoads()
        reload()
    }

    private func set(still image: CGImage) {
        still = image
        imageLayer.contentsScale = window?.backingScaleFactor ?? 1
        if player == nil {
            imageLayer.contents = image
        }
        imageLayer.isHidden = false
        failed = false
        updatePlaceholder()
        onStateChange?(self)
    }

    private func clearImage() {
        still = nil
        imageLayer.contents = nil
        imageLayer.isHidden = true
    }

    // Stops every ask and the animation; the image shown stays.
    private func cancelLoads() {
        stillTask?.cancel()
        stillTask = nil
        stopAnimation()
        if let pinned {
            images?.unpin(pinned)
            self.pinned = nil
        }
        askedPixels = nil
    }

    private func pin(_ spec: ImageSpec) {
        if let pinned, pinned != spec {
            images?.unpin(pinned)
        }
        if pinned != spec {
            images?.pin(spec)
            pinned = spec
        }
    }

    private func updatePlaceholder() {
        let showsDisc = placeholder == .disc && (still == nil || failed)
        placeholderLayer.isHidden = !showsDisc
        if showsDisc {
            effectiveAppearance.performAsCurrentDrawingAppearance {
                placeholderLayer.backgroundColor = Palette.avatarPlaceholder.cgColor
            }
        }
        updateFallback()
    }

    private func updateFallback() {
        let shows = failed && fallbackText != nil
        fallbackLayer.isHidden = !shows
        guard shows else {
            return
        }
        fallbackLayer.string = fallbackText
        effectiveAppearance.performAsCurrentDrawingAppearance {
            fallbackLayer.foregroundColor = Palette.textMuted.cgColor
        }
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updatePlaceholder()
    }

    private func updateAnimation() {
        let wanted = animates && window != nil && content?.canAnimate == true && images != nil
        if wanted, player == nil, animationTask == nil {
            startAnimation()
        } else if !wanted {
            stopAnimation()
        }
    }

    private func startAnimation() {
        guard let images, let spec = spec(animated: AnimatedFormatProbe.current) else {
            return
        }
        let pixels = (spec.width, spec.height)
        animationTask = Task { [weak self] in
            let animation = try? await images.animation(spec)
            // A cancelled ask leaves whatever replaced it alone.
            guard let self, !Task.isCancelled else {
                return
            }
            self.animationTask = nil
            guard let animation else {
                return
            }
            let player = FramePlayer(animation: animation, pixels: pixels, fit: self.fit) { [weak self] image, _ in
                self?.imageLayer.contents = image
                self?.imageLayer.isHidden = false
            }
            self.player = player
            player.start()
        }
    }

    private func stopAnimation() {
        animationTask?.cancel()
        animationTask = nil
        guard let player else {
            return
        }
        player.stop()
        self.player = nil
        imageLayer.contents = still
        imageLayer.isHidden = still == nil
    }
}
