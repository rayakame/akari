import Testing

@testable import AkariKit

@MainActor
struct LoadLogTests {
    let started = ContinuousClock.now

    func trace(
        _ kind: MessageListModel.Load = .older, report: LoadReport? = loadReport(),
        failure: RequestError? = nil
    ) -> LoadTrace {
        LoadTrace(
            sequence: 7, channelId: id(1234), kind: kind, report: report, failure: failure,
            page: [id(1), id(2)], read: .microseconds(900), started: started,
            duration: .milliseconds(412))
    }

    @Test(arguments: [
        ("load", true), ("akari_core=info,load", true), (" load ", true), ("load=info", true),
        ("loader", false), ("scroll", false), ("", false),
    ])
    func theLoadLogIsOnlyOnWhenItsCategoryIsAsked(_ filter: String, _ asked: Bool) {
        #expect(LoadLog.isAsked(by: filter) == asked)
    }

    @Test
    func aPageLineHasEveryStage() {
        let line = LoadLog.page(
            trace(), render: .microseconds(8_700), rows: 50, held: nil, shown: .milliseconds(214))

        #expect(
            line
                == "channel 1234 older page 7: queued 0.0 ms, limited 0.4 ms, headers 182 ms, "
                + "connection reused, body 12 ms, 48213 bytes, decode 2.1 ms, store 0.4 ms, "
                + "read 0.9 ms, render 8.7 ms for 50 rows, shown after 214 ms; 50 messages; "
                + "remaining 4, reset 1.2 s")
    }

    @Test
    func aHeldPageSaysHowLongItWaited() {
        let line = LoadLog.page(
            trace(), render: .microseconds(8_700), rows: 50, held: .milliseconds(520),
            shown: .milliseconds(834))

        #expect(
            line.contains("render 8.7 ms for 50 rows, held 520 ms for live scroll, shown after"))
    }

    @Test
    func aPageLineSaysWhatItDidntGet() {
        let line = LoadLog.page(
            trace(
                .latest, report: loadReport(newConnection: true, remaining: nil, resetAfter: nil)),
            render: nil, rows: 0, held: nil, shown: .milliseconds(300))

        #expect(line.contains("latest page 7"))
        #expect(line.contains("new connection"))
        #expect(line.contains("render none (no rows changed)"))
        #expect(line.hasSuffix("50 messages; no limit headers"))
    }

    @Test
    func aFailedLoadLineHasItsKindAndNoStages() {
        let line = LoadLog.failure(trace(report: nil, failure: .RateLimited(retryAfter: 3)))

        #expect(line == "channel 1234 older page 7 failed after 412 ms: rate limited")
    }

    @Test
    func discordErrorsKeepOnlyTheirCode() {
        let line = LoadLog.failure(
            trace(
                report: nil,
                failure: .Discord(status: 403, code: 50001, message: "Missing Access to secret")))

        #expect(line.hasSuffix("failed after 412 ms: Discord error 50001"))
        #expect(!line.contains("secret"))
    }

    @Test
    func anOpenedLineSaysWhereTheRowsCameFrom() {
        #expect(
            LoadLog.opened(id(1234), from: .window, after: .milliseconds(18))
                == "channel 1234 opened: rows from the window after 18 ms")
        #expect(
            LoadLog.opened(id(1234), from: .network, after: .milliseconds(402))
                == "channel 1234 opened: rows from the network after 402 ms")
    }
}
