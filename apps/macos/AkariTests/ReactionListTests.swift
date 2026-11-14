import AkariKit
import AppKit
import Testing

@testable import Akari

// Reactions in the message list: the pill row in its cell, the pointer, clicks, and changes in
// place.
@MainActor
struct ReactionListTests {
    let list = BarList()

    var ray: User { author(2, "Ray") }

    func rows(_ second: ReactionRow?) -> [MessageListModel.Row] {
        [
            row(1, by: ray, at: list.noon, content: "first"),
            row(2, by: ray, at: list.noon + 10, content: "second").reacting(second),
            row(3, by: ray, at: list.noon + 20, content: "third"),
        ]
    }

    func cell(_ raw: UInt64) throws -> MessageCell {
        try #require(try list.cell(of: raw) as? MessageCell)
    }

    func pills(_ raw: UInt64) throws -> ReactionRowView {
        let view = try #require(try cell(raw).reactions)
        view.layoutSubtreeIfNeeded()
        return view
    }

    /// The pointer goes to the middle of a view of the list.
    func point(at view: NSView) {
        list.move(to: list.table.convert(NSPoint(x: view.bounds.midX, y: view.bounds.midY), from: view))
    }

    func rowHeight(_ raw: UInt64) -> CGFloat {
        list.table.rect(ofRow: list.index(of: raw)).height
    }

    @Test
    func aMessagesReactionsStandUnderItsTextInTheTextsColumn() throws {
        list.show(rows(reactions(pill("👍", 2), pill("🎉", mine: true))))
        let cell = try cell(2)
        let row = try pills(2)

        let first = cell.convert(row.pills[0].bounds, from: row.pills[0])
        #expect(first.minX == 72)
        #expect(first.height == 30)
        #expect(row.pills.map(\.countText) == ["2", "1"])
        #expect(!row.add.isHidden)
        // Two under the last pills, then the row's own bottom.
        let top = cell.isFlipped ? first.maxY : cell.bounds.height - first.minY
        #expect(cell.bounds.height - top == 3)
    }

    @Test
    func theRowsHeightIsItsTextsPlusTheReactions() throws {
        list.show(rows(nil))
        let plain = rowHeight(2)

        list.show(rows(reactions(pill("👍"))))

        // The stack's 2 and the row's 33.
        #expect(rowHeight(2) == plain + 35)
        let cell = try cell(2)
        #expect(cell.fittingSize.height <= rowHeight(2))
    }

    @Test
    func underAStickerTheReactionsStandTwoLower() throws {
        let sticker = row(2, by: ray, at: list.noon + 10, content: "second", stickers: ["wave"])
        list.show([row(1, by: ray, at: list.noon, content: "first"), sticker])
        let plain = rowHeight(2)

        list.show([
            row(1, by: ray, at: list.noon, content: "first"), sticker.reacting(reactions(pill("👍"))),
        ])

        // The stack's 2, the 2 that make Discord's 4 between accessories, and the row's 33.
        #expect(rowHeight(2) == plain + 37)
        let cell = try cell(2)
        let first = try pills(2).pills[0]
        let frame = cell.convert(first.bounds, from: first)
        let top = cell.isFlipped ? frame.maxY : cell.bounds.height - frame.minY
        #expect(cell.bounds.height - top == 3)
    }

    // Discord's accessories are 4 apart: under attachments, embeds and the line for what Akari
    // can't show, as under stickers.
    @Test(arguments: ["attachment", "embed", "extras"])
    func underTheOtherAccessoriesTheReactionsStandTwoLower(_ accessory: String) throws {
        let shown = switch accessory {
        case "attachment": row(2, by: ray, at: list.noon + 10, content: "second", attachments: ["a.zip"])
        case "embed": row(2, by: ray, at: list.noon + 10, content: "second", embeds: [embed()])
        default: row(2, by: ray, at: list.noon + 10, content: "second", componentsV2: true)
        }
        list.show([row(1, by: ray, at: list.noon, content: "first"), shown])
        let plain = rowHeight(2)

        list.show([row(1, by: ray, at: list.noon, content: "first"), shown.reacting(reactions(pill("👍")))])

        #expect(rowHeight(2) == plain + 37)
        let cell = try cell(2)
        let first = try pills(2).pills[0]
        let frame = cell.convert(first.bounds, from: first)
        let top = cell.isFlipped ? frame.maxY : cell.bounds.height - frame.minY
        #expect(cell.bounds.height - top == 3)
    }

    // A thread started from a message that has reactions: its box comes under them.
    @Test
    func theThreadBoxStandsUnderTheReactions() throws {
        list.show(rows(reactions(pill("👍"))))
        let box = ThreadBox(
            threadId: ChannelId(rawValue: 20), name: "t1", link: "See Thread ›", bottom: .noMessages,
            time: nil)
        let threaded = row(
            2, by: ray, at: list.noon + 10, content: "second", hasThread: true, threadBox: box)

        list.show([
            row(1, by: ray, at: list.noon, content: "first"),
            threaded.reacting(reactions(pill("👍"))),
            row(3, by: ray, at: list.noon + 20, content: "third"),
        ])

        let cell = try cell(2)
        let pills = try pills(2)
        let boxView = try #require(ThreadBoxViewTests.boxView(in: cell))
        let reacted = cell.convert(pills.bounds, from: pills)
        let started = cell.convert(boxView.bounds, from: boxView)
        #expect(cell.isFlipped ? reacted.maxY <= started.minY : reacted.minY >= started.maxY)
        #expect(abs(cell.fittingSize.height - rowHeight(2)) < 0.5)
    }

