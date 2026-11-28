import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// The app's own wiring of an edit, past the models and the table: Up arrow in the composer, the
// list's state, and the bridge between the table's editor and `EditModel`.
@MainActor
struct EditWiringTests {
    let screens = SessionScreenTests()
    let account = FormattedAccount(contents: ["test"])
    let channel = FormattedStore.channelIds[0]

    init() {
        account.formattedStore.editable.withLock { $0 = true }
    }

    func bridge(_ session: SessionModel) throws -> (MessageListBridge, MessageListModel) {
        let messages = try #require(session.messages)
        let bridge = MessageListBridge(
            messages: messages, link: MessageTableLink(),
            links: LinkFollower(trusted: session.trustedLinks()), go: { _ in }, markServerRead: {},
            messageActions: session.messageActions(for: messages) { _ in })
        return (bridge, messages)
    }

    @Test
    func upArrowInTheComposerOpensTheEditorInTheNewestRowWithItsText() async throws {
        let session = screens.formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        let newest = account.formattedStore.newest(of: channel)
        let table = try await screens.shown(newest, in: host)
        let controller = try #require(table.delegate as? MessageTableController)
        let composer = try #require(screens.views(ComposerTextView.self, in: host).first)
        let list = try #require(session.messages)

        composer.doCommand(by: #selector(NSResponder.moveUp(_:)))

        #expect(list.editor.editing?.messageId == newest)
        try await screens.waitUntil(host) { controller.editor.superview != nil }
        let index = try #require(controller.timeline.items.firstIndex { $0.id == .message(newest) })
        let cell = try #require(table.view(atColumn: 0, row: index, makeIfNecessary: false) as? MessageCell)
        #expect(cell.editor === controller.editor)
        #expect(controller.editor.textView.string == list.rows.last?.shown.content)
        session.close()
    }

    @Test
    func theBridgeCarriesTheEditorsTextCounterSaveCancelAndFocus() async throws {
        let session = screens.formattedSession(account)
        let (bridge, messages) = try bridge(session)
        let row = try #require(messages.rows.last)
        messages.editor.start(row)

        #expect(bridge.editText() == row.shown.content)
        bridge.editChanged("changed")
        #expect(messages.editor.text == "changed")
        #expect(bridge.editRemaining() == nil)
        bridge.editChanged(String(repeating: "a", count: 1850))
        #expect(bridge.editRemaining() == 150)
        bridge.editFocused(true)
        #expect(messages.editor.editorFocused)

        bridge.editChanged("changed")
        bridge.saveEdit()
        #expect(messages.editor.editing == nil)
        try await screens.waitUntil(NSView()) { !account.editCalls.withLock { $0 }.isEmpty }
        #expect(account.editCalls.withLock { $0 } == ["edit \(row.message.id.rawValue): changed"])

        messages.editor.start(row)
        bridge.cancelEdit()
        #expect(messages.editor.editing == nil)
        session.close()
    }

    @Test
    func theBarsDeleteAsksWithoutTheTipAndTheMenusWithIt() throws {
        let session = screens.formattedSession(account)
        let (bridge, messages) = try bridge(session)
        let row = try #require(messages.rows.last)

        bridge.perform(.delete, on: row, shift: false, from: .bar)
        #expect(session.deleteQuestion?.showsTip == false)
        session.answerDeleteQuestion(false)

        bridge.perform(.delete, on: row, shift: false, from: .menu)
        #expect(session.deleteQuestion?.showsTip == true)
        session.close()
    }
}

// Escape in a list of its own (no session's Escape behind it): an open edit takes the key.
@MainActor
struct EditEscapeBridgeTests {
    let list = MessageTableTests()
    let account = FormattedAccount(contents: ["test"])

