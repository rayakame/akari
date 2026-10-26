import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// Discord's form (canary 649224, module 595840 and its CSS) in a 480 × 700 split: the header's 48
// and its line, the fields at the bottom of the scroller 16 inside, the composer 8 above the
// bottom.
@MainActor
struct ThreadFormViewTests {
    static let size = NSSize(width: 480, height: 700)
    // From the bottom: the composer 56 + 8; the wrapper's 16; a private field of label 20, 8 and
    // the 20-point checkbox; 24; the name's label 20, 8 and its 40-point box.
    static let composerTop: CGFloat = 700 - 8 - 56
    static let checkboxBottom: CGFloat = composerTop - 16
    static let boxBottom: CGFloat = checkboxBottom - 20 - 8 - 20 - 24

    func session(
        creation: ThreadCreation = ThreadCreation(public: true, private: true),
        slowmode: Slowmode? = nil
    ) -> SessionModel {
        let fakes = FakeBrowserStore()
        fakes.lines = true
        fakes.creation = creation
        fakes.threadSlowmode = slowmode
        let session = fakes.session()
        session.open(channel: FakeBrowserStore.parent)
        return session
    }

    func rendered(
        _ session: SessionModel, appearance: NSAppearance.Name = .darkAqua
    ) throws -> Rendered {
        let form = try #require(session.threadForm)
        return try Rendered(
            ThreadFormView(form: form, session: session)
                .frame(width: Self.size.width, height: Self.size.height)
                .background(Color(nsColor: Palette.chat))
                .ignoresSafeArea(),
            appearance: appearance, size: Self.size)
    }

    // A token drawn over others, as the form draws it.
    func swatch(_ layers: [NSColor], appearance: NSAppearance.Name = .darkAqua) throws -> NSColor? {
        try Rendered(
            ZStack {
                ForEach(layers.indices, id: \.self) { Color(nsColor: layers[$0]) }
            }
            .ignoresSafeArea(), appearance: appearance, size: NSSize(width: 10, height: 10)
        ).color(5, 5)
    }

    func near(_ a: NSColor?, _ b: NSColor?, _ tolerance: CGFloat = 0.02) -> Bool {
        guard let a, let b else {
            return false
        }
        return abs(a.redComponent - b.redComponent) <= tolerance
            && abs(a.greenComponent - b.greenComponent) <= tolerance
            && abs(a.blueComponent - b.blueComponent) <= tolerance
    }

    func rows(
        _ image: Rendered, x: CGFloat, matching color: NSColor?, in ys: ClosedRange<CGFloat>
    ) -> [CGFloat] {
        stride(from: ys.lowerBound, through: ys.upperBound, by: 0.5).filter {
            near(image.color(x, $0), color)
        }
    }

    func columns(
        _ image: Rendered, y: CGFloat, matching color: NSColor?, in xs: ClosedRange<CGFloat>
    ) -> [CGFloat] {
        stride(from: xs.lowerBound, through: xs.upperBound, by: 0.5).filter {
            near(image.color($0, y), color)
        }
    }

    // Anything but the background between `xs` in rows `ys`.
    func ink(_ image: Rendered, xs: ClosedRange<CGFloat>, ys: ClosedRange<CGFloat>) -> [CGFloat] {
        let background = image.color(xs.lowerBound, ys.lowerBound)
        return stride(from: ys.lowerBound, through: ys.upperBound, by: 0.5).filter { y in
            stride(from: xs.lowerBound, through: xs.upperBound, by: 1).contains {
                !near(image.color($0, y), background, 0.06)
            }
        }
    }

    @Test
    func theHeaderSaysNewThreadOverItsLine() throws {
        let session = session()
        session.openThreadForm(in: FakeBrowserStore.parent, location: .threadBrowserToolbar)
        let image = try rendered(session)

        let line = try swatch([Palette.chat, Palette.borderSubtle])
        #expect(near(image.color(240, 48.25), line))
        #expect(!near(image.color(240, 47.25), line))
        let title = ink(image, xs: 44...200, ys: 14...34)
        #expect(!title.isEmpty, "the title")
    }

