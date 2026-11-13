import AkariKit
import AppKit
import Testing

@testable import Akari

/// A loaded reply target, its text parsed as the core parses a preview's.
func target(
    by author: User = author(2, "Ray"), _ content: String = "the original message",
    kind: MessageType = .default, edited: Bool = false, attachments: Bool = false,
    sticker: Bool = false
) -> ReplyTarget {
    .loaded(
        preview: ReplyPreview(
            kind: kind, author: author, fromWebhook: false, content: content, edited: edited,
            voiceMessage: false, hasAttachments: attachments, hasEmbeds: false, hasSticker: sticker,
            forwarded: false,
            formatted: content.isEmpty
                ? nil : RichText(text: content, formatted: formattedText(content: content))))
}

/// A reply to message `to` of its channel, with the line the list's model would give its row.
func replyRow(
    _ raw: UInt64, by user: User = author(1), at time: Date, content: String? = nil,
    to answered: UInt64 = 1, _ target: ReplyTarget, mentions: Bool = false,
    kind: MessageType = .reply, delivery: Delivery = .sent, mentionsMe: Bool = false,
    refused: Bool = false
) -> MessageListModel.Row {
    let base = row(raw, by: user, at: time, kind: kind, content: content, delivery: delivery)
    var message = base.message
    message.repliedTo = RepliedTo(
        channelId: message.channelId, messageId: MessageId(rawValue: answered),
        mentionsAuthor: mentions)
    message.mentionsMe = mentionsMe
    var name: String?
    if case .loaded(let preview) = target {
        name = preview.author.displayName
    }
    return MessageListModel.Row(
        id: base.id, message: message, authorName: base.authorName,
        reply: ReplyLine(
            channelId: message.channelId, messageId: MessageId(rawValue: answered), target: target,
            mentionsAuthor: mentions, authorName: name),
        replyRefused: refused)
}

// The line over a reply: the official client's measures (its stylesheet, canary 671549), from
// the row's left edge and the line's top.
@MainActor
struct ReplyLineViewTests {
    func list(scale: CGFloat = 1, _ rows: (BarList) -> [MessageListModel.Row]) -> BarList {
        let list = BarList(scale: scale)
        list.show(rows(list))
        return list
    }

    func cell(_ raw: UInt64, in list: BarList) throws -> MessageCell {
        try #require(try list.cell(of: raw) as? MessageCell)
    }

    // A view's frame in its cell, from the cell's top left.
    func frame(of view: NSView, in cell: MessageCell) -> NSRect {
        let rect = view.convert(view.bounds, to: cell)
        return cell.isFlipped
            ? rect : NSRect(x: rect.minX, y: cell.bounds.height - rect.maxY, width: rect.width, height: rect.height)
    }

    func boxTop(_ cell: MessageCell) -> CGFloat {
        cell.bounds.height - cell.box.height
    }

    @Test(arguments: [1, 2] as [CGFloat])
    func aLoadedTargetShowsItsAvatarNameAndText(scale: CGFloat) throws {
        let list = list(scale: scale) { [replyRow(5, at: $0.noon, target(), mentions: true)] }
        let cell = try cell(5, in: list)
        let line = frame(of: cell.replyLine, in: cell)

        #expect(line.minX == 72)
        #expect(line.minY == boxTop(cell) + 4)
        #expect(line.height == 18)
        #expect(frame(of: cell.replyLine.avatar, in: cell) == NSRect(x: 72, y: line.minY + 1, width: 16, height: 16))
        #expect(!cell.replyLine.avatar.isHidden && !cell.replyLine.showsBadge)
        #expect(cell.replyLine.avatar.cornerRadius == 8)

        #expect(cell.replyLine.shownName == "@Ray")
        let name = try #require(cell.replyLine.nameFrame).offsetBy(dx: line.minX, dy: line.minY)
        #expect(name.minX == 92, "4 after the avatar")
        let content = frame(of: cell.replyLine.content, in: cell)
        #expect(content.minX == name.maxX + 4)
        #expect(content.minY == line.minY && content.height == 18)
        #expect(cell.replyLine.content.shownText.string == "the original message")
        #expect(cell.replyLine.content.isEnabled)

        // The header and the avatar start 4 under the line.
        let avatar = try #require(cell.avatarFrame)
        #expect(avatar.minY == line.maxY + 4)
        #expect(avatar.minX == 16 && avatar.width == 40)
        for rect in [line, content, avatar] {
            #expect((rect.minX * scale).rounded() == rect.minX * scale)
            #expect((rect.minY * scale).rounded() == rect.minY * scale)
        }
    }