    @Test
    func anOpenEditTakesEscapeAndNothingScrolls() async throws {
        account.formattedStore.editable.withLock { $0 = true }
        account.formattedStore.add(60, to: FormattedStore.channelIds[0])
        let session = SessionScreenTests().formattedSession(account)
        let messages = try #require(session.messages)
        let bridge = MessageListBridge(
            messages: messages, link: MessageTableLink(),
            links: LinkFollower(trusted: session.trustedLinks()), go: { _ in }, markServerRead: {})
        bridge.link.controller = list.controller
        list.controller.actions = bridge
        list.show(MessageTableState(rows: messages.rows))
        list.scroll(toRowOf: messages.rows[20].message.id.rawValue)
        let origin = list.clip.bounds.origin
        try #require(!list.controller.isPinnedToBottom)
        messages.editor.start(messages.rows[20])

        bridge.escape()
        try await SessionScreenTests().waitUntil(list.table) { messages.editor.editing == nil }

        #expect(list.clip.bounds.origin == origin, "the list stays where it is read")
        #expect(account.markedRead.withLock { $0 }.isEmpty)
        session.close()
    }
}

// The delete question's window as the session shows it: over the dimmed session, Delete and
// Return delete, and no way of closing it does.
@MainActor
struct DeleteQuestionWindowTests {
    let account = FormattedAccount(contents: ["test"])
    let session: SessionModel
    let view: ShownView

    init() {
        account.formattedStore.editable.withLock { $0 = true }
        session = SessionScreenTests().formattedSession(account)
        view = ShownView(SessionScreen(session: session, splitWidth: SplitWidth()), size: NSSize(width: 1100, height: 700))
    }

    var shield: ShieldWindow? {
        view.window.childWindows?.compactMap { $0 as? ShieldWindow }.first
    }

    var panel: ContainerPanel? {
        shield?.childWindows?.compactMap { $0 as? ContainerPanel }.first
    }

    func ask(_ row: MessageListModel.Row) async throws -> ContainerPanel {
        let list = try #require(session.messages)
        session.messageActions(for: list) { _ in }.perform(.delete, on: row, shift: false, from: .menu)
        try await view.waitUntil("the question") { panel != nil }
        return try #require(panel)
    }

    func rows() throws -> (MessageListModel.Row, MessageListModel.Row) {
        let rows = try #require(session.messages?.rows)
        return (try #require(rows.last), try #require(rows.dropLast().last))
    }

    func returnKey(in panel: NSWindow) throws -> NSEvent {
        try #require(
            NSEvent.keyEvent(
                with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                windowNumber: panel.windowNumber, context: nil, characters: "\r",
                charactersIgnoringModifiers: "\r", isARepeat: false, keyCode: 36))
    }

    // Whatever an earlier answer started reaches the account before this delete does.
    func deleteAfterwards(_ row: MessageListModel.Row) async throws {
        let panel = try await ask(row)
        panel.sendEvent(try returnKey(in: panel))
        try await view.waitUntil("the delete") {
            account.editCalls.withLock { $0 }.contains("delete \(row.message.id.rawValue)")
        }
    }

    @Test
    func theQuestionShowsOverTheDimmedSessionAndReturnDeletes() async throws {
        let (row, _) = try rows()

        let panel = try await ask(row)
        #expect((shield?.backgroundColor.alphaComponent ?? 0) > 0.5)
        panel.sendEvent(try returnKey(in: panel))

        try await view.waitUntil("the delete") { !account.editCalls.withLock { $0 }.isEmpty }
        #expect(account.editCalls.withLock { $0 } == ["delete \(row.message.id.rawValue)"])
        #expect(session.deleteQuestion == nil)
        session.close()
    }

    @Test
    func cancelDeletesNothing() async throws {
        let (row, other) = try rows()
        let panel = try await ask(row)
        let content = try #require(panel.contentView)
        let question = try #require(MessageMenuAreaTests.views(MessageQuestionView.self, in: content).first)

        question.cancel.performClick(nil)

        try await view.waitUntil("closed") { session.deleteQuestion == nil && self.panel == nil }
        try await deleteAfterwards(other)
        #expect(account.editCalls.withLock { $0 } == ["delete \(other.message.id.rawValue)"])
        session.close()
    }

    enum Closing: CaseIterable {
        case commandW, pressOutside, miniaturize
    }

    @Test(arguments: Closing.allCases)
    func closingTheWindowAnyWayDeletesNothing(_ closing: Closing) async throws {
        let (row, other) = try rows()
        let panel = try await ask(row)

        switch closing {
        case .commandW:
            panel.performClose(nil)
        case .pressOutside:
            try #require(shield).onPress()
        case .miniaturize:
            NotificationCenter.default.post(name: NSWindow.willMiniaturizeNotification, object: view.window)
        }

        try await view.waitUntil("closed") { session.deleteQuestion == nil && self.panel == nil }
        try await deleteAfterwards(other)
        #expect(account.editCalls.withLock { $0 } == ["delete \(other.message.id.rawValue)"])
        session.close()
    }
}
