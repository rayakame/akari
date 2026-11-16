@testable import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// Pin's and Unpin's questions: the official words, the message in its box, Unpin's protip and
// the colors of the yes.
@MainActor
struct PinQuestionTests {
    let message = row(1, at: berlinTime(2026, 10, 10, 12, 0), content: "The release is on Friday.")

    func shown(pins: Bool) -> (MessageQuestionView, NSWindow) {
        let question = MessageActionsModel.PinQuestion(row: message, pins: pins, channelName: "new") {}
        let view = MessageQuestionView()
        let height = view.height(for: question, width: 440, maxHeight: 800)
        view.show(question, images: nil, maxHeight: 800)
        let window = TestWindow.make(size: NSSize(width: 440, height: height))
        view.frame = NSRect(x: 0, y: 0, width: 440, height: height)
        window.contentView?.addSubview(view)
        window.layoutIfNeeded()
        view.layoutSubtreeIfNeeded()
        return (view, window)
    }

    func weight(_ text: NSAttributedString, at index: Int) -> CGFloat {
        let font = text.attribute(.font, at: index, effectiveRange: nil) as? NSFont
        let traits = font?.fontDescriptor.object(forKey: .traits) as? [NSFontDescriptor.TraitKey: Any]
        return traits?[.weight] as? CGFloat ?? 0
    }

    // The alerts' yes: a removal's is red, "The Pin Is Stuck!"'s Try Again isn't.
    @Test(arguments: [true, false])
    func anAlertsYesIsRedOnlyWhereTheQuestionSaysSo(destructive: Bool) {
        let question = MessageActionsModel.Confirmation(
            title: "The Pin Is Stuck!", message: "", confirm: "Try Again", cancel: "Cancel",
            destructive: destructive, run: {})

        #expect(question.confirmRole == (destructive ? ButtonRole.destructive : nil))
    }

    @Test
    func pinAsksWithTheChannelsNameAndABrandYesWithoutATip() {
        let (view, _) = shown(pins: true)

        #expect(view.title.stringValue == "Pin It. Pin It Good.")
        #expect(
            view.message.stringValue
                == "Hey, just double-checking that you want to pin this message to #new for "
                + "posterity and greatness?")
        #expect(view.tip.isHidden)
        #expect([view.cancel.title, view.confirm.title] == ["Cancel", "Oh yeah. Pin it"])
        #expect(view.confirm.kind == .primary)
        #expect(!view.confirm.hasDestructiveAction)
        #expect(view.accessibilityLabel() == "Pin It. Pin It Good.")
        #expect(view.box.frame.minY == view.message.frame.maxY + 20)
    }

    @Test
    func unpinAsksWithTheProtipAndARedYes() throws {
        let (view, _) = shown(pins: false)
        let tip = view.tip.attributedStringValue
        let label = (tip.string as NSString).range(of: "Protip:")
        let bold = (tip.string as NSString).range(of: "unpin message")

        #expect(view.title.stringValue == "Unpin Message")
        #expect(view.message.stringValue == "You sure you want to remove this pinned message?")
        #expect(!view.tip.isHidden)
        #expect(
            tip.string
                == "Protip: You can hold down shift when clicking unpin message to bypass this "
                + "confirmation entirely.")
        #expect(label.location == 0)
        #expect(tip.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor == Palette.statusPositive)
        #expect(weight(tip, at: 0) > weight(tip, at: label.length + 2))
        #expect(weight(tip, at: bold.location) > weight(tip, at: label.length + 2))
        #expect(weight(tip, at: NSMaxRange(bold) - 1) > weight(tip, at: label.length + 2))
        #expect(weight(tip, at: bold.location - 1) == weight(tip, at: label.length + 2))
        #expect(
            tip.attribute(.foregroundColor, at: bold.location, effectiveRange: nil) as? NSColor
                == Palette.textMuted)
        #expect([view.cancel.title, view.confirm.title] == ["Cancel", "Remove it please!"])
        #expect(view.confirm.kind == .critical)
        #expect(view.confirm.hasDestructiveAction)
        #expect(view.tip.frame.minY == view.box.frame.maxY + 20)
    }

    func fill(of button: PanelButton) -> NSColor? {
        button.updateLayer()
        return button.layer?.backgroundColor.flatMap { NSColor(cgColor: $0) }?.usingColorSpace(.sRGB)
    }

    @Test
    func pinsYesIsTheBrandsAndUnpinsRed() {
        let (pin, _) = shown(pins: true)
        let (unpin, _) = shown(pins: false)

        #expect(same(fill(of: pin.confirm), Palette.brand.usingColorSpace(.sRGB)))
        let red = fill(of: unpin.confirm)
        #expect((red?.redComponent ?? 0) > 0.8 && (red?.greenComponent ?? 1) < 0.35)
        #expect(pin.confirm.attributedTitle.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor == .white)
    }

    // As the window draws them, from the button's layer.
    func drawn(_ button: PanelButton) -> CGColor? {
        button.displayIfNeeded()
        return button.layer?.backgroundColor
    }

    func resolved(_ color: NSColor, in button: NSView) -> CGColor? {
        var cg: CGColor?
        button.effectiveAppearance.performAsCurrentDrawingAppearance { cg = color.cgColor }
        return cg
    }

    // A view that asked Unpin draws Pin's yes in the brand's color, which the pointer lightens.
    @Test
    func pinsYesIsDrawnInTheBrandsColorAndLightensUnderThePointer() throws {
        let (view, _) = shown(pins: false)
        #expect(drawn(view.confirm) == resolved(Palette.danger, in: view.confirm))

        view.show(
            MessageActionsModel.PinQuestion(row: message, pins: true, channelName: "new") {}, images: nil,
            maxHeight: 800)
        #expect(drawn(view.confirm) == resolved(Palette.brand, in: view.confirm))

        let event = try #require(
            NSEvent.mouseEvent(
                with: .mouseMoved, location: .zero, modifierFlags: [], timestamp: 0, windowNumber: 0,
                context: nil, eventNumber: 0, clickCount: 0, pressure: 0))
        view.confirm.mouseEntered(with: event)
        #expect(drawn(view.confirm) == resolved(Palette.brandHover, in: view.confirm))
        view.confirm.mouseExited(with: event)
        #expect(drawn(view.confirm) == resolved(Palette.brand, in: view.confirm))
    }

    // One view shows one question after another, as SwiftUI keeps it.
    @Test
    func aViewShowsTheNextQuestionsWords() {
        let (view, _) = shown(pins: false)

        view.show(MessageActionsModel.DeleteQuestion(row: message, showsTip: false) {}, images: nil, maxHeight: 800)

        #expect(view.title.stringValue == "Delete Message")
        #expect(view.tip.isHidden)
        #expect(view.confirm.title == "Delete")
        #expect(view.confirm.kind == .critical)
    }
}