    @Test
    func theNameIsMediumAt14AndTheTextRegularAt14() throws {
        let list = list { [replyRow(5, at: $0.noon, target())] }
        let cell = try cell(5, in: list)

        #expect(ReplyLineView.nameFont.pointSize == TypeScale.size(14))
        #expect(NSFontManager.shared.weight(of: ReplyLineView.nameFont) > NSFontManager.shared.weight(of: ReplyLineView.font))
        let text = cell.replyLine.content.shownText
        let font = try #require(text.attribute(.font, at: 0, effectiveRange: nil) as? NSFont)
        #expect(font.pointSize == TypeScale.size(14))
        #expect(text.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor === Palette.textDefault)
    }

    @Test(arguments: [1, 2] as [CGFloat])
    func theSpineRunsFromTheAvatarsMiddleIntoTheLine(scale: CGFloat) throws {
        let list = list(scale: scale) { [replyRow(5, at: $0.noon, target())] }
        let cell = try cell(5, in: list)
        let line = frame(of: cell.replyLine, in: cell)
        let spine = frame(of: cell.spine, in: cell)
        let avatar = try #require(cell.avatarFrame)

        #expect(spine == NSRect(x: 30, y: line.minY, width: 38, height: 18))
        #expect(!cell.spine.isHidden && cell.spine.isEnabled)
        #expect(cell.spine.shape.lineWidth == 2)
        let path = try #require(cell.spine.shape.path)
        let box = path.boundingBoxOfPath.offsetBy(dx: spine.minX, dy: spine.minY)
        // The stroke's middle: up from the avatar's center line, along the line's middle.
        #expect(box.minX == avatar.midX)
        #expect(box.minY == line.midY)
        #expect(box.maxX == 68, "4 before the small avatar")
        #expect(box.maxY == line.maxY)
        #expect(avatar.minY - box.maxY == 4)
        // The corner: a 6 outer radius is 5 on the stroke's middle.
        #expect(!path.contains(CGPoint(x: 6, y: 9)))
        var points: [CGPoint] = []
        path.applyWithBlock { element in
            points.append(element.pointee.points[0])
        }
        #expect(points.first == CGPoint(x: 6, y: 18))
        #expect(points.contains(CGPoint(x: 6, y: 14)))
        #expect(points.last == CGPoint(x: 38, y: 9))
    }

    @Test
    func aDeletedTargetHasTheBadgeItsWordsAndNoJump() throws {
        let list = list { [replyRow(5, at: $0.noon, .deleted)] }
        let cell = try cell(5, in: list)
        let line = frame(of: cell.replyLine, in: cell)

        #expect(cell.replyLine.showsBadge && cell.replyLine.avatar.isHidden)
        #expect(cell.replyLine.shownName == nil)
        #expect(frame(of: cell.replyLine.content, in: cell).minX == line.minX + 20)
        let text = cell.replyLine.content.shownText
        #expect(text.string == "Original message was deleted")
        let font = try #require(text.attribute(.font, at: 0, effectiveRange: nil) as? NSFont)
        #expect(font.fontDescriptor.symbolicTraits.contains(.italic))
        #expect(!cell.replyLine.content.isEnabled)
        #expect(!cell.spine.isEnabled && !cell.spine.isHidden)
    }