    @Test
    func theNameFieldIsDiscords40PointBoxInItsPlace() throws {
        let session = session()
        session.openThreadForm(in: FakeBrowserStore.parent, location: .threadBrowserToolbar)
        let image = try rendered(session)

        let border = try swatch([Palette.chat, Palette.inputBackground, Palette.borderNormal])
        let found = rows(image, x: 240, matching: border, in: 300...640)
        let top = try #require(found.first)
        let bottom = try #require(found.last)
        #expect(abs(bottom + 0.5 - top - 40) <= 0.5, "\(top)…\(bottom)")
        #expect(abs(bottom + 0.5 - Self.boxBottom) <= 2, "\(bottom)")
        let fill = try swatch([Palette.chat, Palette.inputBackground])
        #expect(near(image.color(240, top + 20), fill))
        // Its left edge is the wrapper's 16 in, rounded by 8.
        let edge = columns(image, y: top + 20, matching: border, in: 10...30)
        #expect(edge.first.map { abs($0 - 16) <= 0.5 } == true, "\(edge)")
        #expect(!near(image.color(17, top + 0.25), border), "a rounded corner")
    }

    @Test
    func theCircleIs64Points16AboveTheName() throws {
        let session = session()
        session.openThreadForm(in: FakeBrowserStore.parent, location: .threadBrowserToolbar)
        let image = try rendered(session)

        let fill = try swatch([Palette.chat, Palette.backgroundModStrong])
        let border = try swatch([Palette.chat, Palette.inputBackground, Palette.borderNormal])
        let boxTop = try #require(rows(image, x: 240, matching: border, in: 300...640).first)
        // The label's 20 and 8 lie between the circle's 16 and the box, whose border has the
        // circle's gray.
        let circle = rows(image, x: 48, matching: fill, in: 300...(boxTop - 28))
        let top = try #require(circle.first)
        let bottom = try #require(circle.last)
        #expect(abs(bottom + 0.5 - top - 64) <= 1.5, "\(top)…\(bottom)")
        #expect(abs(boxTop - bottom - 0.5 - 16 - 28) <= 1.5, "\(bottom) \(boxTop)")
        let across = columns(image, y: (top + bottom) / 2 - 20, matching: fill, in: 0...120)
        #expect(across.first.map { $0 >= 16 && $0 <= 40 } == true)
    }

    @Test
    func theCheckboxIs20PointsAndBrandOnceChecked() throws {
        let session = session()
        session.openThreadForm(in: FakeBrowserStore.parent, location: .threadBrowserToolbar)
        let unchecked = try rendered(session)
        session.threadForm?.isPrivate = true
        let checked = try rendered(session)

        let brand = try swatch([Palette.chat, Palette.brand])
        // Inside its 1-point border, past the corners' 4.
        let box = rows(checked, x: 21, matching: brand, in: 400...650)
        let top = try #require(box.first)
        let bottom = try #require(box.last)
        #expect(abs(bottom + 0.5 - top - (ThreadFormLayout.checkbox - 2)) <= 1, "\(top)…\(bottom)")
        #expect(rows(unchecked, x: 21, matching: brand, in: 400...650).isEmpty)
        // Checked, the hint shows under it and pushes the fields up by its 4 and 16.
        let hint = ink(checked, xs: 16...300, ys: (bottom + 1)...(bottom + 24))
        #expect(!hint.isEmpty, "the hint")
    }

    @Test
    func errorsMarkTheBoxAndTheComposerAndSayWhy() async throws {
        let session = session()
        session.openThreadForm(in: FakeBrowserStore.parent, location: .threadBrowserToolbar)
        await session.submitThreadForm()
        let image = try rendered(session)

        let red = try swatch([Palette.chat, Palette.inputErrorTint, Palette.textError])
        let box = rows(image, x: 240, matching: red, in: 300...640)
        let boxBottom = try #require(box.last)
        #expect(!box.isEmpty, "the name's box is red")
        let note = ink(image, xs: 16...300, ys: (boxBottom + 2)...(boxBottom + 22))
        #expect(!note.isEmpty, "Thread Name is required")
        let composerRed = try swatch([Palette.chat, Palette.composer, Palette.danger])
        let edge = rows(image, x: 240, matching: composerRed, in: 560...699)
        let composerTop = try #require(edge.first)
        #expect(abs(composerTop - Self.composerTop) <= 1, "\(composerTop)")
        // "Starter Message is required." sits above it, 16 in.
        let starter = ink(image, xs: 16...300, ys: (composerTop - 30)...(composerTop - 2))
        #expect(!starter.isEmpty)
    }

