import AVKit
import AkariFFI
import AkariKit
import AppKit
import Testing
import os

@testable import Akari

/// The account's originals and saves, as the viewer's actions use them.
nonisolated final class ViewerAccount: Account, @unchecked Sendable {
    let links = OSAllocatedUnfairLock<[ImageKey: String]>(initialState: [:])
    let saves = OSAllocatedUnfairLock<[(ImageKey, String)]>(initialState: [])
    /// Holds every link back until it fires.
    let linkGate = OSAllocatedUnfairLock<Gate?>(initialState: nil)
    let failsSaves = OSAllocatedUnfairLock(initialState: false)

    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    override func originalUrl(key: ImageKey) async throws -> String {
        if let gate = linkGate.withLock({ $0 }) {
            await gate.wait()
        }
        guard let link = links.withLock({ $0[key] }) else {
            throw ImageError.Unavailable
        }
        return link
    }

    override func saveOriginal(key: ImageKey, destination: String) -> PendingSave {
        saves.withLock { $0.append((key, destination)) }
        return WrittenSave(destination: destination, fails: failsSaves.withLock { $0 })
    }
}

nonisolated final class WrittenSave: PendingSave, @unchecked Sendable {
    let destination: String
    let fails: Bool

    init(destination: String, fails: Bool = false) {
        self.destination = destination
        self.fails = fails
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    override func result() async throws -> UInt64 {
        if fails {
            throw ImageError.Unavailable
        }
        // A 2 × 2 PNG, as the core writes the original.
        let image = NSImage(size: NSSize(width: 2, height: 2), flipped: false) { rect in
            NSColor.red.setFill()
            rect.fill()
            return true
        }
        let data = image.tiffRepresentation.flatMap { NSBitmapImageRep(data: $0)?.representation(using: .png, properties: [:]) }
        try data?.write(to: URL(fileURLWithPath: destination))
        return UInt64(data?.count ?? 0)
    }

    override func cancel() {}
}

// The media viewer (D146, canary 653948): over the session's window, Discord's fit, zoom at the
// pointer, keys, the strip, the top bar and its actions; nothing it asked outlives it.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class ViewerTests {
    final class KeyWindow: NSWindow {
        override var isKeyWindow: Bool { true }
        override var canBecomeKey: Bool { true }
    }

    let window: KeyWindow
    let account = ViewerAccount()
    let source = FakeImageSource.stalling()
    let cache: ImageCache
    let viewer = MediaViewer()
    let pasteboard = NSPasteboard(name: NSPasteboard.Name("akari.viewer.tests.\(UUID().uuidString)"))
    var opened: [URL] = []
    var keyed: [NSWindow] = []

    init() {
        window = KeyWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1280, height: 800), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.animationBehavior = .none
        window.contentView = NSView()
        window.orderFrontRegardless()
        cache = ImageCache(source: source)
        viewer.pasteboard = pasteboard
        viewer.reducesMotion = { true }
        viewer.focus = WindowFocus { [weak self] in self?.keyed.append($0) }
        viewer.openURL = { [weak self] in self?.opened.append($0) }
    }

    deinit {
        nonisolated(unsafe) let window = window
        nonisolated(unsafe) let viewer = viewer
        DispatchQueue.main.async {
            MainActor.assumeIsolated {
                viewer.close()
                window.close()
            }
        }
    }

    func item(_ raw: UInt64, _ name: String = "a.png", kind: AttachmentKind = .image, width: UInt32 = 4000, height: UInt32 = 3000)
        -> ViewerModel.Item
    {
        ViewerModel.Item(
            AkariFFI.Attachment(
                id: AttachmentId(rawValue: raw), filename: name, title: nil, description: nil,
                contentType: nil, size: 2_000_000, kind: kind, width: width, height: height,
                placeholder: nil, spoiler: false, animated: false,
                key: .attachment(
                    channelId: ChannelId(rawValue: 10), messageId: MessageId(rawValue: 1),
                    attachmentId: AttachmentId(rawValue: raw)),
                display: .media, cover: nil))
    }

    func open(
        _ items: [ViewerModel.Item], at index: Int = 0, hidesDownloads: Bool = false, images: ImageCache? = nil
    ) throws -> ViewerModel {
        let model = try #require(
            ViewerModel(
                items: items, index: index, author: author(2, "Ray"), authorName: "Ray",
                timestamp: Date(), hidesDownloads: hidesDownloads, account: account))
        viewer.open(model, in: window, images: images ?? cache)
        return model
    }

    func view() throws -> ViewerView {
        try #require(viewer.view)
    }

    func key(_ code: UInt16, _ characters: String) throws {
        let panel = try #require(viewer.panel)
        panel.sendEvent(
            try #require(
                NSEvent.keyEvent(
                    with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                    windowNumber: panel.windowNumber, context: nil, characters: characters,
                    charactersIgnoringModifiers: characters, isARepeat: false, keyCode: code)))
    }

    /// A press and release at `point` (the viewer's top-left coordinates), handed to what's hit.
    func click(at point: NSPoint, to: NSPoint? = nil) throws {
        let view = try view()
        let panel = try #require(viewer.panel)
        let inWindow = view.convert(point, to: nil)
        let end = view.convert(to ?? point, to: nil)
        let event = { (type: NSEvent.EventType, at: NSPoint) in
            try #require(
                NSEvent.mouseEvent(
                    with: type, location: at, modifierFlags: [], timestamp: 0, windowNumber: panel.windowNumber,
                    context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        }
        let target = try #require(view.hitTest(view.convert(point, to: view.superview)))
        target.mouseDown(with: try event(.leftMouseDown, inWindow))
        if to != nil {
            target.mouseDragged(with: try event(.leftMouseDragged, end))
        }
        target.mouseUp(with: try event(.leftMouseUp, end))
    }

    func mediaRequests() -> [ImageRequest] {
        source.requests.withLock { $0.filter { if case .attachment = $0.key { true } else { false } } }
    }

    // Over the whole session window, Discord's scrim (black at 0.92), the panel key.
    @Test
    func itOpensOverTheWindow() throws {
        _ = try open([item(1)])
        let panel = try #require(viewer.panel)

        #expect(panel.frame == window.frame)
        #expect(keyed.contains { $0 === panel })
        #expect(try view().layer?.backgroundColor == NSColor.black.withAlphaComponent(0.92).cgColor)
        #expect(panel.accessibilityLabel() == "Media Viewer")
    }

    // The media sits in Discord's box, centred, asked at its pixels.
    @Test
    func theMediaFitsAndIsAskedAtItsPixels() async throws {
        _ = try open([item(1)])
        let view = try view()
        let size = view.bounds.size
        let fitted = ViewerLayout.fit(NSSize(width: 4000, height: 3000), in: size, several: false)

        #expect(view.stage.mediaFrame.size == fitted)
        #expect(abs(view.stage.mediaFrame.midX - size.width / 2) < 1 && abs(view.stage.mediaFrame.midY - size.height / 2) < 1)
        try await eventually { !self.mediaRequests().isEmpty }
        let scale = window.backingScaleFactor
        #expect(mediaRequests().first?.width == UInt32((fitted.width * scale).rounded(.up)))
    }

    // → ↓ Space next, ← ↑ previous, wrapping; Escape closes.
    @Test
    func keysMoveWrapAndClose() throws {
        let model = try open([item(1), item(2), item(3)])

        try key(124, "\u{F703}")
        #expect(model.index == 1)
        try key(125, "\u{F701}")
        #expect(model.index == 2)
        try key(49, " ")
        #expect(model.index == 0, "wraps")
        try key(123, "\u{F702}")
        #expect(model.index == 2)
        try key(126, "\u{F700}")
        #expect(model.index == 1)
        try key(53, "\u{1b}")
        #expect(viewer.panel == nil)
    }

    // Module 803842 replaces the app's shortcuts while it shows: they're swallowed, but ⌘Q and
    // ⌘H still reach the menu and ⌘W closes.
    @Test
    func shortcutsStayWithTheViewer() throws {
        _ = try open([item(1)])
        let view = try view()
        let press = { (flags: NSEvent.ModifierFlags, characters: String, ignoring: String?, code: UInt16) in
            try #require(
                NSEvent.keyEvent(
                    with: .keyDown, location: .zero, modifierFlags: flags, timestamp: 0,
                    windowNumber: 0, context: nil, characters: characters,
                    charactersIgnoringModifiers: ignoring ?? characters, isARepeat: false, keyCode: code))
        }

        #expect(view.performKeyEquivalent(with: try press(.command, "k", nil, 40)))
        #expect(view.performKeyEquivalent(with: try press(.control, "k", nil, 40)))
        #expect(view.performKeyEquivalent(with: try press(.option, "˚", "k", 40)))
        #expect(view.performKeyEquivalent(with: try press(.function, "f", nil, 3)), "Fn-F, full screen")
        #expect(!view.performKeyEquivalent(with: try press([.function, .numericPad], "\u{F703}", nil, 124)))
        #expect(!view.performKeyEquivalent(with: try press(.command, "q", nil, 12)))
        #expect(!view.performKeyEquivalent(with: try press(.command, "h", nil, 4)))
        // A Russian layout's ⌘ keymap: the letter is Latin with ⌘ only.
        #expect(!view.performKeyEquivalent(with: try press(.command, "q", "й", 12)))
        #expect(!view.performKeyEquivalent(with: try press([.command, .option], "h", "р", 4)))
        #expect(viewer.panel != nil)
        #expect(view.performKeyEquivalent(with: try press(.command, "w", "ц", 13)))
        #expect(viewer.panel == nil)
    }

    // Module 591818 and the modal's root: a click on the media zooms, on zoomed media zooms out;
    // a click on the scrim or the top bar's empty space closes.
    @Test
    func theScrimClosesAndTheMediaZooms() throws {
        _ = try open([item(1)])
        let view = try view()
        let media = view.stage.mediaFrame

        try click(at: NSPoint(x: media.midX + 100, y: media.midY))
        #expect(view.stage.zoomed)
        #expect(view.stage.mediaFrame.size == NSSize(width: media.width * 2.5, height: media.height * 2.5))
        try click(at: NSPoint(x: media.midX + 100, y: media.midY))
        #expect(!view.stage.zoomed)
        #expect(viewer.panel != nil)

        try click(at: NSPoint(x: 5, y: view.bounds.height - 5))
        #expect(viewer.panel == nil)

        _ = try open([item(1)])
        try click(at: NSPoint(x: 400, y: ViewerLayout.marginY + 20))
        #expect(viewer.panel == nil, "the top bar between the author and the buttons")
    }

    // Module 591818: a click zooms 2.5× keeping the point under the pointer; a click while zoomed
    // zooms out, a drag pans instead; the zoomed media is asked at more pixels, never above the
    // source.
    @Test
    func zoomFollowsThePointerAndDragsPan() async throws {
        _ = try open([item(1)])
        let view = try view()
        let size = view.bounds.size
        let media = view.stage.mediaFrame
        let point = NSPoint(x: media.midX + 100, y: media.midY + 40)

        try click(at: point)
        let expected = ViewerLayout.clamp(
            ViewerLayout.zoomOffset(at: point, in: size),
            media: NSSize(width: media.width * 2.5, height: media.height * 2.5), in: size)
        #expect(view.stage.offset == expected)
        try await eventually { self.mediaRequests().contains { $0.width > UInt32(media.width * self.window.backingScaleFactor) + 1 } }
        #expect(mediaRequests().allSatisfy { $0.width <= 4000 && $0.height <= 3000 }, "never above the source")

        let zoomedFrame = view.stage.mediaFrame
        try click(at: NSPoint(x: zoomedFrame.midX, y: zoomedFrame.midY), to: NSPoint(x: zoomedFrame.midX - 60, y: zoomedFrame.midY))
        #expect(view.stage.zoomed, "a drag pans")
        #expect(view.stage.offset.x == ViewerLayout.clamp(NSPoint(x: expected.x - 60, y: expected.y), media: zoomedFrame.size, in: size).x)

        try click(at: NSPoint(x: view.stage.mediaFrame.midX, y: view.stage.mediaFrame.midY))
        #expect(!view.stage.zoomed)
        #expect(view.stage.mediaFrame == media)
    }

    // Switching items resets the zoom; what the old item asked is let go.
    @Test
    func switchingResetsTheZoomAndLetsTheOldItemGo() async throws {
        let model = try open([item(1), item(2)])
        let view = try view()
        let fitted = view.stage.mediaFrame.width * window.backingScaleFactor
        try click(at: NSPoint(x: view.stage.mediaFrame.midX, y: view.stage.mediaFrame.midY))
        try await eventually { self.zoomedRequest(of: 1, above: fitted) != nil }
        let zoomed = try #require(zoomedRequest(of: 1, above: fitted))
        let cancelled = source.cancelled.withLock { $0 }

        try key(124, "\u{F703}")

        #expect(model.index == 1)
        #expect(!view.stage.zoomed)
        #expect(!cache.isPinned(ImageSpec(key: zoomed.key, width: Int(zoomed.width), height: Int(zoomed.height))))
        try await eventually { self.source.cancelled.withLock { $0 } > cancelled }
    }

    func zoomedRequest(of raw: UInt64, above width: CGFloat) -> ImageRequest? {
        mediaRequests().first { $0.key == item(raw).key && CGFloat($0.width) > width + 1 }
    }

    // Closing lets every request, pin and player go.
    @Test
    func closingLetsEverythingGo() async throws {
        let video = item(4, "clip.mp4", kind: .video)
        account.links.withLock { $0[video.key] = "https://cdn.discordapp.com/attachments/10/4/clip.mp4" }
        _ = try open([video, item(2), item(3)])
        let view = try view()
        try await eventually { self.cache.inflightCount > 0 && view.stage.playerView != nil }
        let player = try #require(view.stage.playerView?.player)

        viewer.close()

        #expect(viewer.panel == nil)
        try await eventually { self.cache.inflightCount == 0 }
        #expect(nothingPinned())
        #expect(view.stage.playerView == nil)
        #expect(player.currentItem == nil)
    }

    func nothingPinned() -> Bool {
        source.requests.withLock { $0 }.allSatisfy {
            !cache.isPinned(ImageSpec(key: $0.key, width: Int($0.width), height: Int($0.height)))
        }
    }

    // With motion the panel fades out over 0.2 s: it takes no click or key meanwhile, and lets
    // everything go once gone.
    @Test
    func theFadingPanelTakesNothing() async throws {
        viewer.reducesMotion = { false }
        let model = try open([item(1), item(2)])
        let panel = try #require(viewer.panel)
        try await eventually { self.cache.inflightCount > 0 }

        viewer.close()

        #expect(panel.ignoresMouseEvents)
        panel.sendEvent(
            try #require(
                NSEvent.keyEvent(
                    with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                    windowNumber: panel.windowNumber, context: nil, characters: " ",
                    charactersIgnoringModifiers: " ", isARepeat: false, keyCode: 49)))
        #expect(model.index == 0)
        try await eventually { !panel.isVisible }
        try await eventually { self.cache.inflightCount == 0 }
        #expect(nothingPinned())
    }

    // A sheet on the viewer ends with it; a failure after it closed shows on its window.
    @Test
    func sheetsGoWithTheViewer() async throws {
        _ = try open([item(1)])
        let panel = try #require(viewer.panel)
        let actions = try view().actions

        actions.onFailure(ViewerActions.saveFailed)
        try await eventually { panel.attachedSheet != nil }
        viewer.close()
        #expect(panel.attachedSheet == nil)

        actions.onFailure(ViewerActions.saveFailed)
        try await eventually { self.window.attachedSheet != nil }
        if let sheet = window.attachedSheet {
            window.endSheet(sheet)
        }
    }

    // What a button started and hadn't done yet goes with the viewer.
    @Test
    func pressedActionsGoWithTheViewer() async throws {
        let first = item(1)
        let gate = Gate()
        account.links.withLock { $0[first.key] = "https://cdn.discordapp.com/attachments/10/1/a.png" }
        account.linkGate.withLock { $0 = gate }
        _ = try open([first])
        let browser = try #require(try view().topBar.buttons.first { $0.label == "Open in Browser" })

        browser.performClick(nil)
        viewer.close()
        gate.fire()
        try await Task.sleep(for: .milliseconds(100))

        #expect(opened.isEmpty)
    }

    // The top bar: the author's avatar, the list's name, the time as the group's header with
    // the full date as tooltip.
    @Test
    func theTopBarNamesTheAuthor() throws {
        _ = try open([item(1)])
        let bar = try view().topBar

        #expect(bar.name.stringValue == "Ray")
        #expect(!bar.time.stringValue.isEmpty)
        let time = NSPoint(x: bar.time.frame.midX, y: bar.time.frame.midY)
        #expect(bar.author.hitTest(bar.author.convert(time, to: bar.author.superview)) === bar.author)
        let tip = bar.author.view(bar.author, stringForToolTip: 0, point: time, userData: nil)
        #expect(tip == MessageFormat.fullDate(try view().model.timestamp, calendar: .current, locale: .current))
        #expect(bar.avatar.content?.key == author(2, "Ray").avatar)

        // The author block is the modal's root's: a click there closes.
        let avatar = bar.avatar.convert(NSPoint(x: bar.avatar.bounds.midX, y: bar.avatar.bounds.midY), to: try view())
        try click(at: avatar)
        #expect(viewer.panel == nil)
    }

    // Module 947056's buttons by the item; hidden download options leave Zoom.
    @Test
    func theButtonsFollowTheItem() throws {
        _ = try open([item(1), item(2, "clip.mp4", kind: .video)])
        let bar = try view().topBar
        #expect(bar.buttons.map { $0.accessibilityLabel() } == ["Zoom In", "Save Image", "Open in Browser", "More"])

        try key(124, "\u{F703}")
        #expect(bar.buttons.map { $0.accessibilityLabel() } == ["Save Video", "Open in Browser", "More"])

        viewer.close()
        _ = try open([item(1)], hidesDownloads: true)
        #expect(try view().topBar.buttons.map { $0.accessibilityLabel() } == ["Zoom In"])
    }

    // The strip: one square per item, the others dimmed; a click selects. One item: no strip and
    // no arrows.
    @Test
    func theStripSelects() throws {
        let model = try open([item(1), item(2), item(3)])
        let strip = try view().strip

        #expect(strip.items.count == 3)
        #expect(strip.items.map(\.dimmed) == [false, true, true])
        strip.items[2].performClick(nil)
        #expect(model.index == 2)
        #expect(strip.items.map(\.dimmed) == [true, true, false])
        #expect(strip.items[2].accessibilityLabel() == "Item 3 of 3 (current)")

        viewer.close()
        _ = try open([item(1)])
        #expect(try view().strip.isHidden)
        #expect(try view().previous.isHidden && view().next.isHidden)
    }

    // Copy Media Link and Open in Browser use the original; Save writes it where the panel says.
    @Test
    func theActionsUseTheOriginal() async throws {
        let first = item(1)
        account.links.withLock { $0[first.key] = "https://cdn.discordapp.com/attachments/10/1/a.png" }
        let destination = FileManager.default.temporaryDirectory.appendingPathComponent("viewer-save-\(UUID().uuidString).png")
        viewer.chooseSaveURL = { _ in destination }
        _ = try open([first])
        let actions = try view().actions

        await actions.copyLink()
        #expect(pasteboard.string(forType: .string) == "https://cdn.discordapp.com/attachments/10/1/a.png")
        await actions.openInBrowser()
        #expect(opened == [URL(string: "https://cdn.discordapp.com/attachments/10/1/a.png")!])
        await actions.save()
        #expect(account.saves.withLock { $0.map(\.1) } == [destination.path])
        await actions.copyImage()
        #expect(pasteboard.canReadObject(forClasses: [NSImage.self]))
        try? FileManager.default.removeItem(at: destination)
    }

    // A cancelled save panel saves nothing; a failed save says so; an embed's image is saved
    // under its link's name.
    @Test
    func savingCancelledFailedAndNamed() async throws {
        let image = EmbedImage(
            key: .embedMedia(channelId: ChannelId(rawValue: 10), messageId: MessageId(rawValue: 1), embed: 0, slot: .image),
            width: 640, height: 480, placeholder: nil, animated: false)
        account.links.withLock { $0[image.key] = "https://media.discordapp.net/external/abc/https/example.com/photo.jpg" }
        var names: [String] = []
        var destination: URL?
        viewer.chooseSaveURL = { name in
            names.append(name)
            return destination
        }
        _ = try open([ViewerModel.Item(image)])
        let actions = try view().actions
        var failures: [String] = []
        actions.onFailure = { failures.append($0) }

        await actions.save()
        #expect(names == ["photo.jpg"])
        #expect(account.saves.withLock { $0.isEmpty }, "cancelled")

        destination = FileManager.default.temporaryDirectory.appendingPathComponent("viewer-\(UUID().uuidString).jpg")
        account.failsSaves.withLock { $0 = true }
        await actions.save()
        #expect(failures == [ViewerActions.saveFailed])
    }

    // Module 947056 asks for the previous item with two or more and the next with three or more,
    // at their fitted pixels, so they show at once.
    @Test
    func neighboursAreAskedEarly() async throws {
        _ = try open([item(1), item(2, width: 1000, height: 2000), item(3)])
        let view = try view()
        #expect(view.stage.preloadKeys == [item(3).key, item(2).key])
        let fitted = ViewerLayout.fit(NSSize(width: 1000, height: 2000), in: view.bounds.size, several: true)
        let pixels = UInt32((fitted.width * window.backingScaleFactor).rounded(.up))
        // Its strip thumbnail asks too, at 40 points.
        try await eventually { self.mediaRequests().contains { $0.key == self.item(2).key && $0.width == pixels } }

        try key(124, "\u{F703}")
        #expect(view.stage.preloadKeys == [item(1).key, item(3).key])
        view.layoutSubtreeIfNeeded()
        #expect(mediaRequests().filter { $0.key == item(2).key && $0.width == pixels }.count == 1, "the same ask")
    }

    // Zoomed, the author, the arrows, the strip and the action bar fade (module 947056); Close
    // stays, and so does the bar when its own Zoom button zoomed (it keeps the focus).
    @Test
    func zoomedTheChromeGives() throws {
        _ = try open([item(1), item(2)])
        let view = try view()
        let chrome: [NSView] = [view.topBar.author, view.topBar.actionBar, view.previous, view.next, view.strip]
        #expect(chrome.allSatisfy { !$0.isHidden })

        try click(at: NSPoint(x: view.stage.mediaFrame.midX, y: view.stage.mediaFrame.midY))
        #expect(chrome.allSatisfy { $0.isHidden })
        #expect(!view.close.isHidden)
        try click(at: NSPoint(x: view.stage.mediaFrame.midX, y: view.stage.mediaFrame.midY))
        #expect(chrome.allSatisfy { !$0.isHidden })

        let zoom = try #require(view.topBar.buttons.first)
        zoom.performClick(nil)
        #expect(view.stage.zoomed)
        #expect(view.stage.offset == .zero, "around the middle")
        #expect(!view.topBar.actionBar.isHidden)
        #expect(view.topBar.author.isHidden)
        #expect(zoom.label == "Zoom Out")
        zoom.performClick(nil)
        #expect(!view.stage.zoomed)
        #expect(zoom.label == "Zoom In")
    }

    // Zoomed, scrolling pans within the window's edges; unzoomed it does nothing.
    @Test
    func scrollingPansZoomedMedia() throws {
        _ = try open([item(1)])
        let view = try view()
        let scroll = { (dx: Int32, dy: Int32) in
            let event = try #require(
                CGEvent(
                    scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 2, wheel1: dy, wheel2: dx,
                    wheel3: 0))
            view.stage.scrollWheel(with: try #require(NSEvent(cgEvent: event)))
        }
        try scroll(0, 30)
        #expect(view.stage.offset == .zero)

        try click(at: NSPoint(x: view.stage.mediaFrame.midX, y: view.stage.mediaFrame.midY))
        try scroll(-40, 30)
        #expect(view.stage.offset == NSPoint(x: -40, y: 30))
        try scroll(-100_000, 0)
        let bound = (view.stage.mediaFrame.width - view.bounds.width) / 2
        #expect(view.stage.offset.x == -bound)
    }

    // It covers its window as the window's size changes, and goes when the window minimizes.
    @Test
    func itFollowsItsWindow() throws {
        _ = try open([item(1)])
        let panel = try #require(viewer.panel)

        window.setFrame(NSRect(x: 0, y: 0, width: 900, height: 600), display: false)
        #expect(panel.frame == window.frame)
        try view().layoutSubtreeIfNeeded()
        #expect(try view().stage.mediaFrame.size == ViewerLayout.fit(NSSize(width: 4000, height: 3000), in: try view().bounds.size, several: false))

        NotificationCenter.default.post(name: NSWindow.willMiniaturizeNotification, object: window)
        #expect(viewer.panel == nil)
    }

    // F-6: a video plays its original with the Mac's controls, once; the next item stops it.
    @Test
    func aVideoPlaysItsOriginalOnce() async throws {
        let video = item(2, "clip.mp4", kind: .video, width: 1280, height: 720)
        let url = "https://cdn.discordapp.com/attachments/10/2/clip.mp4"
        account.links.withLock { $0[video.key] = url }
        _ = try open([video, item(1)])
        let view = try view()

        try await eventually { view.stage.playerView != nil }
        #expect(view.stage.playerView?.isHidden == true, "hidden as it joins")
        let player = try #require(view.stage.playerView?.player)
        #expect((player.currentItem?.asset as? AVURLAsset)?.url == URL(string: url))
        #expect(player.rate > 0, "playing")
        #expect(!player.isMuted)
        #expect(view.stage.playerView?.frame == view.stage.mediaFrame)
        #expect(view.stage.playerView?.isHidden == true, "the poster until the first frame")
        let middle = NSPoint(x: view.stage.mediaFrame.midX, y: view.stage.mediaFrame.midY)
        #expect(view.stage.hitTest(view.stage.convert(middle, to: view)) === view.stage)

        try key(49, " ")
        #expect(view.stage.playerView == nil)
        #expect(player.rate == 0)
        #expect(player.currentItem == nil)
    }

    // A GIFV loops its video over its poster while the viewer may animate.
    @Test
    func aGifvLoops() async throws {
        let poster = EmbedImage(
            key: .embedMedia(channelId: ChannelId(rawValue: 10), messageId: MessageId(rawValue: 1), embed: 0, slot: .thumbnail),
            width: 498, height: 280, placeholder: nil, animated: false)
        let video = ImageKey.embedMedia(channelId: ChannelId(rawValue: 10), messageId: MessageId(rawValue: 1), embed: 0, slot: .video)
        account.links.withLock { $0[video] = "file:///dev/null" }
        var plays = false
        viewer.plays = { _ in plays }
        _ = try open([ViewerModel.Item(poster, video: video)])
        let view = try view()
        let panel = try #require(viewer.panel)
        #expect(!view.stage.tile.isLooping, "the panel isn't key")

        plays = true
        NotificationCenter.default.post(name: NSWindow.didBecomeKeyNotification, object: panel)
        try await eventually { view.stage.tile.isLooping }
        plays = false
        NotificationCenter.default.post(name: NSApplication.didResignActiveNotification, object: NSApp)
        #expect(!view.stage.tile.isLooping)
    }

    // More: Copy Image for still images, Copy Media Link, View Details with each value copied.
    @Test
    func moreOffersCopiesAndDetails() throws {
        _ = try open([item(1, "IMG_1.png")])
        let menu = try #require(try view().moreMenu())
        #expect(menu.items.map(\.title) == ["Copy Image", "Copy Media Link", "View Details"])
        let details = try #require(menu.items.last?.submenu)
        #expect(details.items.map(\.title) == ["Filename", "Size"])
        details.performActionForItem(at: 1)
        #expect(pasteboard.string(forType: .string) == "4000x3000 (2 MB)")

        viewer.close()
        _ = try open([item(2, "clip.mp4", kind: .video)])
        #expect(try view().moreMenu()?.items.map(\.title) == ["Copy Media Link", "View Details"])
    }

    // Arrows with ⌘, ⌥ or ⌃ aren't the viewer's.
    @Test
    func modifiedArrowsStay() throws {
        let model = try open([item(1), item(2)])
        let panel = try #require(viewer.panel)
        panel.sendEvent(
            try #require(
                NSEvent.keyEvent(
                    with: .keyDown, location: .zero, modifierFlags: .option, timestamp: 0,
                    windowNumber: panel.windowNumber, context: nil, characters: "\u{F703}",
                    charactersIgnoringModifiers: "\u{F703}", isARepeat: false, keyCode: 124)))
        #expect(model.index == 0)
    }

    // Reduce Motion off: it fades in over 0.2 s and the media grows from 0.9.
    @Test
    func itAppearsWithDiscordsMotion() throws {
        viewer.reducesMotion = { false }
        _ = try open([item(1)])
        let panel = try #require(viewer.panel)
        #expect(try view().stage.tile.layer?.animation(forKey: "open") != nil)
        #expect(panel.alphaValue < 1, "fading in")
        viewer.close()
        #expect(viewer.panel == nil)
    }

    // One authority for the viewer's pointer (cursors.md): zoom over zoomable media, the hand over
    // what clicks, the arrow on the scrim and the top bar's empty space.
    @Test
    func theCursorFollowsWhatsUnderThePointer() throws {
        _ = try open([item(1), item(2)])
        let view = try view()
        let media = view.stage.mediaFrame
        let middle = { (inner: NSView) in inner.convert(NSPoint(x: inner.bounds.midX, y: inner.bounds.midY), to: view) }

        #expect(view.cursor(at: NSPoint(x: media.midX, y: media.midY)) === ViewerStage.zoomIn)
        #expect(view.cursor(at: NSPoint(x: 5, y: view.bounds.height - 5)) === NSCursor.arrow)
        #expect(view.cursor(at: NSPoint(x: 400, y: ViewerLayout.marginY + 20)) === NSCursor.arrow)
        for button in [view.close, view.next, view.strip.items[1], try #require(view.topBar.buttons.first)] as [NSView] {
            #expect(view.cursor(at: middle(button)) === NSCursor.pointingHand, "\(button)")
        }

        try click(at: NSPoint(x: media.midX, y: media.midY))
        #expect(view.cursor(at: NSPoint(x: media.midX, y: media.midY)) === ViewerStage.zoomOut)
    }

    // Module 947056's strip as drawn: the others under black at 50 %, every square's corners 2,
    // the strip's ends 8.
    @Test
    func theStripDimsTheOthers() async throws {
        let images = TestImages()
        let grey = try images.png("grey", pixels: 80, color: [200, 200, 200, 255])
        _ = try open([item(1), item(2), item(3)], at: 1, images: ImageCache(source: FakeImageSource.serving(grey, pixels: 80)))
        let strip = try view().strip
        try await eventually { strip.items.allSatisfy { $0.thumbnail.hasImage } }
        strip.layoutSubtreeIfNeeded()
        strip.displayIfNeeded()

        let width = Int(strip.bounds.width)
        let height = Int(strip.bounds.height)
        var pixels = [UInt8](repeating: 0, count: width * height * 4)
        let context = try #require(
            CGContext(
                data: &pixels, width: width, height: height, bitsPerComponent: 8, bytesPerRow: width * 4,
                space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue))
        try #require(strip.layer).render(in: context)
        let red = strip.items.map { item -> Int in
            let middle = item.convert(NSPoint(x: item.bounds.midX, y: item.bounds.midY), to: strip)
            return Int(pixels[(height / 2 * width + Int(middle.x)) * 4])
        }
        #expect(abs(red[1] - 200) <= 6, "\(red)")
        #expect(abs(red[0] - 100) <= 6 && abs(red[2] - 100) <= 6, "\(red)")
        #expect(strip.items.allSatisfy { $0.layer?.cornerRadius == 2 })
        #expect(strip.ends.layer?.cornerRadius == ViewerLayout.stripEnd)
        #expect(strip.ends.layer?.masksToBounds == true)
        #expect(strip.items.allSatisfy { $0.superview === strip.ends })
    }

    // Media without its size takes the whole box and is asked at its pixels, zoomed too.
    @Test
    func mediaWithoutItsSizeFillsTheBox() async throws {
        _ = try open([item(1, width: 0, height: 0)])
        let view = try view()
        let box = ViewerLayout.fit(NSSize(width: 4096, height: 4096), in: view.bounds.size, several: false)
        #expect(view.stage.mediaFrame.size == box)

        try click(at: NSPoint(x: view.stage.mediaFrame.midX, y: view.stage.mediaFrame.midY))
        let zoomed = UInt32(box.width * ViewerLayout.zoom * window.backingScaleFactor)
        try await eventually { self.mediaRequests().contains { $0.width >= min(zoomed, 4096) - 1 } }
    }

    // A zoomed ask that fails keeps the image shown, and a click still zooms out.
    @Test
    func aFailedZoomedAskKeepsTheImage() async throws {
        let images = TestImages()
        let photo = try images.png("photo", width: 400, height: 300)
        // Above the fitted ask (832 points wide), below the zoomed one, on either scale.
        let fitted: CGFloat = 2000
        let failing = FakeImageSource { request in
            guard CGFloat(request.width) <= fitted else {
                throw ImageError.Unavailable
            }
            return ImageFile(path: photo.path, width: 400, height: 300, animated: false, fromDisk: false)
        }
        _ = try open([item(1)], images: ImageCache(source: failing))
        let view = try view()
        let picture = view.stage.tile.picture
        try await eventually { picture.hasImage }

        try click(at: NSPoint(x: view.stage.mediaFrame.midX, y: view.stage.mediaFrame.midY))
        try await eventually { failing.requests.withLock { $0.contains { CGFloat($0.width) > fitted } } }
        try await Task.sleep(for: .milliseconds(100))
        #expect(picture.hasImage)
        #expect(!picture.failed)

        try click(at: NSPoint(x: view.stage.mediaFrame.midX, y: view.stage.mediaFrame.midY))
        #expect(!view.stage.zoomed)
    }

    // A zoomed image smaller than the window leaves scrim around it: a click there closes.
    @Test
    func theScrimClosesWhileZoomed() throws {
        _ = try open([item(1, width: 300, height: 200)])
        let view = try view()
        try click(at: NSPoint(x: view.stage.mediaFrame.midX, y: view.stage.mediaFrame.midY))
        #expect(view.stage.zoomed)
        #expect(view.stage.mediaFrame.width < view.bounds.width)

        try click(at: NSPoint(x: 5, y: view.bounds.height - 5))
        #expect(viewer.panel == nil)
    }

    // The pointer's cursor follows what's under it after a key moves to another item too.
    @Test
    func theCursorFollowsTheKeys() throws {
        _ = try open([item(1), item(2, "clip.mp4", kind: .video)])
        let view = try view()
        let middle = NSPoint(x: view.stage.mediaFrame.midX, y: view.stage.mediaFrame.midY)
        var set: [NSCursor] = []
        view.pointer = { view.convert(middle, to: nil) }
        view.setCursor = { set.append($0) }

        try key(124, "\u{F703}")
        #expect(set.last === NSCursor.arrow, "a video doesn't zoom")
        try key(124, "\u{F703}")
        #expect(set.last === ViewerStage.zoomIn)
    }

    // A move to another display asks at its scale.
    @Test
    func aDisplayChangeAsksAgain() throws {
        _ = try open([item(1)])
        let stage = try view().stage
        stage.tile.picture.maxPixels = nil
        stage.viewDidChangeBackingProperties()
        stage.layoutSubtreeIfNeeded()
        #expect(stage.tile.picture.maxPixels != nil)
    }

    // A mouse wheel's deltas are lines: each pans a line's height.
    @Test
    func aMouseWheelPansByLines() throws {
        _ = try open([item(1)])
        let view = try view()
        try click(at: NSPoint(x: view.stage.mediaFrame.midX, y: view.stage.mediaFrame.midY))
        let start = view.stage.offset
        let wheel = try #require(
            CGEvent(scrollWheelEvent2Source: nil, units: .line, wheelCount: 1, wheel1: -2, wheel2: 0, wheel3: 0))
        let event = try #require(NSEvent(cgEvent: wheel))
        #expect(!event.hasPreciseScrollingDeltas)

        view.stage.scrollWheel(with: event)
        #expect(view.stage.offset.y == start.y + event.scrollingDeltaY * ViewerStage.lineScroll)
    }

    // The viewer takes the keys it doesn't use, so they don't beep; a button that leaves under the
    // pointer forgets its hover.
    @Test
    func keysStayAndButtonsForgetTheirHover() throws {
        _ = try open([item(1), item(2, "clip.mp4", kind: .video)])
        let view = try view()
        #expect(viewer.panel?.firstResponder === view)
        let zoom = try #require(view.topBar.buttons.first)
        zoom.mouseEntered(with: try #require(NSEvent.enterExitEvent(
            with: .mouseEntered, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: 0,
            context: nil, eventNumber: 0, trackingNumber: 0, userData: nil)))
        #expect(zoom.isHovered)

        try key(124, "\u{F703}")
        #expect(zoom.superview == nil)
        #expect(!zoom.isHovered)
    }

    // Zoomed before the image ever came, a failure shows; a click there still zooms out first.
    @Test
    func aClickOnZoomedFailedMediaZoomsOut() async throws {
        let gate = Gate()
        let failing = FakeImageSource { _ in
            await gate.wait()
            throw ImageError.Unavailable
        }
        _ = try open([item(1)], images: ImageCache(source: failing))
        let view = try view()
        let middle = NSPoint(x: view.stage.mediaFrame.midX, y: view.stage.mediaFrame.midY)
        try click(at: middle)
        #expect(view.stage.zoomed)

        gate.fire()
        try await eventually { view.stage.tile.picture.failed }
        try click(at: NSPoint(x: view.stage.mediaFrame.midX, y: view.stage.mediaFrame.midY))
        #expect(!view.stage.zoomed)
    }

    // The author block takes its clicks (and the time its tooltip); they close the viewer.
    @Test
    func theAuthorBlockIsHit() throws {
        _ = try open([item(1)])
        let view = try view()
        let author = view.topBar.author
        let name = author.convert(NSPoint(x: author.name.frame.midX, y: author.name.frame.midY), to: view)
        #expect(view.hitTest(view.convert(name, to: view.superview)) === author)
        #expect(view.hitTest(view.convert(NSPoint(x: 400, y: ViewerLayout.marginY + 20), to: view.superview)) === view.stage)
    }
}
