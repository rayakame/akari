import AkariKit
import AppKit
import Testing

@testable import Akari

// "(edited)" in its three places with the edit's date as its tooltip, a saved edit dimmed until
// Discord takes it, and a delete dimmed until Discord answers.
@MainActor
struct EditedMessageTests {
    let times = TimestampText(
        locale: Locale(identifier: "en_US"), timeZone: TimeZone(identifier: "Europe/Berlin")!,
        calendar: Calendar(identifier: .gregorian))
    let now = Date(timeIntervalSince1970: 1_800_000_000)
    let edit = Date(timeIntervalSince1970: 1_800_000_000 - 3600)

    func built(_ content: String) -> NSAttributedString {
        let message = row(1, at: now - 7200, content: content, edited: edit).message
        return MessageText.build(message, revealed: [], now: now, times: times).text
    }

    func mark(in text: NSAttributedString) throws -> (range: NSRange, attributes: [NSAttributedString.Key: Any]) {
        let range = (text.string as NSString).range(of: "(edited)")
        try #require(range.location != NSNotFound, "(edited) in \(text.string)")
        return (range, text.attributes(at: range.location, effectiveRange: nil))
    }

    var editTip: String {
        times.fullDate(Int64(edit.timeIntervalSince1970))
    }

    @Test
    func afterTheTextItIsTheOfficialTenPointsWithTheEditsDate() throws {
        let text = built("hello")
        let (range, attributes) = try mark(in: text)

        #expect(text.string == "hello (edited)")
        #expect((attributes[.font] as? NSFont)?.pointSize == 10)
        #expect(attributes[.foregroundColor] as? NSColor == Palette.chatTextMuted)
        #expect(attributes[.toolTip] as? String == editTip)
        #expect(editTip.hasPrefix("Friday, January 15, 2027 at 8:00"), "the edit's full date")
        #expect(text.attribute(.toolTip, at: range.location - 1, effectiveRange: nil) == nil, "not the space")
    }

    @Test
    func afterACodeBlockItTakesALineOfItsOwnWithTheTooltip() throws {
        let text = built("```\ncode\n```")

        #expect(text.string == "code\n(edited)")
        #expect(try mark(in: text).attributes[.toolTip] as? String == editTip)
    }

    // On the same baseline as the text beside it, as the official tag inside the text's line.
    @Test
    func itSitsOnTheTextsBaseline() throws {
        let text = built("hello")
        let body = text.attributes(at: 0, effectiveRange: nil)
        let mark = try mark(in: text).attributes

        #expect(mark[.baselineOffset] as? CGFloat == body[.baselineOffset] as? CGFloat)
    }

    @Test
    func aMessageWithoutTextHasItAfterItsStickersAndBeforeItsReactions() throws {
        let list = MessageListFixture()
        var shown = row(
            1, at: list.noon, content: "", stickers: ["wave"], edited: list.noon + 60)
        shown = MessageListModel.Row(
            id: shown.id, message: shown.message,
            reactions: ReactionRow(
                pills: [ReactionPill(emoji: .unicode("👍"), normal: 1, burst: 0, mine: false)],
                adds: true, toggles: true))
        list.show([shown])
        let cell = try #require(list.table.view(atColumn: 0, row: list.index(of: 1), makeIfNecessary: true) as? MessageCell)
        cell.layoutSubtreeIfNeeded()

        let texts = textViews(in: cell).filter { !$0.isHiddenOrHasHiddenAncestor }
        let extras = try #require(texts.first { $0.string.contains("(edited)") })
        let stickers = try #require(descendants(of: cell).first { $0 is StickerRow })
        let reactions = try #require(cell.reactions)
        func top(_ view: NSView) -> CGFloat { cell.convert(view.bounds, from: view).maxY }
        #expect(top(extras) < top(stickers), "below the stickers")
        #expect(top(reactions) < top(extras), "above the reactions")
        #expect(extras.string == "(edited)", "without the space it has after a text")
        let storage = try #require(extras.textStorage)
        let tip = storage.attribute(
            .toolTip, at: (storage.string as NSString).range(of: "(edited)").location, effectiveRange: nil)
        #expect(tip as? String == MessageFormat.fullDate(list.noon + 60, calendar: berlin, locale: .current))
    }