    @Test
    func pillsTakeTheListsImagesAndItsSayOnAnimation() throws {
        let cache = ImageCache(source: FakeImageSource.stalling())
        var state = MessageTableState(rows: rows(reactions(customPill(7))), reachedOldest: true)
        state.images = ImageCacheRef(cache, placeholders: nil)
        list.controller.show(state)
        list.window.layoutIfNeeded()
        let cell = try cell(2)
        #expect(try pills(2).pills.map { $0.images === cache } == [true])

        state.rows = rows(reactions(customPill(7), customPill(8)))
        list.controller.show(state)
        #expect(try pills(2).pills.map { $0.images === cache } == [true, true])

        let later = ImageCache(source: FakeImageSource.stalling())
        cell.images = later
        #expect(try pills(2).pills.map { $0.images === later } == [true, true])
        cell.animatesEmoji = true
        #expect(try pills(2).pills.map(\.animates) == [true, true])
    }

    @Test
    func aRowOfReactionsMadeLaterTakesTheCellsImagesAndSay() throws {
        list.show(rows(nil))
        let cell = try cell(2)
        let cache = ImageCache(source: FakeImageSource.stalling())
        cell.images = cache
        cell.animatesEmoji = true

        cell.showReactions(reactions(customPill(7)))
        cell.layoutSubtreeIfNeeded()

        let row = try #require(cell.reactions)
        #expect(row.images === cache)
        #expect(row.animatesEmoji)
        #expect(row.pills.map { $0.images === cache && $0.animates } == [true])
    }

    @Test
    func aMessageWithoutReactionsHasNoRow() throws {
        list.show(rows(nil))

        #expect(try cell(2).reactions?.isHidden != false)
    }

    @Test
    func aReusedCellLetsItsReactionsGo() throws {
        list.show(rows(reactions(pill("👍"))))
        let cell = try cell(2)

        cell.showReactions(nil)

        #expect(cell.reactions?.isHidden == true)
        #expect(cell.reactions?.pills.isEmpty == true)
    }

    // MARK: Changes in place

    @Test
    func aReactionChangeKeepsTheCellAndItsTextAndChangesThePills() throws {
        list.show(rows(reactions(pill("👍", 2))))
        let before = try cell(2)
        let thumbs = try pills(2).pills[0]
        let height = rowHeight(2)

        list.show(rows(reactions(pill("👍", 3, mine: true))))

        #expect(try cell(2) === before, "the row wasn't reloaded")
        #expect(try pills(2).pills[0] === thumbs)
        #expect(thumbs.countText == "3")
        #expect(thumbs.pill?.mine == true)
        #expect(rowHeight(2) == height)
    }

    @Test
    func theFirstReactionAndTheLastChangeTheRowsHeightWithoutAReload() throws {
        list.show(rows(nil))
        let before = try cell(2)
        let plain = rowHeight(2)
        let third = list.table.rect(ofRow: list.index(of: 3)).minY

        list.show(rows(reactions(pill("👍"))))
        #expect(try cell(2) === before)
        #expect(rowHeight(2) == plain + 35)
        #expect(list.table.rect(ofRow: list.index(of: 3)).minY == third + 35)
        list.window.layoutIfNeeded()
        let below = try #require(list.table.rowView(atRow: list.index(of: 3), makeIfNecessary: false))
        #expect(below.frame.minY == third + 35, "the row view below moved with it")

        list.show(rows(nil))
        #expect(try cell(2) === before)
        #expect(rowHeight(2) == plain)
        #expect(before.reactions?.isHidden == true)
    }

    @Test
    func pillsThatWrapOntoASecondLineGrowTheRowByThirtyFour() throws {
        let column = list.table.tableColumns[0].width - 88
        let fits = Int((column + 4) / 56)
        let one = (0..<fits - 1).map { pill(String(UnicodeScalar(UInt8(65 + $0)))) }
        list.show(rows(ReactionRow(pills: one, adds: false, toggles: true)))
        let height = rowHeight(2)

        let two = one + [pill("y"), pill("z")]
        list.show(rows(ReactionRow(pills: two, adds: false, toggles: true)))

        #expect(rowHeight(2) == height + 34)
        let row = try pills(2)
        #expect(Set(row.pills.map(\.frame.minY)).count == 2)
        #expect(row.pills.allSatisfy { $0.frame.maxX <= row.bounds.width })
    }

    @Test
    func aListAtItsBottomStaysThereWhenTheNewestMessageGetsAReaction() throws {
        let rows = list.many(30)
        list.show(rows)
        #expect(list.controller.isPinnedToBottom)

        var reacted = rows
        reacted[29] = rows[29].reacting(reactions(pill("👍")))
        list.show(reacted)
        list.window.layoutIfNeeded()

        #expect(list.controller.isPinnedToBottom)
        let last = list.table.rect(ofRow: list.index(of: 30))
        #expect(last.maxY <= list.table.visibleRect.maxY + 0.5)
    }

    @Test
    func aReaderKeepsTheirPlaceWhenAMessageAboveGetsAReaction() throws {
        let rows = list.many(40, longText: true)
        list.show(rows)
        let reader = list.index(of: 20)
        list.table.scrollToVisible(list.table.rect(ofRow: reader))
        list.clip.scroll(to: NSPoint(x: 0, y: list.table.rect(ofRow: reader).minY))
        list.window.layoutIfNeeded()
        let before = list.table.rect(ofRow: reader).minY - list.clip.bounds.minY

        var reacted = rows
        reacted[4] = rows[4].reacting(reactions(pill("👍")))
        list.show(reacted)
        list.window.layoutIfNeeded()

        let after = list.table.rect(ofRow: reader).minY - list.clip.bounds.minY
        #expect(abs(after - before) < 0.5, "the text under the reader's eyes stayed: \(before) → \(after)")
    }

    // MARK: The pointer and clicks

    @Test
    func thePointerLightsThePillUnderItAndShowsTheHand() throws {
        list.show(rows(reactions(pill("👍"), pill("🎉"))))
        let row = try pills(2)

        point(at: row.pills[1])

        #expect(row.pills.map(\.isHovered) == [false, true])
        #expect(list.hovered == [2], "the pill's message is the hovered one")
        let location = try #require(list.pointer.location)
        #expect(list.controller.cursor(at: location) == .pointingHand)

        point(at: row.add)
        #expect(row.pills.map(\.isHovered) == [false, false])
        #expect(row.add.isHovered)
        #expect(list.controller.cursor(at: try #require(list.pointer.location)) == .pointingHand)

        try list.hover(1)
        #expect(!row.add.isHovered)
    }

