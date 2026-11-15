import AkariKit
import AppKit
import Testing

@testable import Akari

// The inline editor in its row: the list's one editor, in the edited message's text's place.
@MainActor
struct MessageEditorTests {
    let list = BarList()

    var editor: MessageEditorView { list.controller.editor }

    func edit(_ raw: UInt64, text: String, problem: EditProblem? = nil, opening: Int = 1) {
        list.actions.editing = text
        list.editing = EditModel.Editing(messageId: MessageId(rawValue: raw), problem: problem, opening: opening)
        list.show(list.rows)
    }

    func cell(_ raw: UInt64) throws -> MessageCell {
        try #require(try list.cell(of: raw) as? MessageCell)
    }

    // What Auto Layout gives the shown cell at the column's width.
    func fittingHeight(of raw: UInt64) throws -> CGFloat {
        let cell = try cell(raw)
        let width = cell.widthAnchor.constraint(equalToConstant: list.table.tableColumns[0].width)
        cell.translatesAutoresizingMaskIntoConstraints = false
        width.isActive = true
        defer {
            width.isActive = false
            cell.translatesAutoresizingMaskIntoConstraints = true
        }
        cell.layoutSubtreeIfNeeded()
        return cell.fittingSize.height
    }

    func height(of raw: UInt64) -> CGFloat {
        list.table.rect(ofRow: list.index(of: raw)).height
    }

    func type(_ text: String) {
        editor.textView.insertText(text, replacementRange: editor.textView.selectedRange())
        list.window.layoutIfNeeded()
    }

    func threeMessages() {
        list.show([
            row(1, at: list.noon, content: "first"),
            row(2, at: list.noon + 10, content: "second"),
            row(3, by: author(2, "Ray"), at: list.noon + 600, content: "third"),
        ])
    }

    @Test
    func theEditorTakesTheTextsPlaceAndTheRestOfTheRowStays() throws {
        threeMessages()
        let before = height(of: 1)

        edit(1, text: "first")

        let cell = try cell(1)
        #expect(cell.editor === editor)
        #expect(editor.superview != nil)
        #expect(editor.textView.string == "first")
        #expect(MessageListFixture.messageText(in: cell)?.isHidden != false, "the text gives way")
        #expect(cell.isEditing)
        #expect(height(of: 1) > before)
        #expect(abs(height(of: 1) - (try fittingHeight(of: 1))) <= 0.5, "the row's height is the cell's")
        #expect(try self.cell(2).editor == nil)
    }

    // The composer's box: 56 for one line, 8 under the header; the line under it 30 high.
    @Test
    func theEditorsGeometryIsTheComposersAndTheOfficialHints() throws {
        threeMessages()
        edit(1, text: "first")

        #expect(editor.height(width: 400) == MessageEditorView.top + 56 + 7 + EditorHint.height + 7)
        #expect(editor.hint.text == "escape to cancel • enter to save")
        let cell = try cell(1)
        let box = editor.convert(editor.textView.frame, to: cell)
        #expect(box.minX == 72)
        #expect(box.width == cell.bounds.width - 72 - 16)
    }

    @Test
    func editOnTheRowBeingEditedGivesTheEditorTheFocusWithTheTextAndCaretAsTheyAre() {
        threeMessages()
        edit(1, text: "first")
        type(" more")
        editor.textView.setSelectedRange(NSRange(location: 2, length: 0))
        list.window.makeFirstResponder(list.table)

        list.editing = EditModel.Editing(messageId: MessageId(rawValue: 1), opening: 1, focus: 1)
        list.show(list.rows)

        #expect(list.window.firstResponder === editor.textView)
        #expect(editor.textView.string == "first more")
        #expect(editor.textView.selectedRange() == NSRange(location: 2, length: 0))
    }

    @Test
    func anEmptyEditorIsALineTall() {
        threeMessages()

        edit(1, text: "")

        #expect(editor.height(width: 400) == MessageEditorView.top + 56 + 7 + EditorHint.height + 7)
    }