    @Test
    func aMessagesFormShowsTheMessageAndNoCheckbox() throws {
        let session = session()
        session.openThreadForm(
            in: FakeBrowserStore.parent, from: BrowserStore.lineIds[0], location: .message)
        let image = try rendered(session)

        let border = try swatch([Palette.chat, Palette.inputBackground, Palette.borderNormal])
        let boxBottom = try #require(rows(image, x: 240, matching: border, in: 200...640).last)
        // Ray's "hello" lies between the box and the composer, with its avatar at 16.
        let preview = ink(image, xs: 16...120, ys: (boxBottom + 20)...(Self.composerTop - 2))
        #expect(!preview.isEmpty)
        let checkbox = try swatch([Palette.chat, Palette.inputBackground, Palette.checkboxBorder])
        #expect(rows(image, x: 16.25, matching: checkbox, in: 200...640).isEmpty)
    }

    @Test
    func slowmodeShowsItsStripAboveTheComposer() throws {
        let session = session(
            slowmode: Slowmode(interval: 30, exempt: false, until: Date().addingTimeInterval(20)))
        session.openThreadForm(in: FakeBrowserStore.parent, location: .threadBrowserToolbar)
        let image = try rendered(session)

        let strip = ink(image, xs: 400...470, ys: (Self.composerTop - 24)...(Self.composerTop - 1))
        #expect(!strip.isEmpty, "the countdown on the right")
        #expect(ThreadFormStrip.text(Slowmode(interval: 30, exempt: true, until: nil)) == "Slowmode Immune")
        #expect(ThreadFormStrip.text(Slowmode(interval: 30, exempt: false, until: nil)) == "Slowmode is enabled.")
        #expect(
            ThreadFormStrip.help(Slowmode(interval: 30, exempt: false, until: nil))
                == "Slowmode is enabled. Members can send one message every 30 seconds")
    }

    @Test
    func theFormRendersInTheLightThemeToo() throws {
        let session = session()
        session.openThreadForm(in: FakeBrowserStore.parent, location: .threadBrowserToolbar)
        let image = try rendered(session, appearance: .aqua)

        let border = try swatch(
            [Palette.chat, Palette.inputBackground, Palette.borderNormal], appearance: .aqua)
        #expect(rows(image, x: 240, matching: border, in: 300...640).count >= 2)
    }
}

@MainActor
struct ThreadFormFieldTests {
    func hosted(_ session: SessionModel) throws -> (NSWindow, NSTextField) {
        let form = try #require(session.threadForm)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 480, height: 700),
            styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = NSHostingView(rootView: ThreadFormView(form: form, session: session))
        window.contentView?.layoutSubtreeIfNeeded()
        let field = try #require(Self.fields(in: window.contentView).first)
        return (window, field)
    }

    static func fields(in view: NSView?) -> [NSTextField] {
        guard let view else {
            return []
        }
        let own = (view as? NSTextField).map { [$0] }?.filter(\.isEditable) ?? []
        return own + view.subviews.flatMap(fields)
    }

    func session() -> SessionModel {
        let fakes = FakeBrowserStore()
        fakes.creation = ThreadCreation(public: true, private: true)
        let session = fakes.session()
        session.open(channel: FakeBrowserStore.parent)
        session.openThreadForm(in: FakeBrowserStore.parent, location: .threadBrowserToolbar)
        return session
    }

    @Test
    func typingNamesTheThreadWithoutLineBreaks() throws {
        let session = session()
        let (window, field) = try hosted(session)
        window.makeFirstResponder(field)

        field.currentEditor()?.insertText("two\nlines")

        #expect(session.threadForm?.name == "two lines")
        window.close()
    }