    @Test
    func aPillThatOnlyShowsHasTheNotAllowedCursorAndTakesNoClick() throws {
        list.show(rows(reactions(pill("👍"), adds: false, toggles: false)))
        let thumbs = try pills(2).pills[0]

        point(at: thumbs)
        #expect(list.controller.cursor(at: try #require(list.pointer.location)) == .operationNotAllowed)
        #expect(!thumbs.isEnabled)

        list.actions.calls = []
        thumbs.performClick(nil)
        #expect(!list.actions.calls.contains { $0.hasPrefix("toggle") })
    }

    @Test
    func aClickOnAPillTogglesItsReactionFromTheInlineButton() throws {
        list.show(rows(reactions(pill("👍", 2), pill("🎉", mine: true))))
        list.actions.calls = []

        try pills(2).pills[1].performClick(nil)

        #expect(list.actions.calls == ["toggle :tada: on 2 from inlineButton"])
    }

    @Test
    func aClickAfterAChangeInPlaceActsOnTheRowAsItIsNow() throws {
        list.show(rows(reactions(pill("👍", 2))))
        let changed = reactions(pill("👍", 3, mine: true))
        list.show(rows(changed))

        try pills(2).pills[0].performClick(nil)

        #expect(list.actions.performed.last?.reactions == changed)
    }

    @Test
    func whereThePickerIsntOfferedTheAddButtonOpensNothing() throws {
        list.show(rows(reactions(pill("👍"))))
        var menus = 0
        list.controller.popUp = { _, _, _ in menus += 1 }

        try pills(2).add.performClick(nil)

        #expect(list.actions.calls.contains("picker for 2 from inlineButton"))
        #expect(list.controller.emojiPicker.windows.panel == nil)
        #expect(menus == 0)
    }

    @Test
    func anOptionClickOnAPillIsThePills() throws {
        list.actions.marksUnread = [2]
        list.show(rows(reactions(pill("👍"))))
        let thumbs = try pills(2).pills[0]
        let point = thumbs.convert(NSPoint(x: thumbs.bounds.midX, y: thumbs.bounds.midY), to: nil)
        list.pointer.location = point
        let event = try #require(
            NSEvent.mouseEvent(
                with: .leftMouseDown, location: point, modifierFlags: .option, timestamp: 0,
                windowNumber: list.window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                pressure: 1))

