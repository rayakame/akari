import AkariKit
import AppKit
import Testing

@testable import Akari

// The log is global, and these tests suspend while a frame is committed.
@MainActor @Suite(.serialized)
struct LoadLineTests {
    final class Lines {
        var all: [String] = []
    }

    let noon = berlinTime(2026, 10, 10, 12, 0)
    let lines = Lines()
    let controller: MessageTableController
    let window: NSWindow
    let opened = ContinuousClock.now

    init() {
        controller = MessageTableController(
            calendar: { berlin }, now: { berlinTime(2026, 10, 10, 15, 0) }, pointer: { nil })
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 600, height: 400),
            styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        window.contentView = controller.scrollView
        window.layoutIfNeeded()
    }

    // Sets the log up for one test and puts it back afterwards; the log is global.
    func logging(enabled: Bool = true, _ body: () async -> Void) async {
        let wasEnabled = LoadLog.enabled
        let write = LoadLog.write
        LoadLog.enabled = enabled
        LoadLog.write = { [lines] in lines.all.append($0) }
        await body()
        LoadLog.enabled = wasEnabled
        LoadLog.write = write
    }

    func rows(_ range: ClosedRange<UInt64>, name: String = "Mira", content: String? = nil)
        -> [MessageListModel.Row]
    {
        range.map { raw in
            row(
                raw, by: author(raw % 3 == 0 ? 2 : 1, name), at: noon + Double(raw) * 120,
                content: content)
        }
    }

    func trace(
        _ sequence: Int, started: ContinuousClock.Instant, failure: RequestError? = nil,
        report: LoadReport? = loadReport()
    ) -> LoadTrace {
        LoadTrace(
            sequence: sequence, channelId: ChannelId(rawValue: 10), kind: .older,
            report: failure == nil ? report : nil, failure: failure, read: .microseconds(900),
            started: started, duration: .milliseconds(412))
    }

    func state(
        _ rows: [MessageListModel.Row], load: LoadTrace? = nil,
        from origin: MessageListModel.Origin = .network
    ) -> MessageTableState {
        MessageTableState(
            rows: rows, channelId: ChannelId(rawValue: 10), lastLoad: load, openedAt: opened,
            openedFrom: origin)
    }

    func show(_ state: MessageTableState) {
        controller.show(state)
        window.layoutIfNeeded()
        CATransaction.flush()
    }

    // The line waits for the frame's commit, whose completion reaches the main queue only
    // once this test lets go of it.
    func settle(until done: (() -> Bool)? = nil) async {
        guard let done else {
            try? await Task.sleep(for: .milliseconds(200))
            return
        }
        let deadline = ContinuousClock.now + .seconds(1)
        repeat {
            try? await Task.sleep(for: .milliseconds(10))
        } while !done() && ContinuousClock.now < deadline
    }

    func pageLines(_ sequence: Int) -> [String] {
        lines.all.filter { $0.contains("older page \(sequence)") }
    }

    @Test
    func aLandingPageLogsOneLineWithEveryStage() async {
        await logging {
            show(state(rows(50...99)))
            await settle { !lines.all.isEmpty }
            let started = ContinuousClock.now

            show(state(rows(1...99), load: trace(1, started: started)))
            await settle { !pageLines(1).isEmpty }

            #expect(pageLines(1).count == 1)
            let line = pageLines(1).first ?? ""
            for stage in [
                "queued", "limited", "headers", "connection reused", "body", "bytes", "decode",
                "store", "read", "render", "shown after", "50 messages",
            ] {
                #expect(line.contains(stage), "\(stage) missing in \(line)")
            }
            #expect(!line.contains("render none"))
        }
    }

    @Test
    func aLoadIsLoggedOnceWhateverRendersAfterIt() async {
        await logging {
            show(state(rows(50...99)))
            let load = trace(1, started: .now)
            show(state(rows(1...99), load: load))
            await settle { !pageLines(1).isEmpty }

            show(state(rows(1...99), load: load))
            show(state(rows(1...100), load: load))
            await settle()

            #expect(pageLines(1).count == 1)
        }
    }

    @Test
    func aLoadWhoseRowsCameEarlierUsesThatRender() async {
        await logging {
            show(state(rows(50...99)))
            let started = ContinuousClock.now
            show(state(rows(1...99)))
            await settle()

            show(state(rows(1...99), load: trace(1, started: started)))
            await settle { !pageLines(1).isEmpty }

            #expect(pageLines(1).first?.contains("render none") == false)
        }
    }

    @Test
    func aLoadThatChangedNoRowsSaysSo() async {
        await logging {
            show(state(rows(50...99)))
            await settle()

            show(state(rows(50...99), load: trace(1, started: .now)))
            await settle { !pageLines(1).isEmpty }

            #expect(pageLines(1).first?.contains("render none (no rows changed)") == true)
        }
    }

    @Test
    func aFailedLoadLogsItsKindAndNoStages() async {
        await logging {
            show(state(rows(50...99)))

            show(
                state(
                    rows(50...99), load: trace(1, started: .now, failure: .ServerError(status: 500))
                ))
            await settle()

            #expect(
                pageLines(1) == ["channel 10 older page 1 failed after 412 ms: server error 500"])
        }
    }

    @Test
    func channelOpensSayWhereTheRowsCameFrom() async {
        await logging {
            show(state(rows(50...99), from: .window))
            await settle { lines.all.contains { $0.contains("opened") } }
            show(state(rows(50...100), from: .window))
            await settle()

            let opens = lines.all.filter { $0.contains("opened") }
            #expect(opens.count == 1)
            #expect(
                opens.first?.hasPrefix("channel 10 opened: rows from the window after ") == true)
        }
    }

    @Test
    func aChannelLoadedFromTheNetworkSaysSo() async {
        await logging {
            show(state([]))
            show(state(rows(50...99), load: trace(1, started: opened)))
            await settle { lines.all.contains { $0.contains("opened") } }

            #expect(
                lines.all.contains {
                    $0.hasPrefix("channel 10 opened: rows from the network after ")
                })
        }
    }

    @Test
    func theLoadLogHoldsNumbersAndIdsOnly() async {
        await logging {
            show(state(rows(50...99, name: "Mira-Name", content: "secret-content-77")))
            show(
                state(
                    rows(1...99, name: "Mira-Name", content: "secret-content-77"),
                    load: trace(1, started: .now)))
            await settle { !pageLines(1).isEmpty }

            #expect(!lines.all.isEmpty)
            for line in lines.all {
                #expect(!line.contains("Mira-Name"))
                #expect(!line.contains("secret-content-77"))
            }
        }
    }

    @Test
    func nothingIsLoggedWhileTheCategoryIsOff() async {
        await logging(enabled: false) {
            show(state(rows(50...99)))
            show(state(rows(1...99), load: trace(1, started: .now)))
            await settle()

            #expect(lines.all.isEmpty)
        }
    }
}
