import AkariKit
import AppKit
import Testing

@testable import Akari

// Each row's text is built once per change and shared by the height pass and the cell (D71).
@MainActor
struct RowTextTests {
    final class Builds {
        var counts: [UInt64: Int] = [:]
    }

    let builds = Builds()
    let list: MessageListFixture

    init() {
        let builds = builds
        list = MessageListFixture { message, revealed, now, times in
            builds.counts[message.id.rawValue, default: 0] += 1
            return MessageText.build(message, revealed: revealed, now: now, times: times)
        }
    }

    func rows(_ range: ClosedRange<UInt64>) -> [MessageListModel.Row] {
        range.map { raw in
            let samples = FormattedSamples.contents
            return row(
                raw, by: author(raw % 3 + 1), at: list.noon + Double(raw) * 60,
                content: samples[Int(raw) % samples.count])
        }
    }

    var builtOnce: Bool {
        builds.counts.values.allSatisfy { $0 == 1 }
    }

    // As SwiftUI makes a list for a channel the user returns to: filled, placed, then sized;
    // until then its column has AppKit's default width.
    @Test
    func aListFilledBeforeItHasASizeBuildsNothingUntilItHasOne() throws {
        let builds = Builds()
        let controller = MessageTableController(
            calendar: { berlin }, now: { berlinTime(2026, 10, 10, 15, 0) }, pointer: { nil },
            build: { message, revealed, now, times in
                builds.counts[message.id.rawValue, default: 0] += 1
                return MessageText.build(message, revealed: revealed, now: now, times: times)
            })

        controller.show(MessageTableState(rows: rows(1...50), reachedOldest: true))
        #expect(builds.counts.isEmpty, "\(builds.counts)")

        let window = TestWindow.make(size: NSSize(width: 725, height: 431))
        let container = try #require(window.contentView)
        container.addSubview(controller.scrollView)
        #expect(builds.counts.isEmpty, "\(builds.counts)")
        controller.scrollView.frame = container.bounds
        window.layoutIfNeeded()
        #expect(builds.counts.count == 50 && builds.counts.values.allSatisfy { $0 == 1 })
    }

    @Test
    func theHeightPassAndTheCellShareOneBuild() {
        list.show(rows(51...100))
        #expect(builds.counts.count == 50 && builtOnce, "\(builds.counts)")

        list.show(rows(1...100))
        #expect(builds.counts.count == 100 && builtOnce, "\(builds.counts)")

        for row in stride(from: list.table.numberOfRows - 1, through: 0, by: -4) {
            list.table.scrollRowToVisible(row)
            list.window.layoutIfNeeded()
        }
        #expect(builtOnce, "\(builds.counts)")

        list.table.reloadData(
            forRowIndexes: IndexSet(integersIn: 0..<list.table.numberOfRows), columnIndexes: [0])
        list.window.layoutIfNeeded()
        #expect(builtOnce, "\(builds.counts)")

        var edited = rows(1...100)
        edited[59] = row(60, by: author(1), at: list.noon + 3_600, content: "now **edited**")
        list.show(edited)
        #expect(builds.counts[60] == 2)
        #expect(builds.counts.filter { $0.key != 60 }.values.allSatisfy { $0 == 1 })
    }

    @Test
    func aRevealRebuildsOnlyThatRow() {
        list.show([
            row(1, at: list.noon, content: "||one||"),
            row(2, at: list.noon + 60, content: "||two||"),
        ])
        builds.counts = [:]

        list.reveal([SpoilerKey(messageId: MessageId(rawValue: 1), index: 0)])

        #expect(builds.counts == [1: 1])
    }

    @Test
    func aNewThemeRebuildsNothing() {
        list.show(rows(1...30))
        builds.counts = [:]

        list.window.appearance = NSAppearance(named: .aqua)
        list.window.layoutIfNeeded()
        list.window.displayIfNeeded()
        list.window.appearance = NSAppearance(named: .darkAqua)
        list.window.layoutIfNeeded()
        list.window.displayIfNeeded()

        #expect(builds.counts.isEmpty)
    }
}