    @Test
    func aTargetThatIsNotLoadedHasTheBadgeItsWordsAndJumps() throws {
        let list = list { [replyRow(5, at: $0.noon, .notLoaded)] }
        let cell = try cell(5, in: list)

        #expect(cell.replyLine.showsBadge)
        #expect(cell.replyLine.content.shownText.string == "Message could not be loaded")
        #expect(cell.replyLine.content.isEnabled && cell.spine.isEnabled)
    }

    @Test
    func aLongTextIsCutOnOneLineAndKeepsItsSymbol() throws {
        let long = String(repeating: "a long original message ", count: 12)
        let list = list { [replyRow(5, at: $0.noon, target(long, attachments: true))] }
        let cell = try cell(5, in: list)
        let line = frame(of: cell.replyLine, in: cell)
        let content = cell.replyLine.content

        #expect(line.maxX == cell.bounds.width - 16)
        #expect(frame(of: content, in: cell).maxX == line.maxX)
        #expect(content.fittingWidth > content.bounds.width, "the text is wider than its room")
        let symbol = try #require(content.trailingFrame)
        #expect(symbol.width == 16 && symbol.maxX == content.bounds.width)
        #expect(symbol.minX == content.textFrame.maxX + 4)
        #expect(cell.bounds.height == list.table.rect(ofRow: list.index(of: 5)).height)
    }

    @Test
    func aShortTextsSymbolFollowsIt() throws {
        let list = list { [replyRow(5, at: $0.noon, target("", sticker: true))] }
        let content = try cell(5, in: list).replyLine.content

        #expect(content.shownText.string == "Click to see sticker")
        #expect(content.symbols == (false, true))
        let symbol = try #require(content.trailingFrame)
        #expect(symbol.minX == content.textFrame.maxX + 4)
        #expect(content.bounds.width == content.fittingWidth)
    }

    @Test
    func anEditedTargetEndsWithTheMark() throws {
        let list = list { [replyRow(5, at: $0.noon, target("fixed", edited: true))] }

        #expect(try cell(5, in: list).replyLine.content.shownText.string == "fixed (edited)")
    }

    @Test
    func aCustomEmojiIsAnImageLayerAtTheLinesSize() throws {
        let list = list { [replyRow(5, at: $0.noon, target("hi <:wave:5> there"))] }
        let content = try cell(5, in: list).replyLine.content
        content.line.layoutSubtreeIfNeeded()

        let emoji = try #require(content.line.emojiViews.first)
        #expect(content.line.emojiViews.count == 1)
        #expect(emoji.frame.size == NSSize(width: 18, height: 18))
        #expect(emoji.frame.minY == 0)
    }

    @Test
    func aHiddenSpoilerStaysCovered() throws {
        let list = list { [replyRow(5, at: $0.noon, target("it was ||the butler||"))] }
        let text = try cell(5, in: list).replyLine.content.shownText
        let index = (text.string as NSString).range(of: "butler").location

        #expect(text.attribute(.backgroundColor, at: index, effectiveRange: nil) as? NSColor === Palette.spoilerHidden)
    }

    // Every state is one line: a target that changes never changes its reply's height.
    @Test
    func theRowIsAsTallAsTheSizingPathSaysInEveryState() throws {
        let table = MessageTableTests()
        let targets: [ReplyTarget] = [
            target(), .deleted, .notLoaded, target("", sticker: true),
            target(String(repeating: "long ", count: 80)),
        ]
        table.show(
            [
                row(1, by: author(2, "Ray"), at: table.noon, content: "first"),
                row(2, by: author(3, "Kim"), at: table.noon + 5, content: "plain"),
            ]
                + targets.enumerated().map { index, target in
                    replyRow(UInt64(10 + index), at: table.noon + 10, content: "a reply", target)
                })

        let plain = table.table.rect(ofRow: table.index(of: 2)).height
        for raw in UInt64(10)..<UInt64(10 + targets.count) {
            let index = table.index(of: raw)
            let height = table.table.rect(ofRow: index).height
            #expect(height == table.fittingHeight(ofRow: index), "\(raw)")
            #expect(height == plain + 22, "the line and 4 under it")
        }
    }

