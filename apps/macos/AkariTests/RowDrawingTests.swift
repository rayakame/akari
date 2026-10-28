import AkariKit
import AppKit
import Testing

@testable import Akari

// Rows draw with layers rather than bitmaps: a drawn bitmap of the placeholder block, at
// least one and a half views tall, was most of the launch's memory peak.
@MainActor
final class RowDrawingTests {
    // The controller of a table-hosted test, alive while the test reads its cells.
    var hold: MessageTableController?
    let window = TestWindow.make(size: NSSize(width: 800, height: 700))

    func shown(_ view: NSView, appearance: NSAppearance.Name = .darkAqua) -> NSView {
        window.appearance = NSAppearance(named: appearance)
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 800, height: 700))
        container.wantsLayer = true
        window.contentView = container
        view.frame = NSRect(x: 0, y: 0, width: 800, height: 600)
        container.addSubview(view)
        window.layoutIfNeeded()
        view.layoutSubtreeIfNeeded()
        window.displayIfNeeded()
        return view
    }

    func placeholders(failure: String? = nil) -> PlaceholderCell {
        let cell = PlaceholderCell()
        cell.translatesAutoresizingMaskIntoConstraints = true
        cell.show(.older, height: 600, failure: failure)
        return cell
    }

    // The layer tree as Core Animation composites it, read by pixel from the top left.
    struct LayerImage {
        let rep: NSBitmapImageRep

        init(_ view: NSView) throws {
            let layer = try #require(view.layer)
            let size = view.bounds.size
            rep = try #require(
                NSBitmapImageRep(
                    bitmapDataPlanes: nil, pixelsWide: Int(size.width),
                    pixelsHigh: Int(size.height),
                    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
                    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0))
            let context = try #require(NSGraphicsContext(bitmapImageRep: rep))
            view.effectiveAppearance.performAsCurrentDrawingAppearance {
                layer.render(in: context.cgContext)
            }
        }

        func alpha(_ x: Int, _ y: Int) -> CGFloat {
            rep.colorAt(x: x, y: y)?.alphaComponent ?? 0
        }

        func rgb(_ x: Int, _ y: Int) -> [CGFloat] {
            guard let color = rep.colorAt(x: x, y: y) else {
                return []
            }
            return [color.redComponent, color.greenComponent, color.blueComponent]
        }
    }

    // A view that implements draw(_:) gets a bitmap backing as large as itself.
    static func drawsItsOwnBitmap(_ type: NSView.Type, over base: NSView.Type) -> Bool {
        let draw = #selector(NSView.draw(_:))
        return class_getMethodImplementation(type, draw)
            != class_getMethodImplementation(base, draw)
    }

    @Test
    func placeholdersDrawNoBitmap() {
        #expect(!Self.drawsItsOwnBitmap(PlaceholderCell.self, over: NSTableCellView.self))
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func placeholdersShowTheirShapes(_ appearance: NSAppearance.Name) throws {
        let image = try LayerImage(shown(placeholders(), appearance: appearance))

        // The first row (66 tall) has a circle at 16...56 and a name bar from 72, 20 below its
        // top; nothing between them. The shapes are hoverBackground, which is translucent.
        #expect(image.alpha(36, 38) > 0.05)
        #expect(image.alpha(100, 26) > 0.05)
        #expect(image.alpha(64, 38) < 0.01)
        #expect(image.alpha(36, 5) < 0.01)
    }

    @Test
    func placeholdersFollowTheirSize() throws {
        let cell = shown(placeholders())
        // The second row's bar spans 0.8 of the text width: to 642 at 800 wide, 322 at 400.
        #expect(try LayerImage(cell).alpha(350, 79) > 0.05)

        cell.setFrameSize(NSSize(width: 400, height: 600))
        cell.layoutSubtreeIfNeeded()

        #expect(try LayerImage(cell).alpha(350, 79) < 0.01)
        #expect(try LayerImage(cell).alpha(300, 79) > 0.05)
    }

    // As the app hosts it: a row of the message table, which is flipped.
    func tableRow(_ state: MessageTableState, edge: MessageTimeline.Edge) throws -> PlaceholderCell
    {
        let controller = MessageTableController(
            calendar: { berlin }, now: { berlinTime(2026, 10, 10, 15, 0) }, pointer: { nil })
        hold = controller
        window.contentView = controller.scrollView
        window.layoutIfNeeded()
        controller.show(state)
        window.layoutIfNeeded()
        window.displayIfNeeded()
        let row = try #require(controller.timeline.items.firstIndex { $0.id == .edge(edge) })
        return try #require(
            controller.tableView.view(atColumn: 0, row: row, makeIfNecessary: false)
                as? PlaceholderCell)
    }

    // Whether any shape covers the band of rows from `top` to `bottom`, measured from the top.
    func shapes(in image: LayerImage, from top: Int, to bottom: Int) -> Bool {
        (top..<bottom).contains { y in
            stride(from: 20, to: 400, by: 4).contains { x in image.alpha(x, y) > 0.05 }
        }
    }

    @Test
    func olderPlaceholdersInATableRowStartAtTheTopAndClearTheirBottomOnAFailure() throws {
        let plain = try tableRow(MessageTableState(rows: []), edge: .older)
        let image = try LayerImage(plain)
        let height = Int(plain.bounds.height)
        #expect(image.alpha(36, 38) > 0.05)
        #expect(image.alpha(36, 5) < 0.01)
        #expect(shapes(in: image, from: height - 48, to: height))

        let failed = try tableRow(
            MessageTableState(rows: [], failedLoads: [.latest: .ServerError(status: 500)]),
            edge: .older)
        let failedImage = try LayerImage(failed)
        #expect(!shapes(in: failedImage, from: height - 48, to: height))
        #expect(failedImage.alpha(36, 38) > 0.05)
    }

    @Test
    func newerPlaceholdersInATableRowClearTheirTopOnAFailure() throws {
        let rows = (1...3).map { raw in
            row(UInt64(raw), at: berlinTime(2026, 10, 10, 12, 0) + Double(raw) * 60)
        }
        let plain = try tableRow(MessageTableState(rows: rows, atPresent: false), edge: .newer)
        #expect(shapes(in: try LayerImage(plain), from: 0, to: 48))

        let failed = try tableRow(
            MessageTableState(
                rows: rows, atPresent: false, failedLoads: [.newer: .ServerError(status: 500)]),
            edge: .newer)
        let image = try LayerImage(failed)
        #expect(!shapes(in: image, from: 0, to: 48))
        #expect(shapes(in: image, from: 48, to: 140))
    }

    // Without a layout pass in between: a theme switch changes nothing else about the cell.
    // hoverBackground is the same in both themes today, so the cell gets a color that isn't.
    @Test
    func anAppearanceChangeRecolorsShownPlaceholders() throws {
        let placeholders = placeholders()
        placeholders.color = NSColor(name: nil) { appearance in
            appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? .systemRed : .systemBlue
        }
        let cell = shown(placeholders)
        let dark = try LayerImage(cell).rgb(36, 38)

        window.appearance = NSAppearance(named: .aqua)
        let light = try LayerImage(cell).rgb(36, 38)

        #expect(dark.count == 3 && light.count == 3)
        #expect(zip(dark, light).contains { abs($0 - $1) > 2 / 255 }, "\(dark) vs \(light)")
    }

    @Test
    func aFailureLeavesItsBandEmpty() throws {
        let plain = try LayerImage(shown(placeholders()))
        let failed = try LayerImage(shown(placeholders(failure: "Couldn't load older messages.")))

        // An older block's band is its bottom 48 (552...600); the 26 row from 544 has its bar at
        // 551...563, which the band hides.
        #expect(plain.alpha(100, 557) > 0.05)
        #expect(failed.alpha(100, 557) < 0.01)
        #expect(failed.alpha(36, 516) > 0.05)
    }
}

