import AkariKit
import AppKit

/// The top bar's row: the author on the left, the action bar on the right. Its empty space takes
/// no clicks, so they reach the scrim and close the viewer.
final class ViewerTopBar: NSView {
    let author = ViewerAuthor()
    let actionBar = ViewerActionBar()

    init() {
        super.init(frame: .zero)
        addSubview(author)
        addSubview(actionBar)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    var avatar: ImageLayerView { author.avatar }
    var name: NSTextField { author.name }
    var time: NSTextField { author.time }
    var buttons: [ViewerButton] { actionBar.buttons }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard !isHidden else {
            return nil
        }
        return actionBar.hitTest(convert(point, from: superview))
    }
}

/// The viewer's page (D146, module 947056): the scrim, the stage, the top bar with Close, the
/// arrows and the strip; the keys while it shows.
final class ViewerView: NSView {
    let model: ViewerModel
    let actions: ViewerActions
    let stage = ViewerStage()
    let topBar = ViewerTopBar()
    let close = ViewerButton(symbol: "xmark", label: "Close", style: .round)
    let previous = ViewerButton(symbol: "arrow.left", label: "Previous", style: .round)
    let next = ViewerButton(symbol: "arrow.right", label: "Next", style: .round)
    let strip = ViewerStrip()
    private let zoomButton = ViewerButton(symbol: "plus.magnifyingglass", label: "Zoom In", style: .bar)
    private let saveButton = ViewerButton(symbol: "arrow.down.to.line", label: "Save Image", style: .bar)
    private let browserButton = ViewerButton(
        symbol: "arrow.up.forward.square", label: "Open in Browser", style: .bar)
    private let moreButton = ViewerButton(symbol: "ellipsis", label: "More", style: .bar)
    var onClose: () -> Void = {}
    var reducesMotion: () -> Bool = { false }
    /// The panel is key and the app active: GIFs and GIFVs play.
    var animates = false {
        didSet {
            if animates != oldValue {
                stage.show(model.current, preloads: model.preloads, several: several, animates: animates)
            }
        }
    }

    private var tracking: NSTrackingArea?
    static let chromeFade: TimeInterval = 0.15
    // Discord's bar stays while it has the focus: zoomed from its own button.
    private var keepsBar = false

    init(model: ViewerModel, images: ImageCache?, placeholders: PlaceholderCache?, actions: ViewerActions) {
        self.model = model
        self.actions = actions
        super.init(frame: .zero)
        wantsLayer = true
        layer?.backgroundColor = ViewerColors.scrim.cgColor
        stage.images = images
        stage.placeholders = placeholders
        stage.playSource = { [weak model] item in await model?.originalURL(of: item) }
        stage.onClose = { [weak self] in self?.onClose() }
        stage.onZoom = { [weak self] zoomed in
            if !zoomed {
                self?.keepsBar = false
            }
            self?.updateButtons()
            self?.updateChrome(animated: true)
            self?.updateCursor()
        }
        for view in [stage, topBar, previous, next, strip, close] as [NSView] {
            addSubview(view)
        }
        topBar.avatar.images = images
        topBar.avatar.show(ImageLayerView.Content(key: model.author.avatar, canAnimate: false))
        topBar.name.stringValue = model.authorName
        let now = Date()
        topBar.time.stringValue = MessageFormat.groupTime(
            model.timestamp, now: now, calendar: .current, locale: .current)
        topBar.time.toolTip = MessageFormat.fullDate(model.timestamp, calendar: .current, locale: .current)
        close.onPress = { [weak self] in self?.onClose() }
        previous.onPress = { [weak self] in self?.go { $0.previous() } }
        next.onPress = { [weak self] in self?.go { $0.next() } }
        zoomButton.onPress = { [weak self] in
            guard let self else {
                return
            }
            self.keepsBar = !self.stage.zoomed
            self.stage.setZoomed(!self.stage.zoomed, at: nil)
        }
        saveButton.onPress = { [weak self] in self?.run { await $0.save() } }
        browserButton.onPress = { [weak self] in self?.run { await $0.openInBrowser() } }
        moreButton.onPress = { [weak self] in self?.showMore() }
        // A hidden strip would still ask for its thumbnails.
        strip.set(model.items.count > 1 ? model.items : [], images: images)
        strip.onSelect = { [weak self] index in self?.go { $0.select(index) } }
        update()
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    private var several: Bool { model.items.count > 1 }

    private func go(_ change: (ViewerModel) -> Void) {
        change(model)
        update()
    }

    private func run(_ action: @escaping (ViewerActions) async -> Void) {
        Task { [actions] in await action(actions) }
    }

    private func update() {
        stage.show(model.current, preloads: model.preloads, several: several, animates: animates)
        strip.select(model.index)
        updateButtons()
        updateChrome(animated: false)
        needsLayout = true
    }

    private func updateButtons() {
        let available = model.actions
        var buttons: [ViewerButton] = []
        if available.contains(.zoom) {
            zoomButton.set(
                symbol: stage.zoomed ? "minus.magnifyingglass" : "plus.magnifyingglass",
                label: stage.zoomed ? "Zoom Out" : "Zoom In")
            buttons.append(zoomButton)
        }
        if available.contains(.save) {
            saveButton.set(
                symbol: "arrow.down.to.line", label: model.current.kind == .video ? "Save Video" : "Save Image")
            buttons.append(saveButton)
        }
        if available.contains(.openInBrowser) {
            buttons.append(browserButton)
        }
        if !Set(available).isDisjoint(with: [.copyImage, .copyLink, .details]) {
            buttons.append(moreButton)
        }
        if buttons != topBar.buttons {
            topBar.actionBar.set(buttons)
            needsLayout = true
        }
    }

    // Module 947056 fades the author, the action bar, the arrows and the strip while zoomed;
    // Close stays.
    private func updateChrome(animated: Bool) {
        let shown: [(NSView, Bool)] = [
            (topBar.author, true), (topBar.actionBar, !topBar.buttons.isEmpty), (previous, several),
            (next, several), (strip, several),
        ]
        let animated = animated && !reducesMotion()
        for (view, wanted) in shown {
            let visible = wanted && (!stage.zoomed || (view === topBar.actionBar && keepsBar))
            if visible {
                view.isHidden = false
                if animated {
                    NSAnimationContext.runAnimationGroup { context in
                        context.duration = Self.chromeFade
                        view.animator().alphaValue = 1
                    }
                } else {
                    view.alphaValue = 1
                }
            } else if animated, wanted, !view.isHidden {
                NSAnimationContext.runAnimationGroup { context in
                    context.duration = Self.chromeFade
                    context.timingFunction = CAMediaTimingFunction(name: .easeOut)
                    view.animator().alphaValue = 0
                } completionHandler: { [weak self, weak view] in
                    MainActor.assumeIsolated {
                        if self?.stage.zoomed == true {
                            view?.isHidden = true
                        }
                    }
                }
            } else {
                view.isHidden = true
                view.alphaValue = 0
            }
        }
    }

    override func layout() {
        super.layout()
        stage.frame = bounds
        let frames = ViewerLayout.frames(in: bounds.size, items: model.items.count, buttons: topBar.buttons.count)
        let top = frames.close.minY
        topBar.frame = NSRect(x: 0, y: top, width: bounds.width, height: ViewerLayout.row)
        topBar.author.frame = topBar.bounds
        topBar.author.place(
            avatar: frames.avatar.offsetBy(dx: 0, dy: -top), author: frames.author.offsetBy(dx: 0, dy: -top))
        topBar.actionBar.frame = (frames.actions ?? .zero).offsetBy(dx: 0, dy: -top)
        close.frame = frames.close
        previous.frame = frames.previous ?? .zero
        next.frame = frames.next ?? .zero
        strip.frame = frames.strip ?? .zero
    }

    // The viewer's one cursor authority: cursor rects go stale in a panel that takes the key
    // before its content is laid out (cursors.md).
    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let tracking {
            removeTrackingArea(tracking)
        }
        let tracking = NSTrackingArea(
            rect: .zero, options: [.mouseMoved, .cursorUpdate, .activeInActiveApp, .inVisibleRect],
            owner: self)
        addTrackingArea(tracking)
        self.tracking = tracking
    }