        #expect(!list.controller.optionClick(event))
    }

    @Test
    func aRightClickOnAPillOpensItsMessagesMenu() throws {
        list.actions.marksUnread = [2]
        list.show(rows(reactions(pill("👍"))))
        let thumbs = try pills(2).pills[0]
        let point = thumbs.convert(NSPoint(x: thumbs.bounds.midX, y: thumbs.bounds.midY), to: nil)
        let event = try #require(
            NSEvent.mouseEvent(
                with: .rightMouseDown, location: point, modifierFlags: [], timestamp: 0,
                windowNumber: list.window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                pressure: 1))

        #expect(thumbs.menu(for: event)?.items.map(\.title) == ["Copy Text", "Mark Unread"])
    }

    // MARK: The tooltip

    @Test
    func aPillsTooltipCountsPeopleUntilItsNamesArrive() throws {
        list.show(rows(reactions(pill("👍", 3))))
        let thumbs = try pills(2).pills[0]
        #expect(thumbs.toolTip == ":thumbsup: reacted by 3 people")

        list.actions.names[":thumbsup:"] = ":thumbsup: reacted by Ann, Bo, and Cy"
        list.controller.reactionNamesArrived(for: MessageId(rawValue: 3))
        #expect(thumbs.toolTip == ":thumbsup: reacted by 3 people")

        list.controller.reactionNamesArrived(for: MessageId(rawValue: 2))
        #expect(thumbs.toolTip == ":thumbsup: reacted by Ann, Bo, and Cy")
    }

    // The model keeps a row's names under the row's own ID, a thread's starter row's too.
    @Test
    func aStarterRowsNamesArriveUnderTheRowsOwnID() throws {
        let original = row(5, by: ray, at: list.noon, content: "the original").message
        let first = row(31, by: ray, at: list.noon, kind: .threadStarterMessage, content: "")
        let starter = MessageListModel.Row(
            id: first.id, message: first.message, starter: .loaded(original)
        ).reacting(reactions(pill("👍", 3)))
        list.show([starter])
        let thumbs = try pills(31).pills[0]
        list.actions.names[":thumbsup:"] = ":thumbsup: reacted by Ann, Bo, and Cy"

        list.controller.reactionNamesArrived(for: MessageId(rawValue: 5))
        #expect(thumbs.toolTip == ":thumbsup: reacted by 3 people")

        list.controller.reactionNamesArrived(for: MessageId(rawValue: 31))
        #expect(thumbs.toolTip == ":thumbsup: reacted by Ann, Bo, and Cy")
    }

    @Test
    func thePointerRestingOnAPillAsksForItsNamesAndLeavingEndsIt() throws {
        list.show(rows(reactions(pill("👍"), pill("🎉"))))
        let row = try pills(2)

        point(at: row.pills[0])
        point(at: row.pills[1])
        try list.hover(1)

        #expect(list.actions.rested.suffix(3) == [":thumbsup: of 2", ":tada: of 2", "none"])
    }

    // MARK: The count's roll

    @Test
    func aCountRollsOnlyInTheKeyWindowWithMotion() throws {
        list.controller.reducesMotion = { false }
        list.show(rows(reactions(pill("👍", 2), pill("🎉", 1), pill("👀", 1))))
        let row = try pills(2)
        #expect(!list.window.isKeyWindow)
        list.show(rows(reactions(pill("👍", 3), pill("🎉", 1), pill("👀", 1))))
        #expect(row.pills[0].countText == "3")
        #expect(!row.pills[0].isRolling, "not in a window that isn't key")

        let key = list.keyWindow()
        defer { key.orderOut(nil) }
        list.controller.reducesMotion = { true }
        list.show(rows(reactions(pill("👍", 3), pill("🎉", 2), pill("👀", 1))))
        #expect(row.pills[1].countText == "2")
        #expect(!row.pills[1].isRolling, "not with motion reduced")

        list.controller.reducesMotion = { false }
        list.show(rows(reactions(pill("👍", 3), pill("🎉", 2), pill("👀", 2))))
        #expect(row.pills[2].isRolling)
    }

    @Test
    func aSystemLinesCountRollsAsAMessagesDoes() throws {
        let join = row(5, by: author(4, "Kim"), at: list.noon + 700, kind: .userJoin)
        list.controller.reducesMotion = { false }
        list.show([row(1, by: ray, at: list.noon), join.reacting(reactions(pill("👋", 4)))])
        let key = list.keyWindow()
        defer { key.orderOut(nil) }
        let wave = try #require((try list.cell(of: 5) as? NoticeCell)?.reactions?.pills.first)

        list.show([row(1, by: ray, at: list.noon), join.reacting(reactions(pill("👋", 5)))])

        #expect(wave.countText == "5")
        #expect(wave.isRolling)
    }

    // MARK: The row as it is now

    @Test
    func theTooltipAndTheAddMenuAfterAChangeInPlaceGoWithTheRowAsItIsNow() throws {
        list.actions.quick[2] = [QuickReaction(emoji: .unicode("💯"), mine: false)]
        list.show(rows(reactions(pill("👍", 2))))
        let changed = reactions(pill("👍", 3, mine: true))
        list.show(rows(changed))
        let row = try pills(2)
        var menus: [NSMenu] = []
        list.controller.popUp = { menu, _, _ in menus.append(menu) }

        _ = row.tooltip(try #require(row.pills[0].pill))
        #expect(list.actions.handed["tooltip"]?.reactions == changed)
        row.add.performClick(nil)
        #expect(list.actions.handed["quick"]?.reactions == changed)
        try #require(menus.first).performActionForItem(at: 0)
        #expect(list.actions.handed["react"]?.reactions == changed)
    }

    @Test
    func aSystemLinesTooltipAfterAChangeInPlaceGoesWithTheLineAsItIsNow() throws {
        let join = row(5, by: author(4, "Kim"), at: list.noon + 700, kind: .userJoin)
        list.show([row(1, by: ray, at: list.noon), join.reacting(reactions(pill("👋", 4)))])
        let changed = reactions(pill("👋", 5, mine: true))
        list.show([row(1, by: ray, at: list.noon), join.reacting(changed)])
        let shown = try #require((try list.cell(of: 5) as? NoticeCell)?.reactions)

        _ = shown.tooltip(try #require(shown.pills.first?.pill))

        #expect(list.actions.handed["tooltip"]?.reactions == changed)
    }

    @Test
    func aSystemLinesAddButtonReactsFromTheInlineButton() throws {
        list.actions.quick[5] = [QuickReaction(emoji: .unicode("💯"), mine: false)]
        let join = row(5, by: author(4, "Kim"), at: list.noon + 700, kind: .userJoin)
        list.show([row(1, by: ray, at: list.noon), join.reacting(reactions(pill("👋", 4)))])
        let shown = try #require((try list.cell(of: 5) as? NoticeCell)?.reactions)
        var menus: [NSMenu] = []
        list.controller.popUp = { menu, _, _ in menus.append(menu) }
        list.actions.calls = []

        shown.add.performClick(nil)
        try #require(menus.first).performActionForItem(at: 0)

        #expect(list.actions.calls == ["react 💯 on 5 from inlineButton"])
    }

    @Test
    func aSystemLinesNamesReachItsPills() throws {
        let join = row(5, by: author(4, "Kim"), at: list.noon + 700, kind: .userJoin)
        list.show([row(1, by: ray, at: list.noon), join.reacting(reactions(pill("👋", 4)))])
        let wave = try #require((try list.cell(of: 5) as? NoticeCell)?.reactions?.pills.first)
        #expect(wave.toolTip == "👋 reacted by 4 people")
        list.actions.names["👋"] = "👋 reacted by Ann, Bo, Cy, and 1 other"

        list.controller.reactionNamesArrived(for: MessageId(rawValue: 5))

        #expect(wave.toolTip == "👋 reacted by Ann, Bo, Cy, and 1 other")
    }

    // The add menu stands 4 under its button, and its row stays lit while it is open.
    @Test
    func theAddMenuOpensUnderItsButtonWithItsRowLit() throws {
        list.actions.quick[2] = [QuickReaction(emoji: .unicode("💯"), mine: false)]
        list.show(rows(reactions(pill("👍"))))
        try list.hover(1)
        let add = try pills(2).add
        var opened: [(point: NSPoint, view: NSView, lit: [UInt64])] = []
        list.controller.popUp = { [list] menu, point, view in
            NotificationCenter.default.post(name: NSMenu.didBeginTrackingNotification, object: menu)
            opened.append((point, view, list.hovered))
            NotificationCenter.default.post(name: NSMenu.didEndTrackingNotification, object: menu)
        }

        add.performClick(nil)

        let menu = try #require(opened.first)
        #expect(menu.view === add)
        #expect(menu.point == NSPoint(x: 0, y: add.isFlipped ? add.bounds.maxY + 4 : -4))
        #expect(menu.lit == [2])
    }

    // MARK: System lines

    @Test
    func aSystemLineHasItsReactionsUnderIt() throws {
        let join = row(5, by: author(4, "Kim"), at: list.noon + 700, kind: .userJoin)
        list.show([row(1, by: ray, at: list.noon), join])
        let plain = rowHeight(5)
        let before = try #require(try list.cell(of: 5) as? NoticeCell)

        list.show([row(1, by: ray, at: list.noon), join.reacting(reactions(pill("👋", 4)))])

        let cell = try #require(try list.cell(of: 5) as? NoticeCell)
        #expect(cell === before)
        #expect(rowHeight(5) == plain + 35)
        #expect(abs(cell.fittingSize.height - rowHeight(5)) < 0.5, "the pills stack under the line")
        let shown = try #require(cell.reactions)
        shown.layoutSubtreeIfNeeded()
        #expect(shown.pills.map(\.countText) == ["4"])
        #expect(cell.convert(shown.pills[0].bounds, from: shown.pills[0]).minX == 72)
        list.actions.calls = []
        shown.pills[0].performClick(nil)
        #expect(list.actions.calls == ["toggle :wave: on 5 from inlineButton"])

        list.show([row(1, by: ray, at: list.noon), join])
        #expect(rowHeight(5) == plain)
        #expect(cell.reactions?.isHidden == true)
    }

    @Test
    func aSystemLinesPillsWrapAtItsTextsColumn() throws {
        let many = (0..<10).map { pill(String(UnicodeScalar(UInt8(65 + $0))), 2) }
        let join = row(5, by: author(4, "Kim"), at: list.noon + 700, kind: .userJoin)
            .reacting(ReactionRow(pills: many, adds: true, toggles: true))
        list.show([row(1, by: ray, at: list.noon), join])

        let cell = try #require(try list.cell(of: 5) as? NoticeCell)
        let shown = try #require(cell.reactions)
        shown.layoutSubtreeIfNeeded()

        #expect(Set(shown.pills.map(\.frame.minY)).count == 2, "two lines")
        #expect(abs(cell.fittingSize.height - rowHeight(5)) < 0.5)
    }

    @Test
    func aSystemLineTakesTheListsImagesAndSayAndLetsItsPillsGo() throws {
        let join = row(5, by: author(4, "Kim"), at: list.noon + 700, kind: .userJoin)
        list.show([row(1, by: ray, at: list.noon), join])
        let cell = try #require(try list.cell(of: 5) as? NoticeCell)
        let cache = ImageCache(source: FakeImageSource.stalling())
        cell.images = cache
        cell.animatesEmoji = true

        cell.showReactions(reactions(customPill(7)))
        let shown = try #require(cell.reactions)
        #expect(shown.images === cache && shown.animatesEmoji)
        let later = ImageCache(source: FakeImageSource.stalling())
        cell.images = later
        cell.animatesEmoji = false
        #expect(shown.pills.map { $0.images === later && !$0.animates } == [true])

        cell.showReactions(nil)
        #expect(shown.isHidden)
        #expect(shown.pills.isEmpty)
    }

    // A "started a thread" line: the text, its reactions, then the thread's box.
    @Test
    func aSystemLinesThreadBoxStandsUnderItsReactions() throws {
        let box = ThreadBox(
            threadId: ChannelId(rawValue: 20), name: "t1", link: "See Thread ›", bottom: .noMessages,
            time: nil)
        let started = row(
            5, by: author(4, "Kim"), at: list.noon + 700, kind: .threadCreated, content: "t1",
            reference: MessageReference(
                messageId: nil, channelId: ChannelId(rawValue: 20), guildId: GuildId(rawValue: 1)),
            threadBox: box)
        list.show([row(1, by: ray, at: list.noon), started])
        let plain = rowHeight(5)
        list.show([row(1, by: ray, at: list.noon), started.reacting(reactions(pill("👋", 4)))])

        let cell = try #require(try list.cell(of: 5) as? NoticeCell)
        let shown = try #require(cell.reactions)
        let boxView = try #require(ThreadBoxViewTests.boxView(in: cell))
        let reacted = cell.convert(shown.bounds, from: shown)
        let under = cell.convert(boxView.bounds, from: boxView)
        #expect(cell.isFlipped ? reacted.maxY <= under.minY : reacted.minY >= under.maxY)
        #expect(abs(cell.fittingSize.height - rowHeight(5)) < 0.5)

        list.show([row(1, by: ray, at: list.noon), started])
        cell.layoutSubtreeIfNeeded()
        #expect(rowHeight(5) == plain)
        #expect(abs(cell.fittingSize.height - plain) < 0.5, "the box right under the line again")

        cell.configure(
            started.message, notice: try #require(started.noticeParts), time: "", fullDate: "")
        cell.layoutSubtreeIfNeeded()
        #expect(cell.fittingSize.height < plain - 30, "a hidden box takes no room")
    }

    @Test
    func aSystemLinesHeightIsWhatItsCellNeeds() throws {
        let join = row(5, by: author(4, "Kim"), at: list.noon + 700, kind: .userJoin)
            .reacting(reactions(pill("👋", 4)))
        list.show([row(1, by: ray, at: list.noon), join])

        let cell = try #require(try list.cell(of: 5) as? NoticeCell)

        #expect(abs(cell.fittingSize.height - rowHeight(5)) < 0.5)
    }
}

// The quick reactions in the hover bar and the reaction items of the menu.
@MainActor
struct ReactionBarTests {
    let list = BarList()
    let quick = [
        QuickReaction(emoji: .unicode("💯"), mine: false),
        QuickReaction(emoji: .unicode("😆"), mine: true),
        QuickReaction(emoji: .unicode("💖"), mine: false),
    ]

    func show() {
        list.actions.bars[2] = ([.addReaction, .more], [.copyLink, .addReaction, .more])
        list.actions.quick[2] = quick
        list.show([
            row(1, by: author(2, "Ray"), at: list.noon, content: "first"),
            row(2, by: author(2, "Ray"), at: list.noon + 10, content: "second"),
        ])
    }

    @Test
    func theQuickReactionsStandBeforeTheButtonsWithALineBetween() throws {
        show()
        try list.hover(2)

        let bar = list.bar
        #expect(bar.quickButtons.map(\.reaction.name) == [":100:", ":laughing:", ":sparkling_heart:"])
        #expect(bar.quickButtons.map(\.frame.minX) == [3, 31, 59])
        #expect(!bar.separator.isHidden)
        #expect(bar.separator.frame == NSRect(x: 91, y: 5, width: 1, height: 24))
        #expect(bar.buttons.map(\.frame.minX) == [96, 124])
        // 28 a button, 9 for the line, 6 of border and padding.
        #expect(abs(bar.frame.width - (5 * 28 + 9 + 6)) < 0.01, "\(bar.frame)")
        let box = try list.box(of: 2)
        #expect(abs(bar.frame.maxX - (box.maxX - 14)) < 0.51)
    }

    @Test
    func voiceOverReachesTheQuickReactionsBeforeTheButtons() throws {
        show()
        try list.hover(2)

        let children = list.bar.accessibilityChildren()?.compactMap { $0 as? NSView } ?? []

        #expect(children.count == 5)
        #expect(children.prefix(3).elementsEqual(list.bar.quickButtons as [NSView], by: ===))
    }

    // The emoji at 20 in the middle of the button's 28.
    @Test
    func aQuickReactionsEmojiStandsInTheMiddleOfItsButton() throws {
        show()
        try list.hover(2)
        let button = list.bar.quickButtons[0]
        button.layoutSubtreeIfNeeded()

        let glyph = try #require(button.subviews.first { $0 is EmojiGlyphView })

        #expect(glyph.frame == NSRect(x: 4, y: 4, width: 20, height: 20))
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theQuickReactionUnderThePointerIsFilledAndTheLineIsSubtle(_ name: NSAppearance.Name) throws {
        list.window.appearance = NSAppearance(named: name)
        show()
        try list.hover(2)
        let button = list.bar.quickButtons[0]
        #expect(button.layer?.backgroundColor == nil)

        list.move(to: list.table.convert(NSPoint(x: 14, y: 14), from: button))

        var fill = CGColor.clear
        var line = CGColor.clear
        NSAppearance(named: name)?.performAsCurrentDrawingAppearance {
            fill = NSColor(hex: "#97979f", alpha: 0.12).cgColor
            line = NSColor(hex: "#97979f", alpha: name == .darkAqua ? 31 / 255 : 71 / 255).cgColor
        }
        #expect(HoverBarTests.parts(button.layer?.backgroundColor) == HoverBarTests.parts(fill))
        #expect(HoverBarTests.parts(list.bar.separator.layer?.backgroundColor) == HoverBarTests.parts(line))
    }

    // The list scrolls under a still pointer and the bar goes to another row with the same
    // quick reactions: only one the pointer is over is lit.
    @Test
    func aScrollLightsOnlyTheQuickReactionUnderThePointer() throws {
        let list = BarList(height: 300)
        let rows = list.many(40)
        for row in rows {
            list.actions.quick[row.id.rawValue] = quick
        }
        list.show(rows)
        try list.hover(39)
        let first = list.bar.quickButtons[0]
        list.move(to: list.table.convert(NSPoint(x: 14, y: 14), from: first))
        #expect(first.isHovered)
        var away = 0

        for _ in 1...12 {
            list.clip.scroll(to: NSPoint(x: 0, y: list.clip.bounds.minY - 17))
            list.controller.scrollView.reflectScrolledClipView(list.clip)
            list.controller.pointerMoved()

            let point = list.table.convert(try #require(list.pointer.location), from: nil)
            let under = list.bar.quickButtons.filter { list.table.convert($0.bounds, from: $0).contains(point) }
            #expect(list.bar.quickButtons.filter(\.isHovered) == under)
            away += under.isEmpty ? 1 : 0
        }

        #expect(away > 0, "the bar lay away from the pointer")
    }

    @Test
    func aRowWithoutQuickReactionsHasNoLine() throws {
        show()
        try list.hover(2)
        try list.hover(1)

        #expect(!list.bar.subviews.contains { $0 is QuickReactionButton }, "the last row's went")
        #expect(list.bar.quickButtons.isEmpty)
        #expect(list.bar.separator.isHidden)
        #expect(abs(list.bar.frame.width - (2 * 28 + 6)) < 0.01)
    }

    @Test
    func shiftTakesTheQuickReactionsAway() throws {
        show()
        try list.hover(2)

        list.shift = true
        list.controller.pointerMoved()

        #expect(list.bar.quickButtons.isEmpty)
        #expect(list.bar.buttons.map(\.item.action) == [.copyLink, .addReaction, .more])
    }

    @Test
    func aCustomQuickReactionComesFromTheImageTier() throws {
        let party = customPill(7, "party", animated: true).emoji
        list.actions.bars[2] = ([.addReaction, .more], [.copyLink, .addReaction, .more])
        list.actions.quick[2] = [QuickReaction(emoji: party, mine: false), quick[0]]
        let cache = ImageCache(source: FakeImageSource.stalling())
        var state = MessageTableState(rows: [
            row(1, by: author(2, "Ray"), at: list.noon, content: "first"),
            row(2, by: author(2, "Ray"), at: list.noon + 10, content: "second"),
        ])
        state.reachedOldest = true
        state.images = ImageCacheRef(cache)
        list.controller.show(state)
        list.window.layoutIfNeeded()
        try list.hover(2)

        let custom = list.bar.quickButtons[0]
        custom.layoutSubtreeIfNeeded()
        #expect(custom.picture.content?.key == party.key)
        #expect(custom.picture.images === cache)
        #expect(!custom.picture.isHidden)
        #expect(custom.glyph.isHidden)
        #expect(custom.picture.frame == NSRect(x: 4, y: 4, width: 20, height: 20))
        #expect(custom.picture.fallbackText == ":party:")
        #expect(list.bar.quickButtons[1].picture.isHidden)
        #expect(!list.bar.quickButtons[1].glyph.isHidden)
    }

    @Test
    func anAnimatedQuickReactionPlaysUnderThePointerInTheKeyWindow() throws {
        let party = customPill(7, "party", animated: true).emoji
        list.actions.bars[2] = ([.addReaction, .more], [.copyLink, .addReaction, .more])
        list.actions.quick[2] = [QuickReaction(emoji: party, mine: false)]
        var state = MessageTableState(rows: [
            row(1, by: author(2, "Ray"), at: list.noon, content: "first"),
            row(2, by: author(2, "Ray"), at: list.noon + 10, content: "second"),
        ])
        state.reachedOldest = true
        state.focused = true
        state.motion = MotionPolicy(settings: displayDefaults(), reduceMotion: true)
        list.controller.show(state)
        list.window.layoutIfNeeded()
        try list.hover(2)
        let button = list.bar.quickButtons[0]
        #expect(!button.picture.animates, "reduced motion: only under the pointer")

        list.move(to: list.table.convert(NSPoint(x: 14, y: 14), from: button))
        #expect(button.picture.animates)

        try list.hover(1)
        try list.hover(2)
        #expect(!list.bar.quickButtons[0].picture.animates)
    }

    @Test
    func aQuickReactionsClickReactsFromTheHoverBar() throws {
        show()
        try list.hover(2)
        list.actions.calls = []

        list.bar.quickButtons[1].performClick(nil)

        #expect(list.actions.calls == ["react :laughing: on 2 from hoverBar"])
    }

    @Test
    func aQuickReactionSaysWhatItsClickDoes() throws {
        show()
        try list.hover(2)

        #expect(list.bar.quickButtons.map(\.toolTip) == [
            ":100:\nClick to react", ":laughing:\nClick to remove", ":sparkling_heart:\nClick to react",
        ])
        #expect(list.bar.quickButtons[1].accessibilityLabel() == "Click to remove laughing")
    }

    @Test
    func thePointerLightsTheQuickReactionUnderItAndKeepsTheRow() throws {
        show()
        try list.hover(2)
        let button = list.bar.quickButtons[0]

        list.move(to: list.table.convert(NSPoint(x: 14, y: 14), from: button))

        #expect(button.isHovered)
        #expect(list.hovered == [2])
        #expect(list.controller.cursor(at: try #require(list.pointer.location)) == .pointingHand)

        list.move(to: list.table.convert(NSPoint(x: 14, y: 14), from: list.bar.quickButtons[1]))
        #expect(!button.isHovered)
        #expect(list.bar.quickButtons[1].isHovered)
    }

    @Test
    func theBarsQuickReactionsFollowTheRowsChange() throws {
        show()
        try list.hover(2)
        list.actions.quick[2] = [QuickReaction(emoji: .unicode("💯"), mine: true)]

        list.show([
            row(1, by: author(2, "Ray"), at: list.noon, content: "first"),
            row(2, by: author(2, "Ray"), at: list.noon + 10, content: "second")
                .reacting(reactions(pill("💯", mine: true))),
        ])

        #expect(list.bar.quickButtons.map(\.reaction.mine) == [true])
    }

    // MARK: The menu

    @Test
    func theMenusEmojiStandBesideTheirShortNames() async throws {
        let files = TestImages()
        let cache = ImageCache(
            source: FakeImageSource.serving(try files.png("party", pixels: 64), pixels: 64))
        let party = QuickReaction(emoji: customPill(7, "party").emoji, mine: false)

        let items = MessageMenu.reactions(quick + [party], images: cache) { _ in }

        #expect(items.map(\.title) == [":100:", ":laughing:", ":sparkling_heart:", ":party:"])
        #expect(items.prefix(3).allSatisfy { $0.image?.size == NSSize(width: 16, height: 16) })
        try await eventually { items[3].image != nil }
        #expect(items[3].image?.size == NSSize(width: 16, height: 16))
        let removals = MessageMenu.removals([pill("👍", 2)], images: cache) { _ in }
        #expect(removals.map(\.title) == [":thumbsup:  2"])
        #expect(removals[0].image?.size == NSSize(width: 16, height: 16))
    }

    // A face fills the item's box as it fills a pill's.
    @Test
    func aMenuItemsEmojiIsDrawnInItsBox() throws {
        let image = try #require(MessageMenu.emojiImage("😀"))
        let rep = try #require(
            NSBitmapImageRep(
                bitmapDataPlanes: nil, pixelsWide: 32, pixelsHigh: 32, bitsPerSample: 8,
                samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                bytesPerRow: 0, bitsPerPixel: 0))
        NSGraphicsContext.saveGraphicsState()
        NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
        image.draw(in: NSRect(x: 0, y: 0, width: 32, height: 32))
        NSGraphicsContext.restoreGraphicsState()

        var inked: [(Int, Int)] = []
        for y in 0..<32 {
            for x in 0..<32 where (rep.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0.5 {
                inked.append((x, y))
            }
        }
        let xs = inked.map(\.0)
        let ys = inked.map(\.1)
        #expect((xs.max() ?? 0) - (xs.min() ?? 0) >= 26, "the face fills the box's width")
        #expect((ys.max() ?? 0) - (ys.min() ?? 0) >= 26)
        #expect((xs.min() ?? 0) >= 0 && (xs.max() ?? 99) <= 31)
    }

    func groups() -> [[MessageActionItem]] {
        let item = MessageTableTests.RecordingActions.item
        return [
            [item(.addReaction, "Add Reaction"), item(.viewReactions, "View Reactions")],
            [item(.copyText, "Copy Text")],
            [item(.removeReactions, "Remove Reactions"), item(.removeAllReactions, "Remove All Reactions")],
        ]
    }

    @Test
    func addReactionAndRemoveReactionsListTheirEmoji() throws {
        var ran: [String] = []
        let items = MessageMenu.items(
            groups(),
            choices: { action in
                switch action {
                case .addReaction: MessageMenu.reactions(quick) { ran.append("react \($0.name)") }
                case .removeReactions:
                    MessageMenu.removals([pill("👍", 2), pill("🎉", 12)]) { ran.append("remove \($0.name)") }
                default: nil
                }
            },
            run: { ran.append("\($0)") })

        let add = try #require(items.first { $0.title == "Add Reaction" }?.submenu)
        #expect(add.items.map(\.title) == [":100:", ":laughing:", ":sparkling_heart:"])
        #expect(add.items.map(\.state) == [.off, .on, .off])
        #expect(add.items[1].accessibilityLabel() == "Click to remove 😆")
        let remove = try #require(items.first { $0.title == "Remove Reactions" }?.submenu)
        #expect(remove.items.map(\.title) == [":thumbsup:  2", ":tada:  12"])
        #expect(items.first { $0.title == "View Reactions" }?.submenu == nil)

        add.performActionForItem(at: 1)
        remove.performActionForItem(at: 0)
        #expect(ran == ["react :laughing:", "remove :thumbsup:"])
    }

    // The menu's items were made with the row of the moment the cell was made.
    @Test
    func theMenusReactionsAfterAChangeInPlaceGoWithTheRowAsItIsNow() throws {
        show()
        list.actions.removable[2] = [pill("👍", 2)]
        list.controller.actions = ReactionMenuActions(list.actions, groups: groups())
        let changed = reactions(pill("👍", 3, mine: true))
        list.show([
            row(1, by: author(2, "Ray"), at: list.noon, content: "first"),
            row(2, by: author(2, "Ray"), at: list.noon + 10, content: "second").reacting(changed),
        ])
        let cell = try #require(try list.cell(of: 2) as? MessageCell)

        let items = cell.menuItems()
        try #require(items.first { $0.title == "Add Reaction" }?.submenu).performActionForItem(at: 0)
        try #require(items.first { $0.title == "Remove Reactions" }?.submenu).performActionForItem(at: 0)

        #expect(list.actions.handed["react"]?.reactions == changed)
        #expect(list.actions.handed["remove"]?.reactions == changed)
    }

    @Test
    func theMessagesMenuReactsAndRemovesThroughTheList() throws {
        show()
        list.actions.menuEmoji[2] = quick
        list.actions.removable[2] = [pill("👍", 2)]
        let model = ReactionMenuActions(list.actions, groups: groups())
        list.controller.actions = model
        let cell = try #require(try list.cell(of: 2) as? MessageCell)

        let items = cell.menuItems()

        let add = try #require(items.first { $0.title == "Add Reaction" }?.submenu)
        add.performActionForItem(at: 0)
        let remove = try #require(items.first { $0.title == "Remove Reactions" }?.submenu)
        remove.performActionForItem(at: 0)
        list.shift = true
        remove.performActionForItem(at: 0)
        #expect(list.actions.calls.suffix(3) == [
            "react :100: on 2 from contextMenu", "remove :thumbsup: from 2", "remove :thumbsup: from 2 shift",
        ])
    }
}

