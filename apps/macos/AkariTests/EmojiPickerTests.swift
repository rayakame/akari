import AkariKit
import AppKit
import Testing

@testable import Akari

// A child panel holding an AppKit view, as the emoji picker's.
@MainActor
struct ChildWindowsViewTests {
    @Test
    func aPanelHoldsAnAppKitViewOverItsShieldAndGoesWithTearDown() throws {
        let window = TestWindow.make(size: NSSize(width: 600, height: 400))
        let windows = ChildWindows(keyable: true)
        var keyed: [NSWindow] = []
        windows.focus = WindowFocus(makeKey: { keyed.append($0) })
        let view = NSView()
        let frame = NSRect(x: window.frame.minX + 20, y: window.frame.minY + 20, width: 200, height: 100)

        windows.show(view, at: frame, in: window, dimmed: false)

        let panel = try #require(windows.panel)
        #expect(panel.contentView === view)
        #expect(panel.frame == frame)
        #expect(windows.shield?.frame == window.frame)
        #expect(keyed == [panel])

        windows.show(view, at: frame.offsetBy(dx: 10, dy: 0), in: window, dimmed: false)
        #expect(windows.panel === panel)
        #expect(panel.frame.minX == frame.minX + 10)
        let other = NSView()
        windows.show(other, at: frame, in: window, dimmed: false)
        #expect(panel.contentView === other)
        #expect(keyed.count == 1, "the panel took the key once")

        windows.tearDown(refocus: false)
        #expect(windows.panel == nil)
        #expect(window.childWindows?.isEmpty != false)
        window.close()
    }
}

// A picker on message 2 in an out-of-sight window: eleven faces used lately, Home's three emoji
// (one animated, one unavailable), Other's two (one needing Nitro), then Unicode's groups.
@MainActor
final class PickerHarness {
    static let faces = ["😀", "😃", "😄", "😁", "😆", "😅", "🤣", "😂", "🙂", "🙃", "😉"]

    let account = ReactionsAccount([], people: [:])
    let reactions: ReactionsModel
    let model: EmojiPickerModel
    let view: EmojiPickerView
    let window: NSWindow
    /// Another window a test keeps alive with the picker.
    var extra: NSWindow?

    init(found: [String: [PickerEmoji]] = [:], icons: [GuildId: ImageKey] = [:]) throws {
        let faces = Self.faces.map(pickerEmoji)
        account.reactionsStore.picker.withLock { picker in
            picker.recent = faces
            picker.servers = [
                EmojiSection(
                    guildId: GuildId(rawValue: 1),
                    emojis: [
                        pickerCustom(7, "wave"), pickerCustom(8, "cat", animated: true),
                        pickerCustom(9, "gone", access: .unavailable),
                    ]),
                EmojiSection(
                    guildId: GuildId(rawValue: 2),
                    emojis: [
                        pickerCustom(20, "blob", in: 2, access: .nitroLocked), pickerCustom(21, "ok", in: 2),
                    ]),
            ]
            picker.names = [GuildId(rawValue: 1): "Home", GuildId(rawValue: 2): "Other"]
            picker.found = found
            picker.icons = icons
        }
        reactions = ReactionsModel(store: account.reactionsStore, account: account)
        model = try #require(
            reactions.openPicker(
                for: row(2, at: berlinTime(2026, 10, 10, 12, 0)), from: .picker))
        view = EmojiPickerView(model: model)
        window = TestWindow.make(size: EmojiPickerView.size)
        window.contentView = view
        window.layoutIfNeeded()
        view.layoutSubtreeIfNeeded()
    }

    var grid: EmojiGridView { view.grid }
    var clip: NSClipView { view.scrollView.contentView }

    func at(_ section: Int, _ item: Int) -> EmojiPickerModel.Position {
        EmojiPickerModel.Position(section: section, item: item)
    }

    func middle(of position: EmojiPickerModel.Position) -> NSPoint {
        let rect = grid.layout.rect(of: position)
        return NSPoint(x: rect.midX, y: rect.midY)
    }

    /// A key's command reaching the search field.
    @discardableResult
    func command(_ selector: Selector) -> Bool {
        view.field.control(view.field.text, textView: NSTextView(), doCommandBy: selector)
    }

    func type(_ text: String) {
        view.field.text.stringValue = text
        view.field.controlTextDidChange(
            Notification(name: NSControl.textDidChangeNotification, object: view.field.text))
    }

    var made: [String] { account.made.withLock { $0 } }
}