    @Test
    func escapeInTheNameClosesTheForm() throws {
        let session = session()
        let (window, field) = try hosted(session)
        window.makeFirstResponder(field)
        session.threadForm?.name = "kept"
        let escape = try #require(escapeKey())
        (field as? NameTextField)?.currentEvent = { escape }

        (field.currentEditor() as? NSTextView)?.doCommand(by: #selector(NSResponder.cancelOperation(_:)))

        #expect(session.threadForm == nil)
        session.openThreadForm(in: FakeBrowserStore.parent, location: .threadBrowserToolbar)
        #expect(session.threadForm?.name == "kept")
        window.close()
    }

    @Test
    func returnInTheNameSubmits() async throws {
        let session = session()
        let (window, field) = try hosted(session)
        window.makeFirstResponder(field)

        (field.currentEditor() as? NSTextView)?.doCommand(by: #selector(NSResponder.insertNewline(_:)))
        for _ in 0..<100 where session.threadForm?.messageError == nil {
            try await Task.sleep(for: .milliseconds(5))
        }

        #expect(session.threadForm?.messageError == ThreadFormModel.messageRequired)
        window.close()
    }
}

@MainActor
struct MessageMenuThreadTests {
    let message = Message(
        id: MessageId(rawValue: 5), channelId: ChannelId(rawValue: 11), kind: .default,
        author: User(
            id: UserId(rawValue: 2), username: "ray", globalName: nil, displayName: "Ray",
            bot: false, system: false, avatar: .defaultAvatar(index: 2)),
        fromWebhook: false, content: "hello", timestamp: Date(), editedTimestamp: nil,
        pinned: false, mentionEveryone: false, attachments: [], embeds: [], stickers: [],
        componentsV2: false, delivery: .sent, formatted: formattedText(content: "hello"),
        mentionsMe: false, reference: nil, hasThread: false, mentions: [], call: nil,
        roleSubscription: nil, purchasedProduct: nil, actor: nil, applicationName: nil,
        embedFields: [])

    // The official order: Reply, Forward, Create Thread | Copy Text, …
    @Test
    func createThreadClosesItsGroupBeforeCopyText() {
        var ran = false
        let items = MessageMenu.items(for: message, createThread: { ran = true })

        #expect(items.map(\.title) == ["Create Thread", "", "Copy Text"])
        #expect(items[1].isSeparatorItem)
        items[0].target?.perform(items[0].action, with: items[0])
        #expect(ran)
    }

    @Test
    func withoutItTheMenuIsAsBefore() {
        #expect(MessageMenu.items(for: message).map(\.title) == ["Copy Text"])
        var empty = message
        empty.content = ""
        #expect(MessageMenu.items(for: empty, createThread: {}).map(\.title) == ["Create Thread"])
    }
}

@MainActor
struct ThreadBrowserCreateTests {
    func rendered(creation: ThreadCreation, empty: Bool = false) throws -> (Rendered, SessionModel) {
        let fakes = FakeBrowserStore()
        fakes.creation = creation
        fakes.empty = empty
        let session = fakes.session()
        session.openThreadBrowser(of: FakeBrowserStore.parent, as: .popout)
        let browser = try #require(session.threadBrowser)
        let size = NSSize(width: 480, height: 651)
        let image = try Rendered(
            ThreadBrowserView(browser: browser, session: session, now: FakeBrowserStore.now)
                .frame(width: size.width, height: size.height)
                .background(Color(nsColor: Palette.surfaceHigh))
                .ignoresSafeArea(),
            size: size)
        return (image, session)
    }

    func brandRows(_ image: Rendered, x: CGFloat, ys: ClosedRange<CGFloat>) -> [CGFloat] {
        stride(from: ys.lowerBound, through: ys.upperBound, by: 0.5).filter { y in
            !brand(image, xs: x...x, ys: y...y).isEmpty
        }
    }

    func brand(_ image: Rendered, xs: ClosedRange<CGFloat>, ys: ClosedRange<CGFloat>) -> [CGFloat] {
        let brand = try? Rendered(
            Color(nsColor: Palette.brand).ignoresSafeArea(), size: NSSize(width: 10, height: 10)
        ).color(5, 5)
        return stride(from: xs.lowerBound, through: xs.upperBound, by: 0.5).filter { x in
            stride(from: ys.lowerBound, through: ys.upperBound, by: 0.5).contains { y in
                guard let color = image.color(x, y), let brand else {
                    return false
                }
                return abs(color.redComponent - brand.redComponent) < 0.06
                    && abs(color.blueComponent - brand.blueComponent) < 0.06
                    && abs(color.greenComponent - brand.greenComponent) < 0.06
            }
        }
    }

    @Test
    func theHeadersCreateButtonEnds16FromTheRight() throws {
        let (image, _) = try rendered(creation: ThreadCreation(public: true, private: false))

        let button = brand(image, xs: 300...479, ys: 20...28)
        let right = try #require(button.last)
        let left = try #require(button.first)
        #expect(abs(right + 0.5 - (480 - 16)) <= 1, "\(left)…\(right)")
        #expect(right - left >= 59, "Discord's min width 60")
        let rows = brandRows(image, x: left + 10, ys: 0...48)
        // 32 with its faint border.
        #expect(abs((rows.last ?? 0) + 0.5 - (rows.first ?? 0) - 32) <= 2, "\(rows)")
    }

    @Test
    func withoutEitherPermissionThereIsNoCreate() throws {
        let (image, _) = try rendered(creation: ThreadCreation(public: false, private: false))

        #expect(brand(image, xs: 300...479, ys: 8...40).isEmpty)
    }

    @Test
    func theEmptyStateOffersCreateThread() throws {
        let (image, _) = try rendered(creation: ThreadCreation(public: false, private: true), empty: true)

        #expect(!brand(image, xs: 140...340, ys: 300...620).isEmpty)
    }

    @Test
    func withCreateTheSearchFieldIsDiscords236() {
        // 480: the paddings, the icon and title, the divider and the button leave 257.
        #expect(ThreadBrowserLayout.searchWidth(free: 257) == 236)
        #expect(ThreadBrowserLayout.createRoom >= 68)
    }
}

@MainActor
struct ThreadFormSlotTests {
    // The channel's red message area keeps the page less the split's 480 and its 8 gap.
    @Test
    func theFormTakesTheSplitViewsCard() throws {
        let fakes = FakeBrowserStore()
        fakes.creation = ThreadCreation(public: true, private: false)
        let session = fakes.session()
        session.open(channel: FakeBrowserStore.parent)
        let size = NSSize(width: 1400, height: 800)
        let without = try Rendered(SessionView(session: session) { _, _ in Color.red }, size: size)
        session.openThreadForm(in: FakeBrowserStore.parent, location: .keyboardShortcut)
        let with = try Rendered(SessionView(session: session) { _, _ in Color.red }, size: size)

        func red(_ image: Rendered, _ x: CGFloat) -> Bool {
            guard let color = image.color(x, 400) else {
                return false
            }
            return color.redComponent > 0.8 && color.greenComponent < 0.5 && color.blueComponent < 0.5
        }
        #expect(session.messages != nil)
        #expect(red(without, 1300), "\(String(describing: without.color(1300, 400)))")
        #expect(!red(with, 1300))
        #expect(
            red(with, 1400 - SplitWidth.standard - SplitWidth.gap - 4),
            "\(String(describing: with.color(1400 - SplitWidth.standard - SplitWidth.gap - 4, 400)))")
        #expect(!red(with, 1400 - SplitWidth.standard + 4))
    }
}

@MainActor
struct ThreadFormFocusTests {
    // The form's field is made before it has a window; joining one still gives it the focus.
    @Test
    func theNameFieldTakesTheFocusOnceInItsWindow() async throws {
        let fakes = FakeBrowserStore()
        fakes.creation = ThreadCreation(public: true, private: true)
        let session = fakes.session()
        session.open(channel: FakeBrowserStore.parent)
        session.openThreadForm(in: FakeBrowserStore.parent, location: .threadBrowserToolbar)
        let form = try #require(session.threadForm)
        let host = NSHostingView(rootView: ThreadFormView(form: form, session: session))
        host.frame = NSRect(x: 0, y: 0, width: 480, height: 700)
        host.layoutSubtreeIfNeeded()
        let window = NSWindow(
            contentRect: host.frame, styleMask: [.titled], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false

        window.contentView = host
        for _ in 0..<100 where session.focusThreadForm {
            try await Task.sleep(for: .milliseconds(5))
        }

        let field = try #require(ThreadFormFieldTests.fields(in: host).first)
        #expect(!session.focusThreadForm)
        #expect((window.firstResponder as? NSTextView)?.delegate === field)
        window.close()
    }
}

// The form in the session's window, beside a channel's real table: its keys and cursors.
@MainActor
struct ThreadFormKeysTests {
    let screen = SessionScreenTests()