    // The text decides the height of a long reply, the avatar that of one without text.
    @Test
    func aLongReplyAndOneWithoutTextAreAsTallAsTheSizingPathSays() throws {
        let table = MessageTableTests()
        let long = String(repeating: "a long answer ", count: 60)
        table.show([
            row(9, at: table.noon - 10, content: "first"),
            row(1, by: author(2, "Ray"), at: table.noon, content: long),
            replyRow(2, by: author(3, "Kim"), at: table.noon + 10, content: long, target()),
            row(3, by: author(2, "Ray"), at: table.noon + 20, content: ""),
            replyRow(4, by: author(3, "Kim"), at: table.noon + 30, content: "", target()),
        ])

        for (plain, reply) in [(1, 2), (3, 4)] as [(UInt64, UInt64)] {
            let index = table.index(of: reply)
            let height = table.table.rect(ofRow: index).height
            #expect(height == table.fittingHeight(ofRow: index), "\(reply)")
            #expect(height == table.table.rect(ofRow: table.index(of: plain)).height + 22, "\(reply)")
        }
        #expect(table.table.rect(ofRow: table.index(of: 1)).height > 100, "the text wraps")
    }

    @Test
    func aContextMenuCommandsResultShowsTheLineTheSameWay() throws {
        let list = list { [replyRow(5, at: $0.noon, target(), kind: .contextMenuCommand)] }
        let cell = try cell(5, in: list)

        #expect(!cell.replyLine.isHidden && !cell.spine.isHidden)
        #expect(cell.replyLine.shownName == "Ray")
    }

    @Test
    func aMessageThatAnswersNothingHasNoLine() throws {
        let list = list { [row(5, at: $0.noon, kind: .reply), replyRow(6, at: $0.noon + 600, target())] }

        let plain = try cell(5, in: list)
        #expect(plain.replyLine.isHidden && plain.spine.isHidden)
        #expect(!(try cell(6, in: list)).replyLine.isHidden)
    }

    // The split is narrower: the line still ends at the text column's edge.
    @Test
    func inANarrowListTheLineIsCutAtTheTextColumn() throws {
        let list = BarList()
        list.window.setContentSize(NSSize(width: 340, height: 400))
        list.window.layoutIfNeeded()
        list.show([replyRow(5, at: list.noon, target(String(repeating: "long ", count: 40)))])
        let cell = try cell(5, in: list)

        #expect(frame(of: cell.replyLine.content, in: cell).maxX == cell.bounds.width - 16)
        #expect(frame(of: cell.replyLine, in: cell).height == 18)
    }

    // A thread's first row shows its parent's message with that message's line.
    @Test
    func aStarterRowShowsItsOriginalsLine() throws {
        let list = BarList()
        let original = replyRow(2, by: author(2, "Ray"), at: list.noon, content: "the start", target(by: author(3, "Kim"), "before"))
        let starter = row(30, at: list.noon + 60, kind: .threadStarterMessage)
        list.show([
            MessageListModel.Row(
                id: starter.id, message: starter.message, starter: .loaded(original.message),
                reply: original.reply)
        ])
        let cell = try cell(30, in: list)

        #expect(!cell.replyLine.isHidden)
        #expect(cell.replyLine.shownName == "Kim")
    }