// The recording actions with a menu of the test's own.
@MainActor
final class ReactionMenuActions: MessageListActions {
    let base: MessageTableTests.RecordingActions
    let groups: [[MessageActionItem]]

    init(_ base: MessageTableTests.RecordingActions, groups: [[MessageActionItem]]) {
        self.base = base
        self.groups = groups
    }

    func messageMenu(for row: MessageListModel.Row) -> [[MessageActionItem]] { groups }
    func messageBar(for row: MessageListModel.Row, shift: Bool) -> [MessageActionItem] {
        base.messageBar(for: row, shift: shift)
    }
    func quickReactions(for row: MessageListModel.Row, shift: Bool) -> [QuickReaction] {
        base.quickReactions(for: row, shift: shift)
    }
    func menuReactions(for row: MessageListModel.Row) -> [QuickReaction] {
        base.menuReactions(for: row)
    }
    func emojiPicker(for row: MessageListModel.Row, from location: ReactionLocation) -> EmojiPickerModel? {
        base.emojiPicker(for: row, from: location)
    }
    func react(_ quick: QuickReaction, on row: MessageListModel.Row, from location: ReactionLocation) {
        base.react(quick, on: row, from: location)
    }
    func removableReactions(for row: MessageListModel.Row) -> [ReactionPill] {
        base.removableReactions(for: row)
    }
    func removeReactions(of pill: ReactionPill?, on row: MessageListModel.Row, shift: Bool) {
        base.removeReactions(of: pill, on: row, shift: shift)
    }
    func retry(_ message: MessageId) {}
    func delete(_ message: MessageId) {}
    func loadMore(_ edge: MessageTimeline.Edge) {}
    func jumpToLatest() {}
    func escape() {}
    func markServerRead() {}
    func jumpToUnread() {}
    func newMessagesLineOnScreen(_ onScreen: Bool, line: MessageId) {}
    func atBottomChanged(_ atBottom: Bool) {}
    func userScrolled(by distance: CGFloat, height: CGFloat) {}
    func reveal(_ spoiler: SpoilerKey) {}
    func openChannel(_ id: ChannelId) {}
    func openThread(_ id: ChannelId, from parent: ChannelId) {}
    func openThreads(of channel: ChannelId) {}
    func jump(to message: MessageId) {}
    func openGuild(_ id: GuildId) {}
    func openLink(_ url: URL, masked: Bool) {}
}

extension BarList {
    // The list moved into a window that says it is key, as the user's is.
    func keyWindow() -> NSWindow {
        let key = TestWindow.hide(
            MessageTableTests.KeyWindow(
                contentRect: NSRect(origin: .zero, size: NSSize(width: 600, height: 400)),
                styleMask: [.titled, .resizable], backing: .buffered, defer: false))
        key.contentView = controller.scrollView
        key.orderFront(nil)
        key.layoutIfNeeded()
        return key
    }
}

extension ReactionPillButton {
    var isRolling: Bool { countLayer.animation(forKey: Self.rollKey) != nil }
}
