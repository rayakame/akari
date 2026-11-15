import AkariKit
import AppKit
import Testing

@testable import Akari

// The delete question: the official words, the message in its box, the Shift tip from the menu,
// Cancel and Delete; and the preview that shows the message outside the list.
@MainActor
struct DeleteMessageTests {
    let noon = berlinTime(2026, 10, 10, 12, 0)

    func question(
        _ row: MessageListModel.Row, tip: Bool, run: @escaping @MainActor () -> Void = {}
    ) -> MessageActionsModel.DeleteQuestion {
        MessageActionsModel.DeleteQuestion(row: row, showsTip: tip, run: run)
    }

    func shown(_ view: NSView, width: CGFloat = 440, height: CGFloat = 600) -> NSWindow {
        let window = TestWindow.make(size: NSSize(width: width, height: height))
        view.frame = NSRect(x: 0, y: 0, width: width, height: height)
        window.contentView?.addSubview(view)
        window.layoutIfNeeded()
        view.layoutSubtreeIfNeeded()
        return window
    }

    func descendants(of view: NSView) -> [NSView] {
        view.subviews + view.subviews.flatMap(descendants)
    }

    @Test
    func aPreviewOutsideTheListTakesNoPointerAndHasNoMenu() throws {
        let box = MessagePreviewBox()
        let message = row(1, at: noon, content: "goodbye")
        box.show(message, images: nil)
        let window = shown(box, height: MessagePreviewBox.height(of: message, width: 440))
        let text = try #require(descendants(of: box).compactMap { $0 as? MessageTextView }.first { !$0.isHidden })
        let point = text.convert(NSPoint(x: 5, y: text.bounds.midY), to: nil)

        #expect(text.string == "goodbye")
        #expect(window.contentView?.hitTest(point) !== text)
        #expect(box.preview.hitTest(box.preview.convert(point, from: nil)) == nil)
        #expect(box.preview.cursor(at: point) == nil)
        #expect(box.preview.cell.menuItems().isEmpty)
    }

    @Test
    func theThreadFormsPreviewKeepsItsTextSelectable() throws {
        let preview = MessagePreviewView()
        MessagePreview.configure(preview.cell, with: row(1, at: noon, content: "hello").message, now: noon)
        _ = shown(preview, height: 80)
        let text = try #require(descendants(of: preview).compactMap { $0 as? MessageTextView }.first { !$0.isHidden })
        let point = text.convert(NSPoint(x: 5, y: text.bounds.midY), to: preview.superview)

        #expect(preview.hitTest(point) === text)
        #expect(!preview.cell.menuItems().isEmpty)
    }

