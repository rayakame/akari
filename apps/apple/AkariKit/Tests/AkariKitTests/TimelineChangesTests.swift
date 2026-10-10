import Foundation
import Testing

@testable import AkariKit

struct TimelineChangesTests {
    let noon = berlinTime(2026, 10, 10, 12, 0)

    func items(_ rows: [MessageListModel.Row]) -> [MessageTimeline.Item] {
        MessageTimeline(rows: rows, calendar: berlin).items
    }

    @Test
    func appendingAMessageInsertsOnlyItsRow() {
        let first = [row(1, at: noon)]

        let changes = TimelineChanges(
            from: items(first), to: items(first + [row(2, at: noon + 60)]))

        #expect(changes.removed.isEmpty)
        #expect(changes.inserted == [2])
        #expect(changes.reloaded.isEmpty)
        #expect(!changes.isEmpty)
    }

    @Test
    func aConfirmationReloadsExactlyOneRow() {
        let before = [row(1, at: noon), row(50, at: noon + 30, delivery: .pending)]
        let after = [row(1, at: noon), row(60, at: noon + 30, key: 50)]

        let changes = TimelineChanges(from: items(before), to: items(after))

        #expect(changes.removed.isEmpty)
        #expect(changes.inserted.isEmpty)
        #expect(changes.reloaded == [2])
    }

    @Test
    func deletingAGroupsFirstMessageReloadsTheNextOne() {
        let both = [row(1, at: noon), row(2, at: noon + 60)]

        let changes = TimelineChanges(from: items(both), to: items([row(2, at: noon + 60)]))

        #expect(changes.removed == [1])
        #expect(changes.inserted.isEmpty)
        #expect(changes.reloaded == [1])
    }

    @Test
    func aNewDaysFirstMessageInsertsItsDividerToo() {
        let first = [row(1, at: noon)]

        let changes = TimelineChanges(
            from: items(first), to: items(first + [row(2, at: noon + 86_400)]))

        #expect(changes.inserted == [2, 3])
        #expect(changes.removed.isEmpty)
        #expect(changes.reloaded.isEmpty)
    }

    @Test
    func nothingChangedIsEmpty() {
        let same = items([row(1, at: noon)])

        #expect(TimelineChanges(from: same, to: same).isEmpty)
    }

    @Test
    func changesTurnTheOldKeysIntoTheNewOnes() {
        var random = SplitMix64(seed: 0xA4A1)
        var next: UInt64 = 1000
        var rows = (1...60).map {
            row(UInt64($0), by: user(UInt64($0 % 3)), at: noon + Double($0) * 90)
        }
        var last = noon + 60 * 90

        for _ in 0..<500 {
            var edited = rows
            switch random.next() % 5 {
            case 0:
                next += 1
                last += Double(random.next() % 900)
                edited.append(row(next, by: user(random.next() % 3), at: last))
            case 1 where !edited.isEmpty:
                edited.remove(at: Int(random.next() % UInt64(edited.count)))
            case 2:
                edited.removeFirst(min(edited.count, Int(random.next() % 10)))
            case 3 where !edited.isEmpty:
                let index = Int(random.next() % UInt64(edited.count))
                let old = edited[index]
                next += 1
                edited[index] = row(
                    next, by: old.message.author, at: old.message.timestamp, key: old.id.rawValue)
            default:
                next += 1
                last += 86_400
                edited.append(row(next, at: last))
            }
            let old = items(rows)
            let new = items(edited)

            let changes = TimelineChanges(from: old, to: new)

            var ids = old.map(\.id)
            for index in changes.removed.reversed() {
                ids.remove(at: index)
            }
            for index in changes.inserted {
                ids.insert(new[index].id, at: index)
            }
            #expect(ids == new.map(\.id))
            let kept = Set(old.map(\.id))
            #expect(changes.reloaded.allSatisfy { kept.contains(new[$0].id) })
            #expect(changes.reloaded.allSatisfy { !changes.inserted.contains($0) })
            rows = edited
        }
    }
}

// Deterministic, so a failure can be replayed.
struct SplitMix64 {
    private var state: UInt64

    init(seed: UInt64) {
        state = seed
    }

    mutating func next() -> UInt64 {
        state &+= 0x9E37_79B9_7F4A_7C15
        var z = state
        z = (z ^ (z >> 30)) &* 0xBF58_476D_1CE4_E5B9
        z = (z ^ (z >> 27)) &* 0x94D0_49BB_1331_11EB
        return z ^ (z >> 31)
    }
}
