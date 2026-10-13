import AkariKit
import AppKit
import Testing

@testable import Akari

// A pasteboard of the test's own, released when the test ends.
nonisolated final class PrivatePasteboard: @unchecked Sendable {
    let pasteboard = NSPasteboard(name: NSPasteboard.Name("akari.tests.\(UUID().uuidString)"))

    deinit {
        pasteboard.releaseGlobally()
    }
}

@MainActor
struct CopyTests {
    let board = PrivatePasteboard()
    var pasteboard: NSPasteboard { board.pasteboard }
    let times = TimestampText(
        locale: Locale(identifier: "en_US"), timeZone: TimeZone(identifier: "Europe/Berlin")!,
        calendar: Calendar(identifier: .gregorian))
    let now = Date(timeIntervalSince1970: 1_800_000_000 - 300)

    func shown(_ content: String) -> MessageTextView {
        let message = named(row(1, at: now, content: content)).message
        let view = MessageTextView()
        view.show(MessageText.build(message, revealed: [], now: now, times: times).text)
        return view
    }

    // What Cmd+C writes: the selection, through the types the view offers.
    func copied(_ view: MessageTextView, _ selection: NSRange? = nil) -> String? {
        view.setSelectedRange(selection ?? NSRange(location: 0, length: view.string.utf16.count))
        _ = view.writeSelection(to: pasteboard, types: view.writablePasteboardTypes)
        return pasteboard.string(forType: .string)
    }

    // The pasteboard lists its legacy name for plain text beside it.
    var holdsOnlyPlainText: Bool {
        let types = pasteboard.types ?? []
        let plain: Set<NSPasteboard.PasteboardType> = [.string, .init("NSStringPboardType")]
        return types.contains(.string) && types.allSatisfy(plain.contains)
    }

    @Test
    func copyingPartOfABoldWordGivesItsLetters() {
        let view = shown("**bold**")
        #expect(copied(view, NSRange(location: 1, length: 3)) == "old")
    }

    @Test
    func copyingAcrossAMentionGivesItsName() {
        #expect(copied(shown("**bold** and <@1>")) == "bold and @mira")
    }

    @Test
    func copyingATimestampGivesItsText() {
        #expect(
            copied(shown("<t:1800000000:f>"))
                == times.text(1_800_000_000, style: .longDateShortTime, now: now))
    }

    @Test
    func copyingAHiddenSpoilerGivesItsText() {
        #expect(copied(shown("a ||secret|| b")) == "a secret b")
    }

    @Test
    func copyingLinesGivesNewlines() {
        #expect(copied(shown("one\ntwo\n```\nthree\nfour\n```")) == "one\ntwo\nthree\nfour")
    }

    @Test
    func onlyPlainTextIsWritten() {
        let view = shown("**bold** <#5>")
        #expect(view.writablePasteboardTypes == [.string])

        _ = copied(view)

        #expect(holdsOnlyPlainText, "\(pasteboard.types ?? [])")
        #expect(pasteboard.string(forType: .string) == "bold general")
    }

    // A drag writes the selection the same way, through writeSelection(to:types:).
    @Test
    func draggingASelectionWritesTheSameText() {
        let view = shown("**bold** and <@1>")
        view.setSelectedRange(NSRange(location: 2, length: 8))

        #expect(view.writeSelection(to: pasteboard, types: [.string, .rtf]))

        #expect(holdsOnlyPlainText, "\(pasteboard.types ?? [])")
        #expect(pasteboard.string(forType: .string) == "ld and @")
    }

    @Test
    func copyTextInTheMenuCopiesTheContent() throws {
        let message = named(row(1, at: now, content: "**bold** and <@1>")).message
        let items = MessageMenu.items(for: message, pasteboard: pasteboard)
        let copy = try #require(items.first { $0.title == "Copy Text" })

        let menu = NSMenu()
        menu.addItem(copy)
        menu.performActionForItem(at: 0)

        #expect(pasteboard.string(forType: .string) == "**bold** and <@1>")
    }

    @Test
    func copyTextIsntOfferedWithoutText() {
        let message = row(1, at: now, content: "", attachments: ["a.png"]).message

        #expect(MessageMenu.items(for: message, pasteboard: pasteboard).isEmpty)
    }

    @Test
    func theTextsMenuOffersCopyText() throws {
        let view = shown("hello")
        view.messageMenu = { MessageMenu.items(for: row(1, at: self.now).message) }
        let event = try #require(
            NSEvent.mouseEvent(
                with: .rightMouseDown, location: .zero, modifierFlags: [], timestamp: 0,
                windowNumber: 0, context: nil, eventNumber: 0, clickCount: 1, pressure: 1))

        let menu = try #require(view.menu(for: event))

        #expect(menu.items.contains { $0.title == "Copy Text" })
    }
}