    // 10 above and below the row, which keeps its own insets (the text at 72) and has no group space.
    @Test
    func theBoxHoldsTheRowTenInWithItsInsets() throws {
        let box = MessagePreviewBox()
        let message = row(1, at: noon, content: "goodbye")
        box.show(message, images: nil)
        let height = MessagePreviewBox.height(of: message, width: 440)
        _ = shown(box, height: height)

        #expect(height == 20 + MessagePreview.height(of: message.message, spaced: false, width: 440))
        #expect(
            height == 20 + MessagePreview.height(of: message.message, width: 440) - 16,
            "no space before the group inside the box")
        let frame = box.convert(box.preview.bounds, from: box.preview)
        #expect(frame.minY == 10)
        #expect(frame.height == height - 20)
        let text = try #require(descendants(of: box).compactMap { $0 as? MessageTextView }.first { !$0.isHidden })
        #expect(box.convert(text.bounds, from: text).minX == 72)
    }

    @Test
    func aTallMessageScrollsInsideTheBox() throws {
        let long = row(1, at: noon, content: String(repeating: "a long message that wraps ", count: 120))
        let view = DeleteMessageView()
        let question = question(long, tip: false)
        let height = view.height(for: question, width: 440, maxHeight: 400)
        view.show(question, images: nil, maxHeight: 400)
        _ = shown(view, height: height)

        #expect(height <= 400)
        #expect(view.box.frame.height < MessagePreviewBox.height(of: long, width: 392))
        #expect(view.box.frame.height >= DeleteLayout.minimumBox)
        let scroll = try #require(view.box.subviews.first as? NSScrollView)
        #expect(try #require(scroll.documentView).frame.height > scroll.contentSize.height)
    }

    @Test
    func theQuestionHasTheOfficialWordsAndTheTipOnlyFromTheMenu() throws {
        let message = row(1, at: noon, content: "goodbye")
        let view = DeleteMessageView()
        let withTip = view.height(for: question(message, tip: true), width: 440, maxHeight: 800)
        let without = view.height(for: question(message, tip: false), width: 440, maxHeight: 800)
        view.show(question(message, tip: true), images: nil, maxHeight: 800)
        _ = shown(view, height: withTip)

        #expect(view.title.stringValue == "Delete Message")
        #expect(view.message.stringValue == "Are you sure you want to delete this message?")
        #expect(!view.tip.isHidden)
        #expect(
            view.tip.stringValue
                == "You can hold down shift when clicking delete message to bypass this confirmation entirely.")
        let tip = view.tip.attributedStringValue
        let bold = (tip.string as NSString).range(of: "delete message")
        let weight = { (index: Int) in
            ((tip.attribute(.font, at: index, effectiveRange: nil) as? NSFont)?.fontDescriptor
                .object(forKey: .traits) as? [NSFontDescriptor.TraitKey: Any])?[.weight] as? CGFloat ?? 0
        }
        #expect(weight(bold.location) > weight(0), "\"delete message\" in bold")
        #expect(without < withTip)
        #expect([view.cancel.title, view.delete.title] == ["Cancel", "Delete"])
        #expect(view.box.frame.minY == view.message.frame.maxY + 20)
        #expect(view.tip.frame.minY == view.box.frame.maxY + 20)
    }

    @Test
    func deleteAnswersYesReturnDeletesAndCancelAndEscapeDont() throws {
        let view = DeleteMessageView()
        var answers: [Bool] = []
        view.answer = { answers.append($0) }

        view.delete.performClick(nil)
        view.cancel.performClick(nil)

        #expect(answers == [true, false])
        #expect(view.delete.keyEquivalent == "\r")
        #expect(view.cancel.keyEquivalent == "\u{1b}")
        #expect(view.delete.hasDestructiveAction)
    }

    @Test
    func thePanelIsCenteredAndAsTallAsItsContent() {
        let question = question(row(1, at: noon, content: "goodbye"), tip: true)

        let frame = DeleteLayout.frame(for: question, in: CGSize(width: 1200, height: 800))

        #expect(frame.width == 440)
        #expect(frame.midX == 600)
        #expect(abs(frame.midY - 400) <= 0.5)
        #expect(frame.height == DeleteMessageView.sizing.height(for: question, width: 440, maxHeight: 720))
        #expect(DeleteLayout.frame(for: question, in: CGSize(width: 400, height: 800)).width == 320)
    }

    @Test
    func theBarsDeleteSaysItCameFromTheBarAndTheMenusFromTheMenu() throws {
        let list = BarList()
        list.actions.bars[1] = ([.more], [.copyLink, .delete])
        list.show([row(1, at: list.noon, content: "first")], bars: false)
        list.shift = true
        try list.hover(1)
        #expect(list.bar.items.map(\.action) == [.copyLink, .delete])

        try #require(list.bar.button(.delete)).performClick(nil)
        #expect(list.actions.calls.last == "delete 1 shift")
        #expect(list.actions.surfaces.last == .bar)

        list.shift = false
        list.actions.marksUnread = [1]
        let cell = try list.cell(of: 1)
        let point = cell.convert(NSPoint(x: cell.box.minX + 20, y: cell.box.midY), to: nil)
        let click = try #require(
            NSEvent.mouseEvent(
                with: .rightMouseDown, location: point, modifierFlags: [], timestamp: 0,
                windowNumber: list.window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                pressure: 1))
        let menu = try #require(cell.menu(for: click))
        menu.performActionForItem(at: 0)
        #expect(list.actions.surfaces.last == .menu)
    }
}