// The picker's panel: its parts, the grid, the keys through the field, picks and search.
@MainActor
struct EmojiPickerViewTests {
    let picker: PickerHarness

    init() throws {
        picker = try PickerHarness()
    }

    @Test
    func theSearchFieldIsOnTopTheStripOnTheLeftTheGridAndItsFooterBeside() {
        let view = picker.view

        #expect(view.frame.size == NSSize(width: 432, height: 440))
        #expect(view.field.frame == NSRect(x: 12, y: 10, width: 408, height: 32))
        #expect(view.strip.frame == NSRect(x: 0, y: 52, width: 48, height: 388))
        #expect(view.scrollView.frame == NSRect(x: 48, y: 52, width: 384, height: 340))
        #expect(view.footerView.frame.minX == 48)
        #expect(view.footerView.frame.maxY == 440)
        #expect(view.footerView.frame.height == 47)
        #expect(view.field.text.placeholderAttributedString?.string == "Find the perfect emoji")
        #expect(view.accessibilityLabel() == "Reaction Picker")
    }

    @Test
    func theGridHasNineColumnsOfFortyUnderEachSectionsHeader() {
        let layout = picker.grid.layout

        #expect(layout.rect(of: picker.at(0, 0)) == NSRect(x: 12, y: 32, width: 40, height: 40))
        #expect(layout.rect(of: picker.at(0, 8)) == NSRect(x: 332, y: 32, width: 40, height: 40))
        #expect(layout.rect(of: picker.at(0, 9)) == NSRect(x: 12, y: 72, width: 40, height: 40))
        // Eleven faces make two rows; Home's header follows them.
        #expect(layout.sections[1].top == 112)
        #expect(layout.rect(of: picker.at(1, 0)).minY == 144)
        #expect(layout.position(at: NSPoint(x: 70, y: 50)) == picker.at(0, 1))
        #expect(layout.position(at: NSPoint(x: 70, y: 120)) == nil, "a header")
        #expect(layout.position(at: NSPoint(x: 140, y: 90)) == nil, "past the last face")
        #expect(layout.position(at: NSPoint(x: 8, y: 50)) == nil)
        #expect(picker.grid.headers[0]?.stringValue == "Frequently Used")
        #expect(picker.grid.headers[0]?.frame.minX == 16, "where the first emoji's box starts")
    }

    @Test
    func onlyTheCellsThatShowHaveViewsCustomEmojiFromTheImageTier() throws {
        let grid = picker.grid
        let shown = picker.clip.bounds.insetBy(dx: 0, dy: -40)

        #expect(!grid.cells.isEmpty)
        // Nine columns over the 340 that show and a row more on each side.
        #expect(grid.cells.count <= 9 * 11)
        #expect(grid.cells.allSatisfy { shown.intersects($0.value.frame) })
        let wave = try #require(grid.cells[picker.at(1, 0)])
        #expect(wave.picture.content?.key == .emoji(emojiId: EmojiId(rawValue: 7), animated: false))
        #expect(wave.glyph.isHidden)
        #expect(wave.picture.fallbackText == ":wave:")
        #expect(wave.picture.frame == NSRect(x: 4, y: 4, width: 32, height: 32))
        let face = try #require(grid.cells[picker.at(0, 0)])
        #expect(face.glyph.emoji == "😀")
        #expect(face.picture.isHidden)

        grid.scroll(to: 2000)

        #expect(grid.cells[picker.at(0, 0)] == nil, "scrolled away")
        #expect(grid.cells.count <= 9 * 11)
        #expect(grid.cells.values.allSatisfy { picker.clip.bounds.insetBy(dx: 0, dy: -40).intersects($0.frame) })
    }