    @Test
    func theCaretOnANewLastLineIsThatLine() {
        threeMessages()
        edit(1, text: "first")
        let first = editor.caretRect

        type("\n")

        let second = editor.caretRect
        #expect(abs(second.minY - first.minY - ComposerTextView.lineHeight) <= 0.5)
        #expect(second.height >= ComposerTextView.lineHeight)
        editor.textView.setSelectedRange(NSRange(location: 2, length: 0))
        #expect(editor.caretRect.height >= ComposerTextView.lineHeight, "a line's height in its middle")
    }

    @Test
    func aLongReasonWrapsAndTheRowMakesItsRoom() throws {
        threeMessages()
        edit(1, text: "first")
        let reason = String(repeating: "This community doesn't allow these words. ", count: 7)

        edit(1, text: "first", problem: .refused(.MessageBlocked(reason: reason)))

        #expect(editor.problem.frame.height > 2 * TypeScale.small.line)
        #expect(abs(height(of: 1) - (try fittingHeight(of: 1))) <= 0.5)
        #expect(editor.problem.frame.minX == -2, "its text on the box's edge")
    }

    @Test
    func anEditOpenedWithFewCharactersLeftShowsTheCounter() {
        threeMessages()
        list.actions.left = 150

        edit(1, text: "first")

        #expect(!editor.counter.isHidden)
        #expect(editor.counter.stringValue == "150")
    }

    @Test
    func eachOpeningStartsItsOwnUndoApartFromTheWindows() throws {
        threeMessages()
        edit(1, text: "first")
        type(" draft")
        let undo = try #require(editor.textView.undoManager)
        #expect(undo.canUndo)
        #expect(undo !== list.window.undoManager)

        edit(2, text: "second", opening: 2)

        #expect(editor.textView.undoManager?.canUndo == false)
    }

    @Test
    func theEditorsEmptyPartsAndItsBoxAreTheRows() throws {
        threeMessages()
        list.actions.left = 150
        edit(1, text: "first")
        let parent = try #require(editor.superview)
        func hit(_ point: NSPoint) -> NSView? {
            editor.hitTest(editor.convert(point, to: parent))
        }

        #expect(hit(NSPoint(x: 10, y: editor.textView.frame.maxY + 3)) == nil, "between the box and the hint")
        #expect(hit(NSPoint(x: editor.bounds.maxX - 4, y: editor.textView.frame.midY)) == nil, "the box beside the counter")
        #expect(hit(NSPoint(x: editor.textView.frame.midX - 40, y: editor.textView.frame.midY)) === editor.textView)
    }

    @Test
    func theCounterSitsOnTheLastLineAndIsPlainAtZero() {
        threeMessages()
        edit(1, text: "first\nsecond\nthird")
        list.actions.left = 0

        type("!")

        let lastLine = editor.textView.frame.maxY - ComposerTextView.insets.height - ComposerTextView.lineHeight / 2
        #expect(abs(editor.counter.frame.midY - lastLine) <= 1)
        #expect(editor.counter.textColor == Palette.textDefault)
    }

    @Test
    func theBoxsBorderFollowsTheFocus() throws {
        threeMessages()
        edit(1, text: "first")
        let box = try #require(editor.subviews.compactMap { $0 as? EditorBox }.first)
        func border(_ color: NSColor) -> CGColor? {
            var resolved: CGColor?
            box.effectiveAppearance.performAsCurrentDrawingAppearance { resolved = color.cgColor }
            return resolved
        }
        box.displayIfNeeded()
        #expect(box.focused)
        #expect(box.layer?.borderColor == border(Palette.borderSubtle))

        list.window.makeFirstResponder(list.table)
        box.displayIfNeeded()

        #expect(!box.focused)
        #expect(box.layer?.borderColor == border(Palette.borderMuted))
    }

