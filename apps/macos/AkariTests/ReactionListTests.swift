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

        #expect(list.actions.calls == ["toggle 🎉 on 2 from inlineButton"])
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
    func theAddButtonOpensTheQuickReactionsUnderIt() throws {
        list.actions.quick[2] = [
            QuickReaction(emoji: .unicode("💯"), mine: false),
            QuickReaction(emoji: .unicode("😆"), mine: true),
        ]
        list.show(rows(reactions(pill("👍"))))
        let add = try pills(2).add
        var menus: [NSMenu] = []
        list.controller.popUp = { menu, _, _ in menus.append(menu) }

        add.performClick(nil)

        let menu = try #require(menus.first)
        #expect(menu.items.map(\.title) == ["💯", "😆"])
        #expect(menu.items.map(\.state) == [.off, .on])
        #expect(menu.items.map(\.toolTip) == ["Click to react", "Click to remove"])
        list.actions.calls = []
        menu.performActionForItem(at: 0)
        #expect(list.actions.calls == ["react 💯 on 2 from inlineButton"])
    }

    @Test
    func withoutQuickReactionsTheAddButtonOpensNothing() throws {
        list.show(rows(reactions(pill("👍"))))
        var menus = 0
        list.controller.popUp = { _, _, _ in menus += 1 }

        try pills(2).add.performClick(nil)

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
        #expect(thumbs.toolTip == "👍 reacted by 3 people")

        list.actions.names["👍"] = "👍 reacted by Ann, Bo, and Cy"
        list.controller.reactionNamesArrived(for: MessageId(rawValue: 3))
        #expect(thumbs.toolTip == "👍 reacted by 3 people")

        list.controller.reactionNamesArrived(for: MessageId(rawValue: 2))
        #expect(thumbs.toolTip == "👍 reacted by Ann, Bo, and Cy")
    }

    @Test
    func thePointerRestingOnAPillAsksForItsNamesAndLeavingEndsIt() throws {
        list.show(rows(reactions(pill("👍"), pill("🎉"))))
        let row = try pills(2)

        point(at: row.pills[0])
        point(at: row.pills[1])
        try list.hover(1)

        #expect(list.actions.rested.suffix(3) == ["👍 of 2", "🎉 of 2", "none"])
    }

    // MARK: The count's roll

    @Test
    func aCountRollsOnlyInTheKeyWindowWithMotion() throws {
        list.show(rows(reactions(pill("👍", 2))))
        let thumbs = try pills(2).pills[0]
        #expect(!list.window.isKeyWindow)

        list.show(rows(reactions(pill("👍", 3))))

        #expect(thumbs.countText == "3")
        #expect(thumbs.countLayer.animation(forKey: ReactionPillButton.rollKey) == nil)
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
        #expect(list.actions.calls == ["toggle 👋 on 5 from inlineButton"])

        list.show([row(1, by: ray, at: list.noon), join])
        #expect(rowHeight(5) == plain)
        #expect(cell.reactions?.isHidden == true)
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
        #expect(bar.quickButtons.map(\.reaction.name) == ["💯", "😆", "💖"])
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
    func aRowWithoutQuickReactionsHasNoLine() throws {
        show()
        try list.hover(1)

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
    func aQuickReactionsClickReactsFromTheHoverBar() throws {
        show()
        try list.hover(2)
        list.actions.calls = []

        list.bar.quickButtons[1].performClick(nil)

        #expect(list.actions.calls == ["react 😆 on 2 from hoverBar"])
    }

    @Test
    func aQuickReactionSaysWhatItsClickDoes() throws {
        show()
        try list.hover(2)

        #expect(list.bar.quickButtons.map(\.toolTip) == [
            "💯\nClick to react", "😆\nClick to remove", "💖\nClick to react",
        ])
        #expect(list.bar.quickButtons[1].accessibilityLabel() == "Click to remove 😆")
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

    @Test
    func addReactionInTheBarOpensTheQuickReactionsUnderItsButton() throws {
        show()
        try list.hover(2)
        var opened: [(menu: NSMenu, view: NSView)] = []
        list.controller.popUp = { menu, _, view in opened.append((menu, view)) }

        try #require(list.bar.button(.addReaction)).performClick(nil)

        let menu = try #require(opened.first)
        #expect(menu.view === list.bar.button(.addReaction))
        #expect(menu.menu.items.map(\.title) == ["💯", "😆", "💖"])
        list.actions.calls = []
        menu.menu.performActionForItem(at: 2)
        #expect(list.actions.calls == ["react 💖 on 2 from picker"])
    }

    // MARK: The menu

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
        #expect(add.items.map(\.title) == ["💯", "😆", "💖"])
        #expect(add.items.map(\.state) == [.off, .on, .off])
        let remove = try #require(items.first { $0.title == "Remove Reactions" }?.submenu)
        #expect(remove.items.map(\.title) == ["👍  2", "🎉  12"])
        #expect(items.first { $0.title == "View Reactions" }?.submenu == nil)

        add.performActionForItem(at: 1)
        remove.performActionForItem(at: 0)
        #expect(ran == ["react 😆", "remove 👍"])
    }

    @Test
    func theMessagesMenuReactsAndRemovesThroughTheList() throws {
        show()
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
            "react 💯 on 2 from contextMenu", "remove 👍 from 2", "remove 👍 from 2 shift",
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