    @Test
    func theSmallAvatarNeverPlays() throws {
        let list = list { [replyRow(5, at: $0.noon, target())] }
        let cell = try cell(5, in: list)

        cell.animatesAvatar = true

        #expect(!cell.replyLine.avatar.animates)
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theColorsAreTheThemes(appearance: NSAppearance.Name) throws {
        let list = list { [replyRow(5, at: $0.noon, target())] }
        list.window.appearance = NSAppearance(named: appearance)
        let cell = try cell(5, in: list)

        var wanted: CGColor?
        cell.effectiveAppearance.performAsCurrentDrawingAppearance {
            wanted = Palette.spine.cgColor
        }
        #expect(cell.spine.shape.strokeColor == wanted)
        let light = appearance == .aqua
        let stroke = try #require(cell.spine.shape.strokeColor.flatMap(NSColor.init(cgColor:))?.usingColorSpace(.sRGB))
        #expect(abs(stroke.redComponent * 255 - (light ? 0xc5 : 0x47)) < 0.5)
        #expect(abs(stroke.blueComponent * 255 - (light ? 0xca : 0x50)) < 0.5)
    }

    // Only the refusal says so: a reply that failed otherwise may answer a message deleted since.
    @Test
    func aFailedReplyDiscordRefusedForItsDeletedMessageSaysSo() throws {
        let list = list {
            [
                replyRow(5, at: $0.noon, .deleted, delivery: .failed, refused: true),
                replyRow(6, at: $0.noon + 600, target(), delivery: .failed),
                replyRow(7, at: $0.noon + 1200, .deleted, delivery: .failed),
            ]
        }

        #expect(try cell(5, in: list).failed?.text == "Not sent: the message it replies to was deleted.")
        #expect(try cell(6, in: list).failed?.text == "Not sent.")
        #expect(try cell(7, in: list).failed?.text == "Not sent.")
    }

    // A reply always starts a group; a row under a header never shows a line.
    @Test
    func aRowThatContinuesAGroupShowsNoLine() throws {
        let reply = replyRow(5, at: berlinTime(2026, 10, 10, 12, 0), content: "an answer", target())
        let line = try #require(reply.reply)
        let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)
        let cell = MessageCell()

        cell.configure(
            reply.message, body: NSAttributedString(string: "an answer"), startsGroup: false,
            groupTime: "", shortTime: "", fullDate: "",
            reply: (line, ReplyLineView.text(of: line, now: Date(), times: times)))

        #expect(cell.replyLine.isHidden && cell.spine.isHidden)
    }

    @Test
    func voiceOverReadsTheLineAsAButtonThatJumps() throws {
        let list = list {
            [
                replyRow(5, at: $0.noon, content: "an answer", target()),
                replyRow(6, at: $0.noon + 600, content: "another", to: 9, .deleted),
            ]
        }
        let loaded = try cell(5, in: list).replyLine.content
        let deleted = try cell(6, in: list).replyLine.content

        #expect(loaded.accessibilityLabel() == "Replying to Ray: the original message")
        #expect(loaded.accessibilityRole() == .button)
        #expect(loaded.accessibilityHelp() == "Jump To Reply")
        #expect(deleted.accessibilityLabel() == "Original message was deleted")
        #expect(deleted.accessibilityRole() == .staticText)
        #expect(deleted.accessibilityHelp() == nil)
    }

    // The New Thread form shows its source as the list does; its line isn't a button there.
    @Test
    func theThreadFormsPreviewShowsTheLineWithoutAJump() throws {
        let source = replyRow(5, at: berlinTime(2026, 10, 10, 12, 0), content: "start here", target())
        let cell = MessageCell()

        MessagePreview.configure(cell, with: source.message, reply: source.reply, now: Date())

        #expect(!cell.replyLine.isHidden)
        #expect(cell.replyLine.shownName == "Ray")
        #expect(!cell.replyLine.content.isEnabled && !cell.spine.isEnabled)
        #expect(
            MessagePreview.height(of: source.message, reply: source.reply, width: 420)
                == MessagePreview.height(of: source.message, width: 420) + 22)
    }
}

// The line follows its target live and takes the pointer by the list's one authority.
@MainActor
struct ReplyLineBehaviorTests {
    let list = BarList()

    func cell(_ raw: UInt64) throws -> MessageCell {
        try #require(try list.cell(of: raw) as? MessageCell)
    }

    // The middle of a view of message `raw`'s cell, as a point of the table.
    func middle(of view: NSView) -> NSPoint {
        let rect = view.convert(view.bounds, to: list.table)
        return NSPoint(x: rect.midX, y: rect.midY)
    }

    func shown(_ target: ReplyTarget = target()) {
        list.show([
            row(1, by: author(2, "Ray"), at: list.noon, content: "the original message"),
            replyRow(5, at: list.noon + 600, content: "an answer", target),
        ])
    }