    @Test
    func aLockedEmojiIsDimmedWithTheNotAllowedCursor() throws {
        let grid = picker.grid
        let gone = try #require(grid.cells[picker.at(1, 2)])

        #expect(gone.alphaValue == 0.35)
        #expect(grid.cells[picker.at(1, 0)]?.alphaValue == 1)
        #expect(grid.cursor(at: picker.middle(of: picker.at(1, 2))) == .operationNotAllowed)
        #expect(grid.cursor(at: picker.middle(of: picker.at(1, 0))) == .pointingHand)
        #expect(grid.cursor(at: NSPoint(x: 70, y: 120)) == .arrow)
    }

    @Test
    func theFirstEmojiIsLitAndTheFooterNamesIt() {
        let grid = picker.grid

        #expect(!grid.selectionView.isHidden)
        #expect(grid.selectionView.frame == grid.layout.rect(of: picker.at(0, 0)))
        #expect(picker.view.footerView.name.stringValue == ":grinning:")
        #expect(picker.view.footerView.detail.isHidden)
        #expect(picker.view.footerView.glyph.emoji == "😀")
    }

    @Test
    func thePointerSelectsWithoutScrollingAndTheFooterSaysTheServer() {
        let before = picker.clip.bounds.origin

        picker.grid.pointerMoved(to: picker.middle(of: picker.at(1, 0)))

        #expect(picker.model.selection == picker.at(1, 0))
        #expect(picker.grid.selectionView.frame == picker.grid.layout.rect(of: picker.at(1, 0)))
        #expect(picker.clip.bounds.origin == before)
        let footer = picker.view.footerView
        #expect(footer.name.stringValue == ":wave:")
        #expect(footer.detail.stringValue == "Home")
        #expect(!footer.picture.isHidden)
        #expect(footer.picture.content?.key == .emoji(emojiId: EmojiId(rawValue: 7), animated: false))

        picker.grid.pointerMoved(to: picker.middle(of: picker.at(1, 2)))
        #expect(footer.detail.stringValue == "Unavailable: its server lost the boosts it needs")
    }

