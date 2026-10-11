import AkariKit
import AppKit
import Darwin
import Testing

@testable import Akari

// Measurements, not assertions: run with TEST_RUNNER_AKARI_PROBE=1 and read the PROBE lines.
// It shows a window on screen, so it never runs in the normal suite or on CI.
@MainActor
@Suite(.enabled(if: ProcessInfo.processInfo.environment["AKARI_PROBE"] != nil))
struct PerformanceProbe {
    let noon = berlinTime(2026, 10, 10, 12, 0)

    static func footprint() -> (current: Double, peak: Double) {
        var info = task_vm_info_data_t()
        var count = mach_msg_type_number_t(
            MemoryLayout<task_vm_info_data_t>.size / MemoryLayout<natural_t>.size)
        let result = withUnsafeMutablePointer(to: &info) { pointer in
            pointer.withMemoryRebound(to: integer_t.self, capacity: Int(count)) {
                task_info(mach_task_self_, task_flavor_t(TASK_VM_INFO), $0, &count)
            }
        }
        guard result == KERN_SUCCESS else {
            return (0, 0)
        }
        return (
            Double(info.phys_footprint) / 1_048_576,
            Double(info.ledger_phys_footprint_peak) / 1_048_576
        )
    }

    // Lengths like a busy channel's: mostly one line, some paragraphs, a few attachments.
    func rows(_ range: ClosedRange<UInt64>) -> [MessageListModel.Row] {
        range.map { raw in
            let words = [1, 1, 2, 1, 6, 1, 3, 1, 14, 2][Int(raw % 10)] * 6
            return row(
                raw, by: author(raw % 4 == 0 ? 2 : raw % 7 == 0 ? 3 : 1),
                at: noon + Double(raw) * 40,
                content: String(repeating: "some words that wrap ", count: words),
                attachments: raw % 13 == 0 ? ["photo.png"] : [])
        }
    }

    struct Frames {
        var times: [Double] = []
        var landings: [Double] = []

        var summary: String {
            let sorted = times.sorted()
            let median = sorted.isEmpty ? 0 : sorted[sorted.count / 2]
            let long = times.filter { $0 > 16.7 }.count
            return String(
                format: "%d frames, median %.1f ms, longest %.1f ms, %d over 16.7 ms, landings %@",
                times.count, median, sorted.last ?? 0, long,
                landings.map { String(format: "%.1f", $0) }.joined(separator: "/"))
        }
    }

    // With AKARI_PROBE_PAUSE set, waits at each phase so `footprint` can sample this process.
    func checkpoint(_ name: String) {
        let footprint = Self.footprint()
        print(String(format: "PROBE checkpoint %@: %.1f MB", name, footprint.current))
        if let pause = ProcessInfo.processInfo.environment["AKARI_PROBE_PAUSE"].flatMap(Double.init)
        {
            Thread.sleep(forTimeInterval: pause)
        }
    }

    func milliseconds(_ body: () -> Void) -> Double {
        let start = ContinuousClock.now
        body()
        let (seconds, attoseconds) = start.duration(to: .now).components
        return Double(seconds) * 1000 + Double(attoseconds) / 1e15
    }

    @Test
    func flingThroughPagesThatLand() {
        let before = Self.footprint()
        let controller = MessageTableController(
            calendar: { berlin }, now: { berlinTime(2026, 10, 10, 15, 0) }, pointer: { nil })
        let window = NSWindow(
            contentRect: NSRect(x: 100, y: 100, width: 1000, height: 900),
            styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        window.contentView = controller.scrollView
        window.isReleasedWhenClosed = false
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }
        let clip = controller.scrollView.contentView
        func frame(_ body: () -> Void = {}) -> Double {
            milliseconds {
                body()
                window.displayIfNeeded()
                CATransaction.flush()
            }
        }
        func post(_ name: Notification.Name) {
            NotificationCenter.default.post(name: name, object: controller.scrollView)
        }

        checkpoint("window")
        let placeholders = frame { controller.show(MessageTableState(rows: [])) }
        checkpoint("placeholders")
        var shown = rows(301...400)
        let firstPage = frame { controller.show(MessageTableState(rows: shown)) }
        let afterFill = Self.footprint()
        checkpoint("first page")

        var up = Frames()
        post(NSScrollView.willStartLiveScrollNotification)
        for step in 0..<160 {
            if step == 30 || step == 80 || step == 130 {
                let first = shown.first?.id.rawValue ?? 301
                shown = rows((first - 50)...(first - 1)) + shown
                up.landings.append(frame { controller.show(MessageTableState(rows: shown)) })
            }
            up.times.append(
                frame {
                    clip.scroll(to: NSPoint(x: 0, y: max(clip.bounds.minY - 90, 0)))
                    controller.scrollView.reflectScrolledClipView(clip)
                    post(NSScrollView.didLiveScrollNotification)
                })
        }
        post(NSScrollView.didEndLiveScrollNotification)
        let afterUp = Self.footprint()
        checkpoint("fling up")

        var down = Frames()
        post(NSScrollView.willStartLiveScrollNotification)
        for _ in 0..<160 {
            down.times.append(
                frame {
                    clip.scroll(to: NSPoint(x: 0, y: clip.bounds.minY + 90))
                    controller.scrollView.reflectScrolledClipView(clip)
                    post(NSScrollView.didLiveScrollNotification)
                })
        }
        post(NSScrollView.didEndLiveScrollNotification)
        let after = Self.footprint()
        checkpoint("fling down")

        print(
            String(
                format: "PROBE placeholders %.1f ms, first page %.1f ms", placeholders, firstPage))
        print("PROBE fling up: \(up.summary)")
        print("PROBE fling down: \(down.summary)")
        print(
            String(
                format:
                    "PROBE footprint MB: before %.1f, after fill %.1f, after up %.1f, after %.1f, lifetime peak %.1f",
                before.current, afterFill.current, afterUp.current, after.current, after.peak))
    }

    // The launch's long frame: a first page replacing the placeholders, again and again, so a
    // sampler attached meanwhile sees mostly this.
    @Test
    func firstPagesRepeatedly() {
        checkpoint("first pages")
        var times: [Double] = []
        for round in 0..<30 {
            let controller = MessageTableController(
                calendar: { berlin }, now: { berlinTime(2026, 10, 10, 15, 0) }, pointer: { nil })
            let window = NSWindow(
                contentRect: NSRect(x: 100, y: 100, width: 1000, height: 900),
                styleMask: [.titled, .resizable], backing: .buffered, defer: false)
            window.isReleasedWhenClosed = false
            window.contentView = controller.scrollView
            window.orderFrontRegardless()
            controller.show(MessageTableState(rows: []))
            window.displayIfNeeded()
            let first = UInt64(round * 1000 + 1)
            times.append(
                milliseconds {
                    controller.show(MessageTableState(rows: rows(first...(first + 49))))
                    window.layoutIfNeeded()
                    window.displayIfNeeded()
                    CATransaction.flush()
                })
            window.orderOut(nil)
        }
        let sorted = times.sorted()
        print(
            String(
                format: "PROBE first pages: median %.1f ms, fastest %.1f, slowest %.1f",
                sorted[sorted.count / 2], sorted[0], sorted[sorted.count - 1]))
    }
}