    @Test
    func thePointerOverTheTextLightsTheTextAndTheSpine() throws {
        shown()
        let cell = try cell(5)

        list.move(to: middle(of: cell.replyLine.content))

        #expect(cell.replyHovered && cell.spine.hovered && cell.replyLine.hovered)
        #expect(
            cell.replyLine.content.shownText.attribute(.foregroundColor, at: 0, effectiveRange: nil)
                as? NSColor === Palette.textStrong)
        #expect(list.controller.cursor(at: list.pointer.location ?? .zero) == .pointingHand)

        try list.hover(1)
        #expect(!cell.replyHovered && !cell.spine.hovered)
        #expect(
            cell.replyLine.content.shownText.attribute(.foregroundColor, at: 0, effectiveRange: nil)
                as? NSColor === Palette.textDefault)
    }

    @Test
    func thePointerOverTheSpineLightsBothToo() throws {
        shown()
        let cell = try cell(5)

        list.move(to: middle(of: cell.spine))

        #expect(cell.replyHovered && cell.replyLine.hovered)
        #expect(list.controller.cursor(at: list.pointer.location ?? .zero) == .pointingHand)
        var wanted: CGColor?
        cell.effectiveAppearance.performAsCurrentDrawingAppearance {
            wanted = Palette.interactiveText.cgColor
        }
        #expect(cell.spine.shape.strokeColor == wanted)
    }

    @Test
    func theNameAndTheAvatarAreNotLinksYet() throws {
        shown()
        let cell = try cell(5)
        let name = try #require(cell.replyLine.nameFrame)
        let point = cell.replyLine.convert(NSPoint(x: name.midX, y: name.midY), to: list.table)

        list.move(to: point)
        #expect(!cell.replyHovered)
        #expect(list.controller.cursor(at: list.pointer.location ?? .zero) == .arrow)

        list.move(to: middle(of: cell.replyLine.avatar))
        #expect(!cell.replyHovered)
        #expect(list.controller.cursor(at: list.pointer.location ?? .zero) == .arrow)
        #expect(list.hovered == [5], "the row is still the hovered one")
    }

    // The pointer may be elsewhere by then; the list says so after its update.
    @Test
    func aCellShownForAnotherReplyIsNotLit() throws {
        shown()
        let cell = try cell(5)
        list.move(to: middle(of: cell.replyLine.content))
        #expect(cell.replyHovered)

        let other = replyRow(7, at: list.noon + 900, content: "another", target("something else"))
        let line = try #require(other.reply)
        let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)
        cell.configure(
            other.message, body: NSAttributedString(string: "another"), startsGroup: true,
            groupTime: "", shortTime: "", fullDate: "",
            reply: (line, ReplyLineView.text(of: line, now: list.noon, times: times)))

        #expect(!cell.replyHovered && !cell.replyLine.hovered && !cell.spine.hovered)
        #expect(
            cell.replyLine.content.shownText.attribute(.foregroundColor, at: 0, effectiveRange: nil)
                as? NSColor === Palette.textDefault)
        var wanted: CGColor?
        cell.effectiveAppearance.performAsCurrentDrawingAppearance {
            wanted = Palette.spine.cgColor
        }
        #expect(cell.spine.shape.strokeColor == wanted)
    }

    @Test
    func aDeletedTargetsLineTakesNoPointer() throws {
        shown(.deleted)
        let cell = try cell(5)

        list.move(to: middle(of: cell.replyLine.content))

        #expect(!cell.replyLine.hovered && !cell.spine.hovered)
        #expect(list.controller.cursor(at: list.pointer.location ?? .zero) == .arrow)
        cell.replyLine.content.performClick(nil)
        cell.spine.performClick(nil)
        #expect(!list.actions.calls.contains { $0.hasPrefix("jump to target") })
    }

    @Test
    func aClickOnTheTextOrTheSpineJumps() throws {
        shown()
        let cell = try cell(5)

        cell.replyLine.content.performClick(nil)
        cell.spine.performClick(nil)

        #expect(list.actions.calls.filter { $0.hasPrefix("jump") } == ["jump to target of 5", "jump to target of 5"])
    }

    @Test
    func noRowTracksThePointer() throws {
        shown()
        let cell = try cell(5)

        let views = MessageMenuAreaTests.views(NSView.self, in: cell.replyLine) + [cell.replyLine, cell.spine]
        #expect(views.count > 4)
        #expect(views.allSatisfy { $0.trackingAreas.isEmpty })
    }

    @Test
    func theLineHasTheMessagesMenu() throws {
        list.actions.marksUnread = [5]
        shown()
        let cell = try cell(5)
        let event = try #require(
            NSEvent.mouseEvent(
                with: .rightMouseDown, location: cell.replyLine.content.convert(NSPoint(x: 4, y: 9), to: nil),
                modifierFlags: [], timestamp: 0, windowNumber: list.window.windowNumber, context: nil,
                eventNumber: 0, clickCount: 1, pressure: 1))

        #expect(cell.replyLine.content.menu(for: event)?.items.map(\.title).contains("Mark Unread") == true)
        #expect(cell.spine.menu(for: event) != nil || cell.menu(for: event) != nil)
    }
}

