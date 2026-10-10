import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

@MainActor
final class ComposerTextViewTests {
    let scrollView: NSScrollView
    let textView: ComposerTextView
    let window: NSWindow
    var submits = 0
    var escapes = 0
    var editLast = 0
    var modifiers: NSEvent.ModifierFlags = []

    init() {
        (scrollView, textView) = ComposerTextView.scrollable()
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 500, height: 400), styleMask: [.titled],
            backing: .buffered, defer: false)
        scrollView.frame = NSRect(x: 0, y: 0, width: 500, height: 56)
        window.contentView?.addSubview(scrollView)
        textView.onSubmit = { [unowned self] in submits += 1 }
        textView.onEscape = { [unowned self] in escapes += 1 }
        textView.onEditLastMessage = { [unowned self] in editLast += 1 }
        textView.modifiers = { [unowned self] in modifiers }
        window.makeFirstResponder(textView)
    }

    func type(_ text: String) {
        textView.insertText(text, replacementRange: textView.selectedRange())
    }

    @Test
    func returnSubmitsAndShiftReturnAddsALine() {
        type("a")
        textView.doCommand(by: #selector(NSResponder.insertNewline(_:)))
        #expect(submits == 1)
        #expect(textView.string == "a")

        modifiers = .shift
        textView.doCommand(by: #selector(NSResponder.insertNewline(_:)))
        #expect(submits == 1)
        #expect(textView.string == "a\n")
    }

    @Test
    func controlReturnAddsAPlainNewline() {
        type("a")
        textView.doCommand(by: #selector(NSResponder.insertLineBreak(_:)))
        #expect(submits == 0)
        #expect(textView.string == "a\n")
    }

    @Test
    func optionReturnAddsALine() {
        type("a")
        textView.doCommand(by: #selector(NSResponder.insertNewlineIgnoringFieldEditor(_:)))
        #expect(submits == 0)
        #expect(textView.string == "a\n")
    }

    @Test
    func returnCommitsMarkedTextInsteadOfSending() {
        textView.setMarkedText(
            "にほん", selectedRange: NSRange(location: 3, length: 0),
            replacementRange: NSRange(location: NSNotFound, length: 0))
        #expect(textView.hasMarkedText())

        textView.doCommand(by: #selector(NSResponder.insertNewline(_:)))

        #expect(submits == 0)
    }

    @Test
    func upArrowInAnEmptyComposerIsReserved() {
        textView.doCommand(by: #selector(NSResponder.moveUp(_:)))
        #expect(editLast == 1)

        type("a\nb")
        textView.doCommand(by: #selector(NSResponder.moveUp(_:)))
        #expect(editLast == 1)
        #expect(textView.selectedRange().location <= 1)
    }

    @Test
    func escapeJumpsInsteadOfCompleting() {
        type("ab")
        textView.doCommand(by: #selector(NSResponder.cancelOperation(_:)))
        #expect(escapes == 1)
        #expect(textView.string == "ab")
    }

    @Test
    func pastedRichTextArrivesPlain() throws {
        let pasteboard = NSPasteboard(name: NSPasteboard.Name("app.akari.tests.\(UUID())"))
        defer { pasteboard.releaseGlobally() }
        let rich = NSAttributedString(
            string: "bold red",
            attributes: [.font: NSFont.boldSystemFont(ofSize: 30), .foregroundColor: NSColor.red])
        let rtf = try rich.data(
            from: NSRange(location: 0, length: rich.length),
            documentAttributes: [.documentType: NSAttributedString.DocumentType.rtf])
        pasteboard.declareTypes([.rtf, .string], owner: nil)
        pasteboard.setData(rtf, forType: .rtf)
        pasteboard.setString("bold red", forType: .string)

        #expect(textView.readSelection(from: pasteboard))

        #expect(textView.string == "bold red")
        let font = textView.textStorage?.attribute(.font, at: 0, effectiveRange: nil) as? NSFont
        #expect(font?.pointSize == 16)
        #expect(font?.fontDescriptor.symbolicTraits.contains(.bold) == false)
    }

    @Test
    func smartSubstitutionsAreOff() {
        #expect(!textView.isRichText)
        #expect(!textView.importsGraphics)
        #expect(!textView.isAutomaticQuoteSubstitutionEnabled)
        #expect(!textView.isAutomaticDashSubstitutionEnabled)
        #expect(!textView.isAutomaticTextReplacementEnabled)
        #expect(!textView.isAutomaticSpellingCorrectionEnabled)
        #expect(textView.isContinuousSpellCheckingEnabled)
    }

    @Test
    func theComposerGrowsToHalfTheAreaThenScrolls() {
        #expect(textView.fittingHeight(maxHeight: 300) == 56)
        type("one\ntwo\nthree")
        #expect(textView.fittingHeight(maxHeight: 300) == 100)
        type(String(repeating: "\nline", count: 60))
        #expect(textView.fittingHeight(maxHeight: 300) == 300)

        scrollView.frame.size.height = 300
        scrollView.layoutSubtreeIfNeeded()
        #expect(textView.frame.height > scrollView.contentView.bounds.height)
    }

    @Test
    func theHeightFollowsTheWidth() async {
        var text = "one\ntwo\nthree"
        var height: CGFloat = 56
        let field = ComposerField(
            text: Binding(get: { text }, set: { text = $0 }), enabled: true, maxHeight: 300,
            height: Binding(get: { height }, set: { height = $0 }), onSubmit: {}, onEscape: {})
        let coordinator = field.makeCoordinator()
        let (scroll, view) = ComposerTextView.scrollable()
        coordinator.attach(view)
        view.string = text
        scroll.frame = NSRect(x: 0, y: 0, width: 0, height: 56)

        coordinator.measure()
        await Task.yield()
        #expect(height == 56)

        scroll.frame.size.width = 400
        scroll.layoutSubtreeIfNeeded()
        for _ in 0..<3 {
            await Task.yield()
        }
        #expect(height == 100)
    }

    func field() -> (ComposerField.Coordinator, ComposerTextView) {
        var current = ""
        let field = ComposerField(
            text: Binding(get: { current }, set: { current = $0 }), enabled: true,
            maxHeight: 300, height: .constant(56), onSubmit: {}, onEscape: {})
        let coordinator = field.makeCoordinator()
        let (scroll, view) = ComposerTextView.scrollable()
        scroll.frame = NSRect(x: 0, y: 0, width: 400, height: 56)
        window.contentView?.addSubview(scroll)
        coordinator.attach(view)
        return (coordinator, view)
    }

    // Typing groups undo by event; a turn of the run loop closes the group.
    func typeAndSettle(_ text: String, into view: ComposerTextView) {
        window.makeFirstResponder(view)
        view.insertText(text, replacementRange: view.selectedRange())
        RunLoop.current.run(until: Date() + 0.05)
    }

    @Test
    func sendingLeavesNothingToUndo() {
        let (coordinator, view) = field()
        typeAndSettle("hello", into: view)
        #expect(view.undoManager?.canUndo == true)

        coordinator.replaceText(with: "")

        #expect(view.undoManager?.canUndo == false)
        view.undoManager?.undo()
        #expect(view.string == "")
    }

    @Test
    func eachChannelHasItsOwnUndo() {
        let (first, firstView) = field()
        let (second, secondView) = field()
        typeAndSettle("abc", into: firstView)
        typeAndSettle("x", into: secondView)

        #expect(firstView.undoManager !== secondView.undoManager)
        secondView.undoManager?.undo()

        #expect(secondView.string == "")
        #expect(firstView.string == "abc")
        _ = (first, second)
    }
}