    @Test
    func theEditorHasItsVoiceOverLabelAndShiftPageUpJumpsToUnread() {
        threeMessages()
        edit(1, text: "first")

        editor.textView.doCommand(by: #selector(NSResponder.pageUpAndModifySelection(_:)))

        #expect(editor.textView.accessibilityLabel() == "Edit Message")
        #expect(list.actions.calls.last == "unread")
    }

    @Test
    func theHintsLinksAreDrawnInTheLinksColorAndRedrawnUnderThePointer() throws {
        threeMessages()
        edit(1, text: "first")
        let hint = editor.hint
        func drawn() throws -> NSBitmapImageRep {
            let rep = try #require(hint.bitmapImageRepForCachingDisplay(in: hint.bounds))
            hint.cacheDisplay(in: hint.bounds, to: rep)
            return rep
        }
        func blue(in rect: NSRect, of rep: NSBitmapImageRep) -> Bool {
            let scale = CGFloat(rep.pixelsWide) / hint.bounds.width
            for x in Int(rect.minX * scale)..<Int(rect.maxX * scale) {
                for y in Int(rect.minY * scale)..<Int(rect.maxY * scale) {
                    if let color = rep.colorAt(x: x, y: y)?.usingColorSpace(.sRGB),
                        color.blueComponent - color.redComponent > 0.25, color.alphaComponent > 0.5
                    {
                        return true
                    }
                }
            }
            return false
        }
        let plain = try drawn()
        #expect(blue(in: hint.save.frame, of: plain))
        #expect(!blue(in: NSRect(x: 0, y: 0, width: hint.cancel.frame.minX, height: hint.bounds.height), of: plain))

        list.window.displayIfNeeded()
        let layer = try #require(hint.layer)
        layer.displayIfNeeded()
        try #require(!layer.needsDisplay())
        hint.save.isHovered = true
        #expect(layer.needsDisplay())
        let hovered = try drawn()
        #expect(hovered.tiffRepresentation != plain.tiffRepresentation, "the underline")
    }

    @Test
    func theEditorTakesTheFocusWhenItOpens() {
        threeMessages()

        edit(2, text: "second")

        #expect(list.window.firstResponder === editor.textView)
        #expect(editor.textView.selectedRange() == NSRange(location: 6, length: 0), "the caret at the end")
    }

    @Test
    func typingReachesTheModelAndGrowsTheRowLineByLine() throws {
        threeMessages()
        edit(1, text: "first")
        let one = height(of: 1)
        let third = list.table.rect(ofRow: list.index(of: 3)).minY

        type("\nsecond line\nthird line")

        #expect(list.actions.editing == "first\nsecond line\nthird line")
        #expect(height(of: 1) == one + 2 * ComposerTextView.lineHeight)
        #expect(abs(height(of: 1) - (try fittingHeight(of: 1))) <= 0.5)
        #expect(list.table.rect(ofRow: list.index(of: 3)).minY == third + 2 * ComposerTextView.lineHeight)
        #expect(list.window.firstResponder === editor.textView, "typing keeps the focus")
    }

    @Test
    func aRowAboveTheReaderGrowsWithoutMovingWhatTheyRead() {
        list.show(list.many(40))
        list.clip.scroll(to: NSPoint(x: 0, y: list.table.rect(ofRow: list.index(of: 20)).minY))
        list.controller.scrollView.reflectScrolledClipView(list.clip)
        edit(20, text: "message 20")
        let reading = list.table.rect(ofRow: list.index(of: 24)).minY - list.clip.bounds.minY

        type("\nmore")

        #expect(list.table.rect(ofRow: list.index(of: 24)).minY - list.clip.bounds.minY == reading + 22)
        let top = list.table.rect(ofRow: list.index(of: 20)).minY - list.clip.bounds.minY
        #expect(top >= 0, "the edited row stays where it was")
    }

    // The edited row wholly above the reader grows by a refusal's reason: what they read stays.
    @Test
    func anEditedRowAboveTheViewGrowsWithoutMovingWhatIsRead() throws {
        list.show(list.many(40))
        edit(10, text: "message 10")
        list.window.makeFirstResponder(list.table)
        scroll(to: list.table.rect(ofRow: list.index(of: 25)).minY)
        try #require(!list.controller.isPinnedToBottom)
        let reading = list.table.rect(ofRow: list.index(of: 30)).minY - list.clip.bounds.minY

        edit(10, text: "message 10", problem: .refused(.Forbidden))

        #expect(list.table.rect(ofRow: list.index(of: 30)).minY - list.clip.bounds.minY == reading)
    }

    @Test
    func returnSavesShiftReturnAddsALineAndEscapeCancels() {
        threeMessages()
        edit(1, text: "first")
        var event: NSEvent?
        editor.textView.currentEvent = { event }

        editor.textView.doCommand(by: #selector(NSResponder.insertNewline(_:)))
        #expect(list.actions.calls.last == "save edit")

        event = keyDown("\r", keyCode: 36, modifiers: .shift)
        editor.textView.doCommand(by: #selector(NSResponder.insertNewline(_:)))
        #expect(editor.textView.string == "first\n")

        event = escapeKey()
        editor.textView.doCommand(by: #selector(NSResponder.cancelOperation(_:)))
        #expect(list.actions.calls.last == "cancel edit")

        event = escapeKey(.shift)
        editor.textView.doCommand(by: #selector(NSResponder.cancelOperation(_:)))
        #expect(list.actions.calls.last == "server read")
        #expect(list.actions.calls.filter { $0 == "save edit" }.count == 1)
    }

    @Test
    func theHintsLinksSaveAndCancel() {
        threeMessages()
        edit(1, text: "first")

        editor.hint.save.performClick(nil)
        #expect(list.actions.calls.last == "save edit")
        editor.hint.cancel.performClick(nil)
        #expect(list.actions.calls.last == "cancel edit")
    }

    @Test
    func theHintsLinksLieOverTheirWords() {
        threeMessages()
        edit(1, text: "first")

        func width(_ text: String) -> CGFloat {
            NSAttributedString(string: text, attributes: [.font: TypeScale.small.nsFont]).size().width
        }
        let cancel = editor.hint.cancel.frame
        let save = editor.hint.save.frame
        #expect(abs(cancel.minX - width("escape to ")) <= 0.5)
        #expect(abs(cancel.width - width("cancel")) <= 1)
        #expect(abs(save.minX - width("escape to cancel • enter to ")) <= 0.5)
        #expect(abs(save.width - width("save")) <= 1)
    }

    @Test
    func theCounterSaysWhatIsLeftOnceFewAre() {
        threeMessages()
        edit(1, text: "first")
        #expect(editor.counter.isHidden)

        list.actions.left = 150
        type("!")
        #expect(!editor.counter.isHidden)
        #expect(editor.counter.stringValue == "150")
        #expect(editor.counter.toolTip == "150 characters remaining")
        #expect(editor.textView.frame.width == editor.bounds.width - MessageEditorView.counterRoom)

        list.actions.left = 0
        type("!")
        #expect(editor.counter.toolTip == "Message is at max length")

        list.actions.left = -3
        type("!")
        #expect(editor.counter.stringValue == "-3")
        #expect(editor.counter.toolTip == "Message is too long")
        #expect(editor.counter.textColor == Palette.danger)

        list.actions.left = nil
        type("!")
        #expect(editor.counter.isHidden)
        #expect(editor.textView.frame.width == editor.bounds.width)
    }

    // A line that fits the box until the counter takes its room.
    func lineNearTheEdge() -> String {
        let room = editor.textView.frame.width - 2 * ComposerTextView.insets.width
        var line = ""
        while NSAttributedString(string: line + "a", attributes: ComposerTextView.attributes).size().width < room - 20 {
            line += "a"
        }
        return line
    }

    @Test
    func theRowFollowsTheTextAsTheCounterComesAndGoes() throws {
        threeMessages()
        edit(1, text: "first")
        let one = height(of: 1)
        let line = lineNearTheEdge()
        edit(1, text: line, opening: 2)
        #expect(height(of: 1) == one)

        list.actions.left = 150
        type("a")
        #expect(height(of: 1) == one + ComposerTextView.lineHeight, "the counter's room wraps the line")
        #expect(abs(height(of: 1) - (try fittingHeight(of: 1))) <= 0.5)

        list.actions.left = nil
        type("a")
        #expect(height(of: 1) == one)
        #expect(abs(height(of: 1) - (try fittingHeight(of: 1))) <= 0.5)
    }

    @Test
    func aNewLineAtTheEndGrowsTheRowAtOnce() throws {
        threeMessages()
        edit(1, text: "first")
        let one = height(of: 1)

        type("\n")

        #expect(height(of: 1) == one + ComposerTextView.lineHeight)
        #expect(abs(height(of: 1) - (try fittingHeight(of: 1))) <= 0.5)
        let caret = editor.caretRect
        #expect(caret.maxY <= editor.textView.frame.maxY - ComposerTextView.insets.height + 0.5)
    }

    @Test
    func theEditorsFocusReachesTheModel() {
        threeMessages()

        edit(1, text: "first")
        #expect(list.actions.editorFocus.last == true)

        list.window.makeFirstResponder(list.table)
        #expect(list.actions.editorFocus.last == false)
    }

    @Test(arguments: [150, 400, -1234])
    func theCounterHoldsItsWholeTextInsideTheBox(left: Int) throws {
        threeMessages()
        edit(1, text: "first")
        let one = height(of: 1)

        list.actions.left = left
        type("!")

        let counter = editor.counter
        let text = counter.attributedStringValue.size().width
        let cell = try #require(counter.cell).cellSize.width
        #expect(counter.frame.width >= max(text, cell), "\(counter.stringValue): \(counter.frame.width) for \(text)")
        let box = NSRect(
            x: 0, y: MessageEditorView.top, width: editor.bounds.width,
            height: editor.textView.frame.height)
        #expect(box.contains(counter.frame))
        let textEnd = counter.frame.maxX - (cell - text) / 2
        #expect(abs(box.maxX - textEnd - MessageEditorView.counterTrailing) <= 0.5, "the digits end 14 in")
        #expect(
            editor.textView.frame.maxX - ComposerTextView.insets.width <= counter.frame.minX,
            "the text wraps before the counter")
        #expect(height(of: 1) == one)
        #expect(abs(height(of: 1) - (try fittingHeight(of: 1))) <= 0.5)
    }

    @Test
    func aRefusalsReasonStandsUnderTheBoxAndTheRowMakesRoom() throws {
        threeMessages()
        edit(1, text: "first")
        let plain = height(of: 1)
        list.window.makeFirstResponder(editor.textView)

        edit(1, text: "first", problem: .refused(.Forbidden))

        #expect(editor.problemText == "You don't have permission to edit this message.")
        #expect(height(of: 1) > plain + 7)
        #expect(abs(height(of: 1) - (try fittingHeight(of: 1))) <= 0.5)
        #expect(editor.textView.string == "first", "the same opening keeps the text")
        #expect(list.window.firstResponder === editor.textView)
        #expect(editor.problem.frame.minY == editor.textView.frame.maxY + 7)
        #expect(editor.hint.frame.minY == editor.problem.frame.maxY + 7)
    }

    @Test
    func aChangeOfTheEditedMessageKeepsTheEditorItsTextAndFocus() throws {
        threeMessages()
        edit(1, text: "first")
        type(" draft")

        var changed = list.rows
        changed[0] = row(1, at: list.noon, content: "first", edited: list.noon + 5)
        list.show(changed)

        #expect(try cell(1).editor === editor)
        #expect(editor.textView.string == "first draft")
        #expect(list.window.firstResponder === editor.textView)
    }

    @Test
    func anUpdateOfTheEditedRowLeavesTheFocusWhereTheUserPutIt() throws {
        threeMessages()
        edit(1, text: "first")
        list.window.makeFirstResponder(list.table)

        var changed = list.rows
        changed[0] = row(1, at: list.noon, content: "first", edited: list.noon + 5)
        list.show(changed)

        #expect(try cell(1).editor === editor)
        #expect(list.window.firstResponder === list.table)
    }

    @Test
    func aReloadOfTheRowsKeepsTheEditorsFocus() throws {
        threeMessages()
        edit(1, text: "first")
        try #require(list.window.firstResponder === editor.textView)

        list.controller.show(
            MessageTableState(rows: list.rows, reachedOldest: true, showsAltText: true, editing: list.editing))
        list.window.layoutIfNeeded()

        #expect(try cell(1).editor === editor)
        #expect(list.window.firstResponder === editor.textView)
    }

    @Test
    func endingTheEditGivesTheRowItsTextBack() throws {
        threeMessages()
        let plain = height(of: 1)
        edit(1, text: "first")

        list.editing = nil
        list.show(list.rows)

        #expect(try cell(1).editor == nil)
        #expect(editor.superview == nil)
        #expect(MessageListFixture.messageText(in: try cell(1))?.isHidden == false)
        #expect(height(of: 1) == plain)
        #expect(try !cell(1).isEditing)
    }

    @Test
    func anotherEditMovesTheEditorWithItsNewText() throws {
        threeMessages()
        edit(1, text: "first")

        edit(2, text: "second", opening: 2)

        #expect(try cell(1).editor == nil)
        #expect(try cell(2).editor === editor)
        #expect(editor.textView.string == "second")
        #expect(list.window.firstResponder === editor.textView)
    }

    @Test
    func aRowThatStartsOrStopsBeingEditedIsRedrawn() throws {
        threeMessages()
        let cell = try cell(1)
        list.window.displayIfNeeded()
        let layer = try #require(cell.layer)
        layer.displayIfNeeded()
        try #require(!layer.needsDisplay())

        cell.isEditing = true

        #expect(layer.needsDisplay())
    }

    @Test
    func theEditedRowStaysLitAndOpensNoMessageMenu() throws {
        threeMessages()
        list.actions.marksUnread = [1, 2]
        edit(1, text: "first")
        let cell = try cell(1)
        let other = try self.cell(2)

        #expect(cell.isEditing)
        #expect(cell.messageMenu(for: try rightClick(in: cell)) == nil)
        #expect(other.messageMenu(for: try rightClick(in: other)) != nil)
    }

    func rightClick(in cell: MessageCell) throws -> NSEvent {
        let point = cell.convert(NSPoint(x: cell.box.minX + 20, y: cell.box.midY), to: nil)
        return try #require(
            NSEvent.mouseEvent(
                with: .rightMouseDown, location: point, modifierFlags: [], timestamp: 0,
                windowNumber: list.window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                pressure: 1))
    }

    @Test
    func anEditOpenedAwayFromItsRowScrollsItIntoViewWithRoomAround() {
        list.show(list.many(40))
        #expect(list.controller.isPinnedToBottom)

        edit(8, text: "message 8")

        #expect(list.window.firstResponder === editor.textView, "the focus once its row shows")
        let rect = list.table.rect(ofRow: list.index(of: 8))
        #expect(rect.minY - list.clip.bounds.minY >= MessageEditorView.revealPadding - 0.5)
        #expect(rect.maxY <= list.clip.bounds.maxY)
    }

    @Test
    func aRowTallerThanTheListOpenedBelowTheViewShowsItsTop() {
        var rows = list.many(40)
        rows[19] = row(20, at: list.noon + 2400, content: Array(repeating: "line", count: 40).joined(separator: "\n"))
        list.show(rows)
        scroll(to: 0)

        edit(20, text: rows[19].message.content)

        let rect = list.table.rect(ofRow: list.index(of: 20))
        #expect(rect.minY >= list.clip.bounds.minY - 0.5)
        #expect(rect.minY < list.clip.bounds.maxY)
    }

    @Test
    func anEditOpenedBelowTheViewShowsItsRowWithRoomUnderIt() {
        list.show(list.many(40))
        scroll(to: 0)

        edit(20, text: "message 20")

        let rect = list.table.rect(ofRow: list.index(of: 20))
        let bottom = list.clip.bounds.maxY - list.controller.scrollView.contentInsets.bottom
        #expect(abs(rect.maxY + MessageEditorView.revealPadding - bottom) <= 0.5)
    }

    @Test
    func anEditOpenedAboveTheViewShowsItsRowBelowTheStartsRoom() {
        list.show(list.many(40))
        list.controller.show(MessageTableState(rows: list.rows, reachedOldest: true, startRoom: 40))
        list.window.layoutIfNeeded()
        #expect(list.controller.isPinnedToBottom)

        list.actions.editing = "message 20"
        list.editing = EditModel.Editing(messageId: MessageId(rawValue: 20), opening: 1)
        list.controller.show(
            MessageTableState(rows: list.rows, reachedOldest: true, editing: list.editing, startRoom: 40))
        list.window.layoutIfNeeded()

        let rect = list.table.rect(ofRow: list.index(of: 20))
        #expect(abs(rect.minY - MessageEditorView.revealPadding - (list.clip.bounds.minY + 40)) <= 0.5)
    }

    @Test
    func anEditOpenedOnTheFirstRowScrollsNoFurtherThanTheListsTop() {
        list.show(list.many(40))

        edit(1, text: "message 1")

        #expect(list.clip.bounds.minY >= -0.5)
        #expect(list.table.rect(ofRow: list.index(of: 1)).minY >= list.clip.bounds.minY - 0.5)
    }

    // The reveal is the list's own scroll, and its new place is where the reader is.
    @Test
    func theRevealIsNoScrollOfTheUsersAndKeepsItsPlace() {
        list.show(list.many(40))
        list.actions.scrolls = []

        edit(8, text: "message 8")
        let offset = list.table.rect(ofRow: list.index(of: 8)).minY - list.clip.bounds.minY

        #expect(list.actions.scrolls.isEmpty)
        list.show((1...5).map { row(UInt64(100 + $0), at: list.noon - Double(100 - $0) * 60) } + list.rows)
        #expect(abs(list.table.rect(ofRow: list.index(of: 8)).minY - list.clip.bounds.minY - offset) <= 0.5)
    }

    @Test
    func aRevealNearTheListsTopAsksForOlderMessages() {
        let rows = list.many(40)
        list.controller.show(MessageTableState(rows: rows, reachedOldest: false))
        list.window.layoutIfNeeded()
        list.actions.calls.removeAll()

        list.actions.editing = "message 2"
        list.editing = EditModel.Editing(messageId: MessageId(rawValue: 2), opening: 1)
        list.controller.show(MessageTableState(rows: rows, reachedOldest: false, editing: list.editing))
        list.window.layoutIfNeeded()

        #expect(list.actions.calls.contains("more older"))
    }

    @Test
    func theCaretMovedNearTheListsTopAsksForOlderMessages() throws {
        var rows = list.many(40)
        rows[2] = row(3, at: list.noon + 360, content: Array(repeating: "line", count: 80).joined(separator: "\n"))
        list.controller.show(MessageTableState(rows: rows, reachedOldest: false))
        list.window.layoutIfNeeded()
        list.actions.editing = rows[2].message.content
        list.editing = EditModel.Editing(messageId: MessageId(rawValue: 3), opening: 1)
        let editing = MessageTableState(rows: rows, reachedOldest: false, editing: list.editing)
        list.controller.show(editing)
        list.window.layoutIfNeeded()
        editor.textView.moveToEndOfDocument(nil)
        list.window.layoutIfNeeded()
        // Far from the top: a new state asks for nothing more, and the next ask is the caret's.
        list.controller.show(editing)
        list.window.layoutIfNeeded()
        list.actions.calls.removeAll()

        editor.textView.moveToBeginningOfDocument(nil)

        #expect(list.actions.calls.contains("more older"))
    }

    @Test
    func theCaretMovedAboveTheViewIsScrolledToWithRoomAboveIt() throws {
        list.show(list.many(40))
        edit(40, text: Array(repeating: "line", count: 30).joined(separator: "\n"))
        editor.textView.moveToEndOfDocument(nil)
        list.window.layoutIfNeeded()
        let start = editor.convert(editor.caretRect, to: list.table)
        try #require(start.minY > list.clip.bounds.minY + list.clip.bounds.height / 2)

        editor.textView.moveToBeginningOfDocument(nil)

        let caret = editor.convert(editor.caretRect, to: list.table)
        #expect(caret.minY < start.minY)
        #expect(abs(caret.minY - MessageEditorView.caretMargin - list.clip.bounds.minY) <= 0.5)
    }

    @Test
    func nsTextViewsOwnScrollDoesNothingInTheList() throws {
        list.show(list.many(40))
        edit(40, text: Array(repeating: "line", count: 30).joined(separator: "\n"))
        editor.textView.moveToEndOfDocument(nil)
        list.window.layoutIfNeeded()
        let start = editor.convert(editor.textView.frame, to: list.table).minY
        try #require(start < list.clip.bounds.minY, "the text's start is out of sight")
        let origin = list.clip.bounds.origin

        editor.textView.scrollRangeToVisible(NSRange(location: 0, length: 0))

        #expect(list.clip.bounds.origin == origin)
    }

    @Test
    func aReasonArrivingWhileTheUserIsElsewhereScrollsNothing() {
        list.show(list.many(40))
        edit(40, text: "message 40")
        list.window.makeFirstResponder(list.table)
        scroll(to: 0)
        let origin = list.clip.bounds.origin

        edit(40, text: "message 40", problem: .refused(.Forbidden))

        #expect(list.clip.bounds.origin == origin)
    }

    @Test
    func typingPastTheListsBottomKeepsTheCaretInSight() {
        list.show(list.many(40))
        edit(40, text: "message 40")

        type(String(repeating: "\nline", count: 20))

        let caret = editor.convert(editor.caretRect, to: list.table)
        let bottom = list.clip.bounds.maxY - list.controller.scrollView.contentInsets.bottom
        #expect(caret.maxY <= bottom)
        #expect(caret.minY >= list.clip.bounds.minY)
    }

    // Away from the bottom the list keeps the reader's place, so only the reveal brings the
    // caret back.
    @Test
    func typingPastWhatShowsInTheMiddleOfTheListBringsTheCaretBack() {
        list.show(list.many(40))
        edit(20, text: "message 20")
        let rect = list.table.rect(ofRow: list.index(of: 20))
        let bottom = list.clip.bounds.height - list.controller.scrollView.contentInsets.bottom
        list.clip.scroll(to: NSPoint(x: 0, y: rect.maxY - bottom))
        list.controller.scrollView.reflectScrolledClipView(list.clip)
        #expect(!list.controller.isPinnedToBottom)

        #expect(list.window.firstResponder === editor.textView, "a scroll keeps the focus")

        type(String(repeating: "\nline", count: 12))

        let caret = editor.convert(editor.caretRect, to: list.table)
        #expect(caret.maxY <= list.clip.bounds.maxY - list.controller.scrollView.contentInsets.bottom)
        #expect(caret.minY >= list.clip.bounds.minY)
    }

    func scroll(to y: CGFloat) {
        list.clip.scroll(to: NSPoint(x: 0, y: y))
        list.controller.scrollView.reflectScrolledClipView(list.clip)
        list.window.layoutIfNeeded()
    }

    @Test
    func theEditorScrolledAwayAndBackKeepsTheFocus() throws {
        list.show(list.many(40))
        edit(40, text: "message 40")
        let bottom = list.clip.bounds.minY

        scroll(to: 0)
        scroll(to: bottom)

        #expect(editor.window != nil)
        #expect(list.window.firstResponder === editor.textView)
    }

    @Test
    func theEditorScrolledBackLeavesTheFocusWhereTheUserPutIt() throws {
        list.show(list.many(40))
        edit(40, text: "message 40")
        let bottom = list.clip.bounds.minY
        scroll(to: 0)

        list.window.makeFirstResponder(list.table)
        scroll(to: bottom)

        #expect(list.window.firstResponder === list.table)
    }

    @Test
    func theEditorsTextHasTheIBeamAndItsLinksTheHandAndTheUnderline() throws {
        threeMessages()
        edit(1, text: "first")
        let text = editor.convert(NSPoint(x: editor.textView.frame.midX, y: editor.textView.frame.midY), to: list.table)
        list.move(to: text)
        #expect(list.controller.cursor(at: list.table.convert(text, to: nil)) == .iBeam)

        let save = editor.hint.save
        let link = save.convert(NSPoint(x: save.bounds.midX, y: save.bounds.midY), to: list.table)
        list.move(to: link)
        #expect(list.controller.cursor(at: list.table.convert(link, to: nil)) == .pointingHand)
        #expect(save.underlined)
        #expect(!editor.hint.cancel.underlined)

        list.move(to: text)
        #expect(!save.underlined)
    }

    @Test
    func anOptionClickInTheEditorStaysTheEditors() throws {
        threeMessages()
        list.actions.marksUnread = [1]
        edit(1, text: "first")
        let point = editor.convert(NSPoint(x: editor.textView.frame.midX, y: editor.textView.frame.midY), to: list.table)
        list.move(to: point)

        #expect(!list.controller.optionClick(try list.click(.option, at: point)))
        let header = try list.box(of: 1)
        #expect(list.controller.optionClick(try list.click(.option, at: NSPoint(x: header.minX + 100, y: header.minY + 10))))
    }
}
