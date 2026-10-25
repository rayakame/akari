import AkariKit
import AppKit

/// A box's image as a layer (D135): asked from the cache at the box's pixels on the window's
/// backing scale, set as layer contents at that scale on whole backing pixels, clipped by the
/// layer. It asks when shown in a window, cancels when it shows something else or leaves, asks
/// again when the backing scale changes, and plays the animated form through a `FramePlayer`
/// while `animates` is on. It draws nothing and tracks no pointer: the list stays the authority.
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
        /// Discord's disc while an avatar loads, kept when it fails.
        case disc
    }

    var images: ImageCache? {
        didSet {
            if images !== oldValue {
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
    /// Whether the owner wants the animation (the motion policy's say); nothing without an
    /// animated form.
    var animates = false {
        didSet {
            if animates != oldValue {
                updateAnimation()
            }
        }
    }
    private(set) var content: Content?
    /// The still failed to load; the placeholder stays.
    private(set) var failed = false
    let imageLayer = CALayer()
    let placeholderLayer = CALayer()
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
            layer.actions = ["contents": NSNull(), "hidden": NSNull(), "bounds": NSNull(), "position": NSNull()]
            layer.masksToBounds = true
            layer.isHidden = true
            self.layer?.addSublayer(layer)
        }
        imageLayer.contentsGravity = .resizeAspectFill
        updatePlaceholder()
    }

    required init?(coder: NSCoder) {
        nil
    }

    /// Shows `content`: the cached image at once, else the placeholder (or the old image, until
    /// the new one has decoded) while it loads. `nil` clears the box.
    func show(_ content: Content?) {
        guard content != self.content else {
            return
        }
        self.content = content
        failed = false
        cancelLoads()
        if content == nil {
            still = nil
            imageLayer.contents = nil
            imageLayer.isHidden = true
        }
        updatePlaceholder()
        reload()
    }

    // Clicks and the pointer belong to what lies around the box.
    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil {
            cancelLoads()
        } else {
            reload()
        }
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        let scale = window?.backingScaleFactor ?? 1
        imageLayer.contentsScale = scale
        placeholderLayer.contentsScale = scale
        // The size in pixels changed: ask at it, keeping the old image until the new one decoded.
        if content != nil, let asked = askedPixels, asked != pixels {
            cancelLoads(keepingImage: true)
            reload()
        }
    }

    override func layout() {
        super.layout()
        let scale = window?.backingScaleFactor ?? 1
        let frame = Self.alignedFrame(bounds, scale: scale)
        imageLayer.frame = frame
        placeholderLayer.frame = frame
        if content != nil, let asked = askedPixels, asked != pixels {
            cancelLoads(keepingImage: true)
            reload()
        }
    }

    /// `bounds` moved onto whole backing pixels at `scale`: the origin to the nearest pixel,
    /// the size rounded up to whole pixels, so a bitmap decoded at the box's pixels draws 1:1.
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

    private var pixels: (Int, Int) {
        Self.pixels(bounds.size, scale: window?.backingScaleFactor ?? 1)
    }

    private func spec(animated: AnimatedFormat? = nil) -> ImageSpec? {
        guard let content else {
            return nil
        }
        let pixels = pixels
        guard pixels.0 > 0, pixels.1 > 0 else {
            return nil
        }
        return ImageSpec(key: content.key, width: pixels.0, height: pixels.1, animated: animated)
    }

    private func reload() {
        guard window != nil, let images, stillTask == nil, let spec = spec() else {
            return
        }
        askedPixels = pixels
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
                self.updatePlaceholder()
            }
        }
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
    }

    private func cancelLoads(keepingImage: Bool = false) {
        stillTask?.cancel()
        stillTask = nil
        stopAnimation()
        if let pinned {
            images?.unpin(pinned)
            self.pinned = nil
        }
        askedPixels = nil
        if !keepingImage {
            still = nil
        }
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
        let showsDisc = placeholder == .disc && content != nil && (still == nil || failed)
        placeholderLayer.isHidden = !showsDisc
        if showsDisc {
            effectiveAppearance.performAsCurrentDrawingAppearance {
                placeholderLayer.backgroundColor = Palette.avatarPlaceholder.cgColor
            }
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
        let pixels = pixels
        animationTask = Task { [weak self] in
            guard let animation = try? await images.animation(spec) else {
                self?.animationTask = nil
                return
            }
            guard let self, !Task.isCancelled else {
                return
            }
            self.animationTask = nil
            let player = FramePlayer(animation: animation, pixels: pixels) { [weak self] image, _ in
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