// AppKit hands unsatisfiable constraints to visualizeConstraints(_:) when asked to; each one is
// a long log message on the frame it happens in.
@MainActor
struct ConstraintConflictTests {
    final class RecordingWindow: OutOfSightWindow {
        var conflicts = 0

        override func visualizeConstraints(_ constraints: [NSLayoutConstraint]?) {
            conflicts += 1
        }
    }

    let noon = berlinTime(2026, 10, 10, 12, 0)

    func conflicts(
        width: CGFloat = 1000, height: CGFloat = 900,
        _ steps: [(MessageTableController) -> MessageTableState]
    ) -> Int {
        let key = "NSConstraintBasedLayoutVisualizeMutuallyExclusiveConstraints"
        let arguments = UserDefaults.standard.volatileDomain(forName: UserDefaults.argumentDomain)
        UserDefaults.standard.setVolatileDomain(
            arguments.merging([key: true]) { $1 }, forName: UserDefaults.argumentDomain)
        defer {
            UserDefaults.standard.setVolatileDomain(arguments, forName: UserDefaults.argumentDomain)
        }
        let controller = MessageTableController(
            calendar: { berlin }, now: { berlinTime(2026, 10, 10, 15, 0) }, pointer: { nil })
        let window = TestWindow.hide(
            RecordingWindow(
                contentRect: NSRect(origin: .zero, size: NSSize(width: width, height: height)), styleMask: [.titled, .resizable],
                backing: .buffered, defer: false))
        window.contentView = controller.scrollView
        window.layoutIfNeeded()
        for step in steps {
            controller.show(step(controller))
            window.layoutIfNeeded()
            window.displayIfNeeded()
        }
        return window.conflicts
    }

    func rows(_ range: ClosedRange<Int>, by authorOf: (Int) -> UInt64 = { UInt64($0 % 3) })
        -> [MessageListModel.Row]
    {
        range.map { raw in
            row(
                UInt64(raw), by: author(authorOf(raw)), at: noon + Double(raw) * 60,
                content: String(repeating: "words that wrap ", count: raw % 9 * 4 + 1))
        }
    }