// A target that changes reaches only its replies' rows, and nothing the user reads moves.
@MainActor
struct ReplyLineUpdateTests {
    let list = MessageTableTests()

    func rows(_ target: ReplyTarget, mentions: Bool = false) -> [MessageListModel.Row] {
        var rows = list.messages(60, from: 101, longText: true)
        rows.insert(replyRow(90, at: list.noon, content: "an answer", target, mentions: mentions), at: 0)
        return rows
    }

    func line(of raw: UInt64) throws -> ReplyLineView {
        try #require(try list.cell(of: raw) as? MessageCell).replyLine
    }

    @Test
    func anEditedTargetRedrawsOnlyItsReplysRow() throws {
        list.show(rows(target("before")))
        list.scroll(toRowOf: 90)
        list.table.forget()

        list.show(rows(target("after", edited: true)))

        #expect(list.table.reloaded == [IndexSet(integer: list.index(of: 90))])
        #expect(list.table.fullReloads == 0 && list.table.removes == 0 && list.table.inserts == 0)
        #expect(try line(of: 90).content.shownText.string == "after (edited)")
    }

    @Test
    func aTargetLoadedLaterShowsWithItsAt() throws {
        list.show(rows(.notLoaded))
        list.scroll(toRowOf: 90)
        #expect(try line(of: 90).showsBadge)

        list.show(rows(target(), mentions: true))

        #expect(try line(of: 90).shownName == "@Ray")
        #expect(!(try line(of: 90).showsBadge))
    }

    // The reply is above what the user reads: its target's deletion moves no row view.
    @Test
    func aTargetDeletedAboveTheReaderMovesNothing() throws {
        list.show(rows(target(String(repeating: "long ", count: 60))))
        list.scroll(toRowOf: 130)
        let before = list.visibleViews()
        let heights = (0..<list.table.numberOfRows).map { list.table.rect(ofRow: $0).height }
        let origin = list.clip.bounds.origin

        list.show(rows(.deleted))

        let after = list.visibleViews()
        #expect(after.keys.sorted() == before.keys.sorted())
        for (raw, place) in before {
            #expect(after[raw]?.y == place.y, "\(raw)")
            #expect(after[raw]?.view === place.view, "\(raw)")
        }
        #expect((0..<list.table.numberOfRows).map { list.table.rect(ofRow: $0).height } == heights)
        #expect(list.clip.bounds.origin == origin)
        list.expectRowsMatchTheTimeline()
    }