    func shown(width: CGFloat = 1100) async throws -> (SessionModel, FormattedAccount, NSHostingView<SessionScreen>, NSWindow) {
        let account = FormattedAccount(contents: ["test", "short"])
        let session = screen.formattedSession(account)
        session.openThreadForm(in: FormattedStore.channelIds[0], location: .keyboardShortcut)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: width, height: 600), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = host
        window.orderFrontRegardless()
        try await screen.waitUntil(host) {
            !screen.views(MessageTableView.self, in: host).isEmpty
                && !ThreadFormFieldTests.fields(in: host).isEmpty
                && screen.views(ComposerTextView.self, in: host).count == 2
        }
        return (session, account, host, window)
    }

    func formComposer(_ host: NSView) throws -> ComposerTextView {
        // The form's is the right one.
        try #require(
            screen.views(ComposerTextView.self, in: host).max {
                $0.convert($0.bounds, to: nil).minX < $1.convert($1.bounds, to: nil).minX
            })
    }

    func nameField(_ host: NSView) throws -> NameTextField {
        try #require(ThreadFormFieldTests.fields(in: host).first as? NameTextField)
    }

    @Test(arguments: [false, true])
    func escapeClosesTheFormFromEitherField(inName: Bool) async throws {
        let (session, _, host, window) = try await shown()
        defer { window.close() }
        let escape = try #require(escapeKey())
        if inName {
            let field = try nameField(host)
            window.makeFirstResponder(field)
            field.currentEvent = { escape }
            (field.currentEditor() as? NSTextView)?.doCommand(by: #selector(NSResponder.cancelOperation(_:)))
        } else {
            let composer = try formComposer(host)
            composer.currentEvent = { escape }
            composer.doCommand(by: #selector(NSResponder.cancelOperation(_:)))
        }

        #expect(session.threadForm == nil)
    }

    @Test(arguments: [false, true])
    func shiftEscapeMarksTheServerReadAndKeepsTheForm(inName: Bool) async throws {
        let (session, account, host, window) = try await shown()
        defer { window.close() }
        let shiftEscape = try #require(escapeKey(.shift))
        if inName {
            let field = try nameField(host)
            window.makeFirstResponder(field)
            field.currentEvent = { shiftEscape }
            (field.currentEditor() as? NSTextView)?.doCommand(by: #selector(NSResponder.cancelOperation(_:)))
        } else {
            let composer = try formComposer(host)
            composer.currentEvent = { shiftEscape }
            composer.doCommand(by: #selector(NSResponder.cancelOperation(_:)))
        }

        #expect(session.threadForm != nil)
        #expect(account.markedGuildsRead.withLock { $0 } == [[GuildId(rawValue: 1)]])
    }

    @Test(arguments: [false, true])
    func commandPeriodDoesNothing(inName: Bool) async throws {
        let (session, account, host, window) = try await shown()
        defer { window.close() }
        let period = try #require(periodKey(.command))
        if inName {
            let field = try nameField(host)
            window.makeFirstResponder(field)
            field.currentEvent = { period }
            (field.currentEditor() as? NSTextView)?.doCommand(by: #selector(NSResponder.cancelOperation(_:)))
        } else {
            let composer = try formComposer(host)
            composer.currentEvent = { period }
            composer.doCommand(by: #selector(NSResponder.cancelOperation(_:)))
        }

        #expect(session.threadForm != nil)
        #expect(account.markedGuildsRead.withLock { $0 }.isEmpty)
    }

    @Test
    func returnDuringCompositionInTheNameCommitsAndCreatesNothing() async throws {
        let (session, _, host, window) = try await shown()
        defer { window.close() }
        let field = try nameField(host)
        window.makeFirstResponder(field)
        let editor = try #require(field.currentEditor() as? NSTextView)
        editor.setMarkedText("か", selectedRange: NSRange(location: 1, length: 0), replacementRange: NSRange(location: NSNotFound, length: 0))

        editor.doCommand(by: #selector(NSResponder.insertNewline(_:)))
        try await Task.sleep(for: .milliseconds(50))

        #expect(session.threadForm?.messageError == nil, "nothing was submitted")
    }

    @Test
    func theNameFieldIsNamedByItsLabel() async throws {
        let (_, _, host, window) = try await shown()
        defer { window.close() }

        #expect(try nameField(host).accessibilityLabel() == "Thread Name")
    }

    // At 1000 the page is too narrow for both: the form floats over the channel's table.
    @Test
    func overAFloatingFormTheListShowsTheIBeamOverItsFields() async throws {
        let (_, _, host, window) = try await shown(width: 1000)
        defer { window.close() }
        let table = try #require(screen.views(MessageTableView.self, in: host).first)
        let field = try nameField(host)
        let overField = field.convert(NSPoint(x: field.bounds.midX, y: field.bounds.midY), to: nil)
        let inField = field.convert(NSPoint(x: field.bounds.minX + 2, y: field.bounds.midY), to: nil)
        #expect(table.convert(table.visibleRect, to: nil).contains(overField), "it floats over the list")

        #expect(table.cursor(at: overField) == .iBeam)
        #expect(table.cursor(at: inField) == .iBeam)
        // Its card's empty space shows the arrow, not a message's I-beam under it.
        let header = field.convert(NSPoint(x: field.bounds.midX, y: -60), to: nil)
        if table.convert(table.visibleRect, to: nil).contains(header) {
            #expect(table.cursor(at: header) == .arrow)
        }
    }
}

@MainActor
struct ThreadFormPreviewTests {
    // Ray's long message from #new as the source, in a 500-point window less the title bar.
    func rendered(lines: Int, height: CGFloat = 468, emoji: Bool = false) throws -> (SessionModel, ThreadFormModel, NSHostingView<some View>, NSWindow) {
        let fakes = FakeBrowserStore()
        fakes.lines = true
        fakes.firstLineEmoji = emoji
        fakes.creation = ThreadCreation(public: true, private: true)
        fakes.longFirstLine = lines
        let session = fakes.session()
        session.open(channel: FakeBrowserStore.parent)
        session.openThreadForm(in: FakeBrowserStore.parent, from: BrowserStore.lineIds[0], location: .message)
        let form = try #require(session.threadForm)
        let host = NSHostingView(
            rootView: ThreadFormView(form: form, session: session).frame(width: 480, height: height))
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 480, height: height), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        return (session, form, host, window)
    }

    @Test
    func aTallMessageScrollsAndTheNameAndItsErrorStayVisible() async throws {
        let (session, form, host, window) = try rendered(lines: 40)
        defer { window.close() }
        await session.submitThreadForm()
        form.message = ""
        host.layoutSubtreeIfNeeded()
        let field = try #require(ThreadFormFieldTests.fields(in: host).first)
        let frame = field.convert(field.bounds, to: host)

        #expect(host.bounds.contains(frame), "\(frame) in \(host.bounds)")
        #expect(frame.minY > 49 && frame.maxY < host.bounds.height - 64)
    }

    @Test
    func theComposerGrowsToHalfTheFormAtMost() async throws {
        let (_, form, host, window) = try rendered(lines: 1)
        defer { window.close() }
        form.message = Array(repeating: "line", count: 60).joined(separator: "\n")
        for _ in 0..<40 {
            host.layoutSubtreeIfNeeded()
            try await Task.sleep(for: .milliseconds(10))
        }
        let composer = try #require(SessionScreenTests().views(ComposerTextView.self, in: host).first)
        let scroll = try #require(composer.enclosingScrollView)

        #expect(scroll.frame.height > 100)
        #expect(scroll.frame.height <= host.bounds.height / 2 + 1, "\(scroll.frame.height)")
    }

    // The preview's emoji and stickers come from the session's cache, like the list's.
    @Test
    func thePreviewShowsEmojiFromTheCache() throws {
        let (session, form, _, window) = try rendered(lines: 1, emoji: true)
        defer { window.close() }
        let cache = ImageCache(source: FakeImageSource.stalling())
        let host = NSHostingView(
            rootView: ThreadFormView(form: form, session: session).frame(width: 480, height: 468)
                .environment(\.imageCache, cache))
        let own = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 480, height: 468), styleMask: [.titled],
            backing: .buffered, defer: false)
        own.isReleasedWhenClosed = false
        defer { own.close() }
        own.contentView = host
        host.layoutSubtreeIfNeeded()

        let preview = try #require(SessionScreenTests().views(MessagePreviewView.self, in: host).first)
        #expect(form.source?.content.contains("<:") == true, "the fixture's source carries a custom emoji")
        let layers = SessionScreenTests().views(ImageLayerView.self, in: preview).filter {
            if case .emoji = $0.content?.key { true } else { false }
        }
        #expect(layers.count == 1)
        #expect(layers.first?.images === cache)
    }

    @Test
    func thePreviewsTextShowsTheIBeam() throws {
        let (_, _, host, window) = try rendered(lines: 1)
        defer { window.close() }
        let preview = try #require(SessionScreenTests().views(MessagePreviewView.self, in: host).first)
        let text = try #require(SessionScreenTests().views(MessageTextView.self, in: preview).first { !$0.string.isEmpty })
        let glyph = text.convert(NSPoint(x: 4, y: text.bounds.midY), to: nil)
        let past = text.convert(NSPoint(x: text.bounds.maxX - 2, y: text.bounds.midY), to: nil)

        #expect(preview.cursor(at: glyph) == .iBeam)
        #expect(preview.cursor(at: past) == .arrow)
    }
}