    // A reused cell's header goes from an app's tag to none.
    @Test
    func aHeaderLosingItsTagConflictsWithNothing() {
        let key = "NSConstraintBasedLayoutVisualizeMutuallyExclusiveConstraints"
        let arguments = UserDefaults.standard.volatileDomain(forName: UserDefaults.argumentDomain)
        UserDefaults.standard.setVolatileDomain(
            arguments.merging([key: true]) { $1 }, forName: UserDefaults.argumentDomain)
        defer {
            UserDefaults.standard.setVolatileDomain(arguments, forName: UserDefaults.argumentDomain)
        }
        let window = TestWindow.hide(
            RecordingWindow(
                contentRect: NSRect(origin: .zero, size: NSSize(width: 600, height: 200)), styleMask: [.titled],
                backing: .buffered, defer: false))
        let cell = MessageCell()
        window.contentView = cell

        for (raw, user) in [(1, author(5, "Bot", bot: true)), (2, author(1))] {
            let message = row(UInt64(raw), by: user, at: noon).message
            cell.configure(
                message, body: NSAttributedString(string: message.content), startsGroup: true,
                groupTime: "12:00", shortTime: "12:00", fullDate: "")
            window.layoutIfNeeded()
        }

        #expect(window.conflicts == 0)
    }

    @Test
    func anUnsatisfiableConstraintIsCounted() {
        #expect(
            conflicts([
                { controller in
                    let box = NSView()
                    box.translatesAutoresizingMaskIntoConstraints = false
                    controller.scrollView.addSubview(box)
                    NSLayoutConstraint.activate([
                        box.widthAnchor.constraint(equalToConstant: 10),
                        box.widthAnchor.constraint(equalToConstant: 20),
                    ])
                    return MessageTableState(rows: [])
                }
            ]) > 0)
    }

    @Test
    func theFirstPageReplacesThePlaceholdersWithoutAConflict() {
        #expect(
            conflicts([
                { _ in MessageTableState(rows: []) },
                { _ in MessageTableState(rows: rows(1...100)) },
            ]) == 0)
    }

    @Test
    func aShortChannelThatGrowsKeepsItsBeginningWithoutAConflict() {
        let plain = { (range: ClosedRange<UInt64>, long: Bool) in
            range.map { raw in
                row(
                    raw, by: author(raw % 3 == 0 ? 2 : 1), at: self.noon + Double(raw) * 120,
                    content: long ? String(repeating: "words that wrap ", count: 12) : nil)
            }
        }
        let short = plain(1...3, false)
        let four = short + plain(4...4, false)
        let long = four + plain(5...64, true)
        #expect(
            conflicts(
                width: 600, height: 400,
                [
                    { _ in MessageTableState(rows: short, reachedOldest: true) },
                    { _ in MessageTableState(rows: four, reachedOldest: true) },
                    { controller in
                        controller.scrollView.window?.setContentSize(
                            NSSize(width: 600, height: 300))
                        controller.scrollView.window?.layoutIfNeeded()
                        return MessageTableState(rows: long, reachedOldest: true)
                    },
                ]) == 0)
    }

    @Test
    func aDeletedGroupStartGivesItsPlaceWithoutAConflict() {
        let grouped = rows(1...40, by: { UInt64($0 / 4) })
        #expect(
            conflicts([
                { _ in MessageTableState(rows: grouped) },
                { _ in MessageTableState(rows: grouped.filter { $0.id.rawValue % 4 != 0 }) },
            ]) == 0)
    }
}

// Building a cell is what a fling pays when the reuse pool runs dry, and a first page pays for
// every visible row; buttons are the costly part.
@MainActor
struct CellBuildingTests {
    func buttons(in view: NSView) -> [NSButton] {
        view.subviews.flatMap { ($0 as? NSButton).map { [$0] } ?? buttons(in: $0) }
    }

    func configure(_ cell: MessageCell, _ delivery: Delivery) {
        let message = row(1, at: berlinTime(2026, 10, 10, 12, 0), delivery: delivery).message
        cell.configure(
            message, body: NSAttributedString(string: message.content), startsGroup: true,
            groupTime: "12:00", shortTime: "12:00",
            fullDate: "Saturday, 10 October 2026 at 12:00")
    }

    @Test
    func aMessageCellBuildsItsRetryButtonsOnlyForAFailedMessage() {
        let cell = MessageCell()
        configure(cell, .sent)
        #expect(buttons(in: cell).isEmpty)

        configure(cell, .failed)
        let shown = buttons(in: cell).filter { !$0.isHiddenOrHasHiddenAncestor }
        #expect(shown.map(\.title) == ["Retry", "Delete"])

        configure(cell, .sent)
        #expect(buttons(in: cell).allSatisfy { $0.isHiddenOrHasHiddenAncestor })
    }
}