// Pin and Unpin from the list: the Shift-held bar's button and the menu's item, as the model
// lists them, reach the list's actions with Shift as it was at the click.
@MainActor
final class PinActionsInTheListTests {
    nonisolated final class PinStore: Store, @unchecked Sendable {
        init() {
            super.init(noHandle: NoHandle())
        }

        required init(unsafeFromHandle handle: UInt64) {
            fatalError("a fake has no Rust object")
        }

        // 1 isn't pinned, 2 is.
        override func messageActions(channelId: ChannelId, messageId: MessageId) -> MessageActions {
            testMessageActions(pin: messageId.rawValue == 1, unpin: messageId.rawValue == 2)
        }

        override func channel(id: ChannelId) -> Channel? {
            Channel(
                id: id, kind: .guildText, guildId: GuildId(rawValue: 1), parentId: nil, name: "new",
                position: 0, topic: nil, nsfw: false, rateLimitPerUser: 0, recipientIds: [], thread: nil)
        }
    }

    let list = BarList()

    init() {
        list.actions.model = MessageActionsModel(
            store: PinStore(), account: GuildAccount(), copy: { _ in }, retry: { _ in },
            discard: { _ in }, createThread: { _, _ in }, viewThread: { _, _ in }, failed: { _ in })
        list.show(
            [
                row(1, by: author(2, "Ray"), at: list.noon, content: "first"),
                row(2, by: author(2, "Ray"), at: list.noon + 600, content: "pinned"),
            ], bars: false)
    }

    @Test
    func shiftHeldTheBarHasPinAfterMarkUnreadWithTheOfficialWords() throws {
        list.shift = true
        try list.hover(1)

        #expect(list.bar.items.map(\.action) == [.copyLink, .markUnread, .pin, .more])
        let pin = try #require(list.bar.button(.pin))
        #expect(pin.toolTip == "Pin Message")

        list.shift = false
        pin.performClick(nil)

        #expect(list.actions.calls.last == "pin 1")
        #expect(list.actions.surfaces.last == .bar)
    }

    @Test
    func aPinnedMessagesMenuHasUnpinAfterCopyText() throws {
        let cell = try list.cell(of: 2)
        let point = cell.convert(NSPoint(x: cell.box.minX + 20, y: cell.box.midY), to: nil)
        let click = try #require(
            NSEvent.mouseEvent(
                with: .rightMouseDown, location: point, modifierFlags: [], timestamp: 0,
                windowNumber: list.window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                pressure: 1))
        let menu = try #require(cell.menu(for: click))

        #expect(menu.items.map(\.title) == ["Copy Text", "Unpin Message", "Mark Unread", "Copy Message Link"])
        menu.performActionForItem(at: 1)

        #expect(list.actions.calls.last == "unpin 2")
        #expect(list.actions.surfaces.last == .menu)
    }
}