    @Test
    func theArrowKeysInTheFieldMoveTheSelection() {
        #expect(picker.command(#selector(NSResponder.moveRight(_:))))
        #expect(picker.model.selection == picker.at(0, 1))
        picker.command(#selector(NSResponder.moveDown(_:)))
        #expect(picker.model.selection == picker.at(0, 10))
        picker.command(#selector(NSResponder.moveLeft(_:)))
        #expect(picker.model.selection == picker.at(0, 9))
        picker.command(#selector(NSResponder.moveUp(_:)))
        #expect(picker.model.selection == picker.at(0, 0))
        #expect(!picker.command(#selector(NSResponder.selectAll(_:))), "the field's own")
    }

    @Test
    func aKeyMoveScrollsTheSelectionIntoViewBelowTheStickyHeader() {
        for _ in 0..<12 {
            picker.command(#selector(NSResponder.moveDown(_:)))
        }
        let selected = picker.grid.layout.rect(of: picker.model.selection!)
        let shown = picker.clip.bounds
        #expect(picker.clip.bounds.minY > 0, "it scrolled")
        #expect(selected.maxY <= shown.maxY)

        for _ in 0..<6 {
            picker.command(#selector(NSResponder.moveUp(_:)))
        }
        let up = picker.grid.layout.rect(of: picker.model.selection!)
        #expect(up.minY >= picker.clip.bounds.minY + EmojiGridLayout.header - 0.5)
        #expect(up.maxY <= picker.clip.bounds.maxY)
    }

    @Test
    func enterPicksTheSelectionAndCloses() async throws {
        picker.command(#selector(NSResponder.moveRight(_:)))

        #expect(picker.command(#selector(NSResponder.insertNewline(_:))))

        #expect(!picker.model.isOpen)
        try await eventually { picker.made == [":smiley: from picker"] }
    }

    @Test
    func shiftEnterPicksAndKeepsItOpen() async throws {
        picker.view.modifiers = { .shift }

        picker.command(#selector(NSResponder.insertNewline(_:)))

        #expect(picker.model.isOpen)
        try await eventually { picker.made == [":grinning: from picker"] }
    }

    @Test
    func aClickPicksAndClosesAndShiftKeepsItOpen() async throws {
        picker.grid.click(at: picker.middle(of: picker.at(1, 0)), shift: true)
        #expect(picker.model.isOpen)
        try await eventually { picker.made == [":wave: from picker"] }

        picker.grid.click(at: picker.middle(of: picker.at(0, 2)), shift: false)
        #expect(!picker.model.isOpen)
        try await eventually { picker.made == [":wave: from picker", ":smile: from picker"] }
    }

    @Test
    func aClickOnALockedEmojiDoesNothing() async throws {
        picker.grid.click(at: picker.middle(of: picker.at(1, 2)), shift: false)
        picker.grid.click(at: NSPoint(x: 70, y: 120), shift: false)

        #expect(picker.model.isOpen)
        try await Task.sleep(for: .milliseconds(50))
        #expect(picker.made.isEmpty)
    }

    @Test
    func theStripHasASectionsButtonsWithALineBeforeUnicodesGroups() throws {
        let strip = picker.view.strip
        let buttons = strip.buttons

        #expect(buttons.count == picker.model.sections.count)
        #expect(buttons[0].image != nil, "a symbol")
        #expect(buttons[1].icon == nil)
        #expect(buttons[1].initials.stringValue == "H")
        #expect(buttons.map(\.toolTip).prefix(4) == ["Frequently Used", "Home", "Other", "Smileys & Emotion"])
        #expect(buttons[0].frame == NSRect(x: 8, y: 8, width: 32, height: 32))
        #expect(!strip.line.isHidden)
        #expect(strip.line.frame.minY > buttons[2].frame.maxY)
        #expect(strip.line.frame.maxY < buttons[3].frame.minY)
        #expect(strip.lit == 0)
        #expect(buttons[0].isLit)
    }

    @Test
    func aStripButtonShowsItsSectionsTopAndLightsUp() throws {
        let strip = picker.view.strip

        strip.buttons[4].performClick(nil)

        #expect(picker.model.selection == picker.at(4, 0))
        #expect(picker.clip.bounds.minY == picker.grid.layout.sections[4].top)
        #expect(strip.lit == 4)
        #expect(strip.buttons[4].isLit)
        #expect(!strip.buttons[0].isLit)
        #expect(picker.view.stickyTitle.stringValue == "People & Body")
    }

    @Test
    func theStickyHeaderNamesTheSectionAtTheTopAndTheNextPushesItUp() {
        let grid = picker.grid
        let next = grid.layout.sections[4].top

        grid.scroll(to: next - 50)
        #expect(picker.view.stickyTitle.stringValue == "Smileys & Emotion")
        #expect(picker.view.stickyBand(offset: 0))

        grid.scroll(to: next - 10)
        #expect(picker.view.stickyTitle.stringValue == "Smileys & Emotion")
        #expect(picker.view.stickyBand(offset: -22))
        #expect(picker.view.sticky.isFlipped)
        #expect(picker.view.convert(picker.view.stickyTitle.frame, from: picker.view.stickyTitle.superview).minY == 42, "12 into a band 22 above the grid's top")
        // What the header covers takes no pointer.
        #expect(grid.position(at: NSPoint(x: 70, y: grid.visibleRect.minY + 5)) == nil)
    }

    @Test
    func typingSearchesAndTheResultsHaveNoHeader() async throws {
        let found = try PickerHarness(found: ["o": [pickerEmoji("👌"), .custom(emoji: pickerCustom(21, "ok", in: 2))]])

        found.type("o")
        try await eventually { found.model.sections.first?.kind == .results }

        let grid = found.grid
        #expect(grid.headers.isEmpty)
        #expect(grid.layout.rect(of: found.at(0, 0)) == NSRect(x: 12, y: 12, width: 40, height: 40))
        #expect(found.view.sticky.isHidden)
        #expect(found.view.strip.lit == nil)
        #expect(!found.view.strip.buttons.isEmpty, "the strip stays")
        #expect(!found.view.field.clearButton.isHidden)
        #expect(found.view.empty.isHidden)
        #expect(found.view.footerView.name.stringValue == ":ok_hand:")
    }

    @Test
    func aSearchThatFindsNothingSaysSo() async throws {
        picker.type("zzz")
        try await eventually { picker.model.foundNothing }

        #expect(!picker.view.empty.isHidden)
        #expect(picker.view.empty.stringValue == "No emoji match your search")
        #expect(picker.view.footerView.name.stringValue.isEmpty)
        #expect(picker.grid.selectionView.isHidden)
    }

    @Test
    func aStripButtonEndsASearchAndEmptiesTheField() async throws {
        picker.type("zzz")
        try await eventually { picker.model.foundNothing }

        picker.view.strip.buttons[1].performClick(nil)

        #expect(picker.view.field.text.stringValue.isEmpty)
        #expect(picker.view.field.clearButton.isHidden)
        #expect(picker.model.selection == picker.at(1, 0))
        #expect(picker.view.empty.isHidden)
    }

    @Test
    func theClearButtonBrowsesAgain() async throws {
        picker.type("zzz")
        try await eventually { picker.model.foundNothing }

        picker.view.field.clearButton.performClick(nil)

        #expect(picker.model.query.isEmpty)
        #expect(picker.model.sections.first?.kind == .recent)
    }

    @Test
    func voiceOverReadsEachEmojiAsAButtonAndPressesIt() async throws {
        let grid = picker.grid
        let face = try #require(grid.cells[picker.at(0, 0)])
        let gone = try #require(grid.cells[picker.at(1, 2)])

        #expect(face.accessibilityRole() == .button)
        #expect(face.accessibilityLabel() == "grinning")
        #expect(gone.accessibilityLabel() == "gone")
        #expect(gone.accessibilityHelp() == "Unavailable: its server lost the boosts it needs")
        #expect(!gone.isAccessibilityEnabled())
        #expect((grid.accessibilityChildren()?.first as? PickerEmojiCell) === face)

        #expect(face.accessibilityPerformPress())
        try await eventually { picker.made == [":grinning: from picker"] }
    }

    @Test
    func anAnimatedEmojiPlaysOnlyInTheKeyWindowByTheSettingOrSelected() throws {
        let view = picker.view
        var key = false
        view.isKey = { key }
        view.motion = MotionPolicy(settings: displayDefaults(), reduceMotion: true)
        let cat = try #require(picker.grid.cells[picker.at(1, 1)])
        #expect(!cat.picture.animates)

        key = true
        view.updateAnimation()
        #expect(!cat.picture.animates, "reduced motion: only the selected one")

        picker.grid.pointerMoved(to: picker.middle(of: picker.at(1, 1)))
        #expect(cat.picture.animates)
        #expect(view.footerView.picture.animates)

        view.motion = MotionPolicy(settings: displayDefaults(), reduceMotion: false)
        picker.grid.pointerMoved(to: picker.middle(of: picker.at(1, 0)))
        #expect(cat.picture.animates, "the setting plays every animated one that shows")
    }
}

extension EmojiPickerView {
    func stickyBand(offset: CGFloat) -> Bool {
        sticky.subviews.first?.frame.minY == offset
    }
}

// Where the picker stands and how it goes.
@MainActor
struct EmojiPickerPresenterTests {
    let window = NSRect(x: 0, y: 0, width: 1200, height: 800)
    let size = EmojiPickerView.size

    @Test
    func leftOfItsOpenerWithTheTopsAligned() {
        let anchor = NSRect(x: 700, y: 600, width: 150, height: 34)

        let frame = EmojiPickerPresenter.frame(beside: anchor, on: .left, size: size, in: window)

        #expect(frame.maxX == 692)
        #expect(frame.maxY == 634)
        #expect(frame.size == size)
    }

    @Test
    func rightOfItsOpenerWithTheTopsAligned() {
        let anchor = NSRect(x: 300, y: 600, width: 34, height: 30)

        let frame = EmojiPickerPresenter.frame(beside: anchor, on: .right, size: size, in: window)

        #expect(frame.minX == 342)
        #expect(frame.maxY == 630)
    }

    @Test
    func theOtherSideWhereThereIsNoRoomAndAlwaysInsideTheWindow() {
        let left = EmojiPickerPresenter.frame(
            beside: NSRect(x: 200, y: 600, width: 100, height: 34), on: .left, size: size, in: window)
        #expect(left.minX == 308, "no room on the left: right of it")

        let right = EmojiPickerPresenter.frame(
            beside: NSRect(x: 1000, y: 600, width: 34, height: 30), on: .right, size: size, in: window)
        #expect(right.maxX == 992, "no room on the right: left of it")

        let low = EmojiPickerPresenter.frame(
            beside: NSRect(x: 700, y: 100, width: 150, height: 34), on: .left, size: size, in: window)
        #expect(low.minY == 8, "pushed up from the window's bottom")

        let narrow = NSRect(x: 0, y: 0, width: 600, height: 800)
        let squeezed = EmojiPickerPresenter.frame(
            beside: NSRect(x: 300, y: 600, width: 150, height: 34), on: .left, size: size, in: narrow)
        #expect(squeezed.minX >= 8 && squeezed.maxX <= 592)
    }

    @Test
    func thePanelIsAKeyDialogWithTheFieldFocusedAndEscapeClosesIt() throws {
        let picker = try PickerHarness()
        let window = TestWindow.make(size: NSSize(width: 1000, height: 700))
        let presenter = EmojiPickerPresenter()
        var keyed: [NSWindow] = []
        presenter.windows.focus = WindowFocus(makeKey: { keyed.append($0) })
        var closed = 0
        presenter.onClose = { closed += 1 }

        presenter.show(
            picker.model, beside: NSRect(x: 600, y: 500, width: 100, height: 34), on: .left, in: window,
            images: nil, motion: .default)

        let panel = try #require(presenter.windows.panel)
        let view = try #require(presenter.view)
        #expect(panel.contentView === view)
        #expect(panel.accessibilitySubrole() == .dialog)
        #expect(panel.accessibilityLabel() == "Reaction Picker")
        #expect(keyed == [panel])
        #expect((panel.firstResponder as? NSTextView)?.delegate === view.field.text)
        #expect(presenter.windows.shield != nil, "a press outside closes it")

        let escape = try #require(
            NSEvent.keyEvent(
                with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                windowNumber: panel.windowNumber, context: nil, characters: "\u{1b}",
                charactersIgnoringModifiers: "\u{1b}", isARepeat: false, keyCode: 53))
        panel.sendEvent(escape)

        #expect(presenter.windows.panel == nil)
        #expect(!picker.model.isOpen)
        #expect(closed == 1)
        window.close()
    }

    @Test
    func aPressOnTheShieldClosesIt() throws {
        let picker = try PickerHarness()
        let window = TestWindow.make(size: NSSize(width: 1000, height: 700))
        let presenter = EmojiPickerPresenter()
        presenter.windows.focus = WindowFocus(makeKey: { _ in })
        presenter.show(
            picker.model, beside: NSRect(x: 600, y: 500, width: 100, height: 34), on: .left, in: window,
            images: nil, motion: .default)
        let shield = try #require(presenter.windows.shield)

        shield.onPress()

        #expect(presenter.windows.panel == nil)
        #expect(!picker.model.isOpen)
        window.close()
    }

    @Test
    func aPickThatClosesTakesThePanelAway() async throws {
        let picker = try PickerHarness()
        let window = TestWindow.make(size: NSSize(width: 1000, height: 700))
        let presenter = EmojiPickerPresenter()
        presenter.windows.focus = WindowFocus(makeKey: { _ in })
        var closed = 0
        presenter.onClose = { closed += 1 }
        presenter.show(
            picker.model, beside: NSRect(x: 600, y: 500, width: 100, height: 34), on: .left, in: window,
            images: nil, motion: .default)
        let view = try #require(presenter.view)

        view.grid.click(at: NSPoint(x: 32, y: 52), shift: false)

        #expect(presenter.windows.panel == nil)
        #expect(closed == 1)
        try await eventually { picker.made == [":grinning: from picker"] }
        window.close()
    }
}

// The picker from the list: the bar's Add Reaction, a row's add button, the menu's "View More"
// and a post's React to Post.
@MainActor
struct EmojiPickerListTests {
    let list = BarList(width: 1000, height: 560)
    let account = ReactionsAccount([], people: [:])

    init() {
        list.actions.picking = ReactionsModel(store: account.reactionsStore, account: account)
        list.controller.emojiPicker.windows.focus = WindowFocus(makeKey: { _ in })
    }

    func show(_ second: ReactionRow? = nil) {
        list.actions.bars[2] = ([.addReaction, .more], [.copyLink, .addReaction, .more])
        list.show([
            row(1, by: author(2, "Ray"), at: list.noon, content: "first"),
            row(2, by: author(2, "Ray"), at: list.noon + 10, content: "second").reacting(second),
            row(3, by: author(2, "Ray"), at: list.noon + 20, content: "third"),
        ])
    }

    var presenter: EmojiPickerPresenter { list.controller.emojiPicker }

    func screen(_ view: NSView) -> NSRect {
        list.window.convertToScreen(view.convert(view.bounds, to: nil))
    }

    func escape() throws {
        let panel = try #require(presenter.windows.panel)
        panel.sendEvent(
            try #require(
                NSEvent.keyEvent(
                    with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                    windowNumber: panel.windowNumber, context: nil, characters: "\u{1b}",
                    charactersIgnoringModifiers: "\u{1b}", isARepeat: false, keyCode: 53)))
    }

    @Test
    func addReactionInTheBarOpensThePickerLeftOfTheBar() throws {
        show()
        try list.hover(2)
        let bar = screen(list.bar)

        try #require(list.bar.button(.addReaction)).performClick(nil)

        #expect(list.actions.calls.contains("picker for 2 from picker"))
        let panel = try #require(presenter.windows.panel)
        #expect(panel.frame.maxX == bar.minX - 8)
        // Its top at the bar's, unless that would take it below the window.
        #expect(panel.frame.maxY == bar.maxY || panel.frame.minY == list.window.frame.minY + 8)
        #expect(presenter.model?.location == .picker)
    }

    @Test
    func whileItsOpenItsRowStaysLitWithItsBarAndOtherRowsDontHover() throws {
        show()
        try list.hover(2)
        try #require(list.bar.button(.addReaction)).performClick(nil)

        try list.hover(3)

        #expect(list.hovered == [2])
        #expect(!list.bar.isHidden)
        #expect(list.bar.button(.addReaction)?.isSelected == true)
        let bar = try list.box(of: 2)
        #expect(list.bar.frame.maxX <= bar.maxX)

        try escape()

        #expect(presenter.windows.panel == nil)
        #expect(list.bar.button(.addReaction)?.isSelected != true)
        #expect(list.hovered == [3], "the pointer's row again")
    }

    @Test
    func theRowsAddButtonOpensThePickerRightOfItAndStaysLit() throws {
        // Room on both sides of the button.
        show(ReactionRow(pills: ["👍", "🎉", "🔥", "👀", "✅", "🚀", "😭"].map { pill($0) }, adds: true, toggles: true))
        let cell = try #require(try list.cell(of: 2) as? MessageCell)
        let add = try #require(cell.reactions?.add)
        cell.reactions?.layoutSubtreeIfNeeded()
        let button = screen(add)
        #expect(button.minX - 8 - 432 >= list.window.frame.minX + 8)
        try list.hover(1)

        add.performClick(nil)

        #expect(list.actions.calls.contains("picker for 2 from inlineButton"))
        let panel = try #require(presenter.windows.panel)
        #expect(panel.frame.minX == button.maxX + 8)
        // Its top at the button's, unless that would take it below the window.
        #expect(panel.frame.maxY == button.maxY || panel.frame.minY == list.window.frame.minY + 8)
        #expect(add.isSelected)
        #expect(list.hovered == [2])
        #expect(presenter.model?.location == .inlineButton)

        presenter.close()
        #expect(!add.isSelected)
    }

    @Test
    func addReactionsSubmenuListsItsEmojiThenViewMoreWhichOpensThePicker() throws {
        list.actions.menuEmoji[2] = [
            QuickReaction(emoji: .unicode("💯"), mine: false), QuickReaction(emoji: .unicode("👍"), mine: true),
        ]
        show()
        let model = ReactionMenuActions(
            list.actions,
            groups: [[MessageTableTests.RecordingActions.item(.addReaction, "Add Reaction")]])
        list.controller.actions = model
        let cell = try #require(try list.cell(of: 2) as? MessageCell)
        let add = try #require(cell.menuItems().first { $0.title == "Add Reaction" }?.submenu)

        #expect(add.items.map(\.title) == [":100:", ":thumbsup:", "", "View More"])
        #expect(add.items[2].isSeparatorItem)
        add.performActionForItem(at: 0)
        #expect(list.actions.calls.last == "react :100: on 2 from contextMenu")

        add.performActionForItem(at: 3)
        NotificationCenter.default.post(name: NSMenu.didEndTrackingNotification, object: add)

        #expect(list.actions.calls.contains("picker for 2 from picker"))
        #expect(presenter.windows.panel != nil)
        #expect(presenter.model?.location == .picker)
        #expect(list.hovered == [2], "the menu's end leaves the picker's row lit")
    }

    @Test
    func reactToPostOpensThePickerRightOfItsButtonWithoutLightingARow() throws {
        var state = MessageTableState(rows: [
            row(1, by: author(2, "Ray"), at: list.noon, content: "the post"),
            row(2, by: author(3), at: list.noon + 600, content: "an answer"),
        ])
        state.reachedOldest = true
        state.postBar = PostBar(
            post: MessageId(rawValue: 1), following: false, canFollow: true, invites: false,
            reactions: [], defaultReaction: nil, reacts: true, toggles: true)
        list.controller.show(state)
        list.window.layoutIfNeeded()
        let index = try #require(list.controller.timeline.items.firstIndex { $0.id == .postBar })
        let cell = try #require(list.table.view(atColumn: 0, row: index, makeIfNecessary: true) as? PostBarCell)
        cell.layoutSubtreeIfNeeded()
        let button = screen(cell.react)

        cell.react.performClick(nil)

        #expect(list.actions.calls.contains("picker for 1 from forumToolbar"))
        let panel = try #require(presenter.windows.panel)
        #expect(panel.frame.minX == button.maxX + 8)
        #expect(list.hovered.isEmpty)
    }

    @Test
    func theRowLeavingTheListClosesThePicker() throws {
        show()
        try list.hover(2)
        try #require(list.bar.button(.addReaction)).performClick(nil)
        let model = try #require(presenter.model)

        list.show([row(1, by: author(2, "Ray"), at: list.noon, content: "first")])

        #expect(presenter.windows.panel == nil)
        #expect(!model.isOpen)
    }

    @Test
    func theListGoingClosesThePicker() throws {
        show()
        try list.hover(2)
        try #require(list.bar.button(.addReaction)).performClick(nil)

        list.controller.dismantle()

        #expect(presenter.windows.panel == nil)
    }

    @Test
    func aPickReactsWithTheMessageAndLetsTheRowGo() async throws {
        account.reactionsStore.picker.withLock { $0.recent = [pickerEmoji("🔥")] }
        show()
        try list.hover(2)
        try #require(list.bar.button(.addReaction)).performClick(nil)
        let view = try #require(presenter.view)
        try list.hover(3)

        view.grid.click(at: NSPoint(x: 32, y: 52), shift: false)

        #expect(presenter.windows.panel == nil)
        #expect(list.hovered == [3])
        try await eventually { account.made.withLock { $0 } == [":fire: from picker"] }
    }
}