    // A row above the reader gains its line (a thread's first row whose original arrives): it
    // grows by the line, upward, and what the reader sees stays.
    @Test
    func aLineThatAppearsAboveTheReaderMovesNothing() throws {
        var rows = list.messages(60, from: 101, longText: true)
        rows.insert(row(90, at: list.noon, content: "an answer"), at: 0)
        list.show(rows)
        list.scroll(toRowOf: 130)
        let before = list.visibleViews()
        let height = list.table.rect(ofRow: list.index(of: 90)).height
        try #require(!before.keys.contains(90))

        rows[0] = replyRow(90, at: list.noon, content: "an answer", target())
        list.show(rows)

        #expect(list.table.rect(ofRow: list.index(of: 90)).height == height + 22)
        let after = list.visibleViews()
        #expect(after.keys.sorted() == before.keys.sorted())
        for (raw, place) in before {
            #expect(after[raw]?.y == place.y, "\(raw)")
            #expect(after[raw]?.view === place.view, "\(raw)")
        }
        list.expectRowsMatchTheTimeline()

        rows[0] = row(90, at: list.noon, content: "an answer")
        list.show(rows)
        #expect(list.table.rect(ofRow: list.index(of: 90)).height == height)
        for (raw, place) in before {
            #expect(list.visibleViews()[raw]?.y == place.y, "\(raw)")
        }
    }

    @Test
    func aReusedCellLetsItsLineGo() throws {
        list.show([replyRow(5, at: list.noon, target("hi <:wave:5>"))])
        let cell = try #require(try list.cell(of: 5) as? MessageCell)
        cell.replyLine.content.line.layoutSubtreeIfNeeded()
        #expect(cell.replyLine.content.line.emojiViews.count == 1)

        cell.configure(
            row(6, at: list.noon).message, body: NSAttributedString(string: "plain"), startsGroup: true,
            groupTime: "", shortTime: "", fullDate: "")
        cell.layoutSubtreeIfNeeded()

        #expect(cell.replyLine.isHidden && cell.spine.isHidden)
        #expect(cell.replyLine.line == nil)
        #expect(cell.replyLine.content.shownText.length == 0)
    }
}

// A time in the answered message's text follows the system's settings as the list's do.
@MainActor
struct ReplyLineTimeTests {
    let refresh = TimeRefreshTests()

    @Test
    func aLocaleChangeReformatsATimeInTheLine() async throws {
        let at = refresh.seconds(berlinTime(2026, 10, 10, 14, 5))
        refresh.show([replyRow(5, at: berlinTime(2026, 10, 10, 15, 0), content: "an answer", target("at <t:\(at):t>"))])
        let line = { () -> String in
            let cell = refresh.table.view(atColumn: 0, row: refresh.index(of: 5), makeIfNecessary: true) as? MessageCell
            return cell?.replyLine.content.shownText.string ?? ""
        }
        #expect(line().contains("2:05\u{202F}PM"), "\(line())")

        refresh.clock.locale = Locale(identifier: "de_DE")
        await refresh.post(NSLocale.currentLocaleDidChangeNotification)

        #expect(line().contains("14:05"), "\(line())")
    }

    @Test
    func aRelativeTimeInTheLineTicks() {
        let due = refresh.seconds(refresh.clock.now) + 61
        refresh.show([
            replyRow(5, at: refresh.clock.now, content: "an answer", target("due <t:\(due):R>"))
        ])
        let index = refresh.index(of: 5)
        let line = { () -> String in
            let cell = refresh.table.view(atColumn: 0, row: index, makeIfNecessary: true) as? MessageCell
            return cell?.replyLine.content.shownText.string ?? ""
        }
        let times = TimestampText(
            locale: refresh.clock.locale, timeZone: refresh.clock.calendar.timeZone,
            calendar: refresh.clock.calendar)
        let before = line()
        let height = refresh.table.rect(ofRow: index).height
        #expect(before == "due " + times.text(due, style: .relative, now: refresh.clock.now), "\(before)")
        #expect(refresh.timers.active.count == 1)
        #expect(
            refresh.timers.active.first?.date
                == times.nextChange(due, style: .relative, now: refresh.clock.now))

        refresh.clock.now += 2
        refresh.timers.fire()

        #expect(line() == "due " + times.text(due, style: .relative, now: refresh.clock.now), "\(line())")
        #expect(line() != before)
        #expect(refresh.table.rect(ofRow: index).height == height)
        #expect(refresh.timers.active.count == 1, "the next change is asked for")
    }
}