    @Test
    func aThreadsBoxComesAfterTheEditedMarkOfAMessageWithoutText() throws {
        let list = MessageListFixture()
        let base = row(1, at: list.noon, content: "", stickers: ["wave"], edited: list.noon + 60)
        let shown = MessageListModel.Row(
            id: base.id, message: base.message,
            threadBox: ThreadBox(
                threadId: ChannelId(rawValue: 20), name: "t1", link: "2 Messages ›",
                bottom: .noRecentMessages, time: list.noon))
        list.show([shown])
        let cell = try #require(list.table.view(atColumn: 0, row: list.index(of: 1), makeIfNecessary: true) as? MessageCell)
        cell.layoutSubtreeIfNeeded()

        let extras = try #require(textViews(in: cell).first { !$0.isHiddenOrHasHiddenAncestor && $0.string.contains("(edited)") })
        let box = try #require(ThreadBoxViewTests.boxView(in: cell))
        func top(_ view: NSView) -> CGFloat { cell.convert(view.bounds, from: view).maxY }
        #expect(top(box) < top(extras), "the box under \"(edited)\"")
    }

    func changed(_ row: MessageListModel.Row, _ change: (inout Message) -> Void) -> MessageListModel.Row {
        var message = row.message
        change(&message)
        return MessageListModel.Row(id: row.id, message: message)
    }

    @Test
    func aSavedEditIsDimmedLikeAMessageBeingSent() throws {
        let pending = changed(row(1, at: now, content: "saved")) { $0.edit = .pending }
        #expect(MessageText.Source(pending.message).color == Palette.textMuted)

        let failed = changed(row(2, at: now, content: "Discord's")) {
            $0.edit = .failed(content: "saved", error: .Forbidden)
        }
        #expect(MessageText.Source(failed.message).color == Palette.textDefault)
    }

    // Discord's answer may change nothing but the edit's state.
    @Test
    func aSavedEditsDimLiftsWhenOnlyItsEditChanges() throws {
        let list = MessageListFixture()
        let saved = changed(row(1, at: list.noon, content: "saved")) { $0.edit = .pending }
        func color() throws -> NSColor? {
            try list.text(of: 1).textStorage?.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor
        }
        list.show([saved])
        #expect(try color() === Palette.textMuted)

        list.show([changed(saved) { $0.edit = nil }])

        #expect(try color() === Palette.textDefault)
    }

    @Test
    func aMessageBeingDeletedIsDimmedAndComesBackOnARefusal() throws {
        let list = MessageListFixture()
        let going = changed(row(1, at: list.noon, content: "going")) { $0.deleting = true }
        list.show([going, row(2, at: list.noon + 10)])
        func cell(_ raw: UInt64) throws -> MessageCell {
            try #require(list.table.view(atColumn: 0, row: list.index(of: raw), makeIfNecessary: true) as? MessageCell)
        }

        let dimmed = try cell(1)
        #expect(dimmed.subviews.contains { $0.alphaValue == MessageCell.deletingAlpha })
        let avatar = try #require(dimmed.subviews.first { $0 is ImageLayerView })
        #expect(avatar.alphaValue == MessageCell.deletingAlpha, "the avatar too")
        #expect(try cell(2).subviews.allSatisfy { $0.alphaValue == 1 })

        list.show([changed(going) { $0.deleting = false }, row(2, at: list.noon + 10)])
        #expect(try cell(1).subviews.allSatisfy { $0.alphaValue == 1 })
    }

    func descendants(of view: NSView) -> [NSView] {
        view.subviews + view.subviews.flatMap(descendants)
    }

    func textViews(in view: NSView) -> [MessageTextView] {
        descendants(of: view).compactMap { $0 as? MessageTextView }
    }
}