    override func cursorUpdate(with event: NSEvent) {
        updateCursor()
    }

    override func mouseMoved(with event: NSEvent) {
        updateCursor()
    }

    private func updateCursor() {
        guard let window else {
            return
        }
        cursor(at: convert(window.mouseLocationOutsideOfEventStream, from: nil))?.set()
    }

    /// The cursor at `point` (this view's space); nil over a video, whose player sets its own.
    func cursor(at point: NSPoint) -> NSCursor? {
        guard bounds.contains(point), let hit = hitTest(convert(point, to: superview)) else {
            return .arrow
        }
        if hit is NSButton {
            return .pointingHand
        }
        if hit === stage {
            return stage.cursor(at: convert(point, to: stage))
        }
        return hit.isDescendant(of: stage) ? nil : .arrow
    }

    // → ↓ Space next, ← ↑ previous (module 803842); a video's player would seek with them.
    func handleKey(_ event: NSEvent) -> Bool {
        guard event.modifierFlags.isDisjoint(with: [.command, .control, .option]) else {
            return false
        }
        switch event.keyCode {
        case 124, 125, 49:
            go { $0.next() }
        case 123, 126:
            go { $0.previous() }
        default:
            return false
        }
        return true
    }

    // The app's shortcuts are off while the viewer shows; ⌘Q and ⌘H still reach the menu.
    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard event.modifierFlags.contains(.command) else {
            return super.performKeyEquivalent(with: event)
        }
        switch event.charactersIgnoringModifiers?.lowercased() {
        case "q", "h":
            return false
        case "w":
            onClose()
            return true
        default:
            return true
        }
    }

    private func showMore() {
        guard let menu = moreMenu() else {
            return
        }
        let below = moreButton.isFlipped ? moreButton.bounds.maxY + 4 : -4
        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: below), in: moreButton)
    }

    func moreMenu() -> NSMenu? {
        let available = model.actions
        var items: [NSMenuItem] = []
        if available.contains(.copyImage) {
            items.append(MessageMenu.item("Copy Image") { [weak self] in self?.run { await $0.copyImage() } })
        }
        if available.contains(.copyLink) {
            items.append(MessageMenu.item("Copy Media Link") { [weak self] in self?.run { await $0.copyLink() } })
        }
        let details = ViewerModel.details(of: model.current)
        if available.contains(.details), !details.isEmpty {
            let submenu = NSMenu()
            for detail in details {
                let item = MessageMenu.item(detail.title) { [actions] in actions.copy(detail.value) }
                if #available(macOS 14.4, *) {
                    item.subtitle = detail.value
                } else {
                    item.title = "\(detail.title): \(detail.value)"
                }
                submenu.addItem(item)
            }
            let item = NSMenuItem(title: "View Details", action: nil, keyEquivalent: "")
            item.submenu = submenu
            items.append(item)
        }
        let menu = MessageMenu.menu(items)
        menu?.appearance = NSAppearance(named: .darkAqua)
        return menu
    }

    /// Lets every image and video go.
    func stop() {
        stage.stop()
        strip.set([], images: nil)
        topBar.avatar.show(nil)
    }
}
