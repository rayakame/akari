import Foundation

/// How to turn one timeline into the next with row updates.
public struct TimelineChanges: Equatable, Sendable {
    /// Indexes in the old items; remove these first.
    public let removed: IndexSet
    /// Indexes in the new items; insert these after removing.
    public let inserted: IndexSet
    /// Indexes in the new items of kept items whose content changed, or that started or stopped
    /// coming right after a day divider.
    public let reloaded: IndexSet

    public var isEmpty: Bool {
        removed.isEmpty && inserted.isEmpty && reloaded.isEmpty
    }

    public init(from old: [MessageTimeline.Item], to new: [MessageTimeline.Item]) {
        var removed = IndexSet()
        var inserted = IndexSet()
        for change in new.map(\.id).difference(from: old.map(\.id)) {
            switch change {
            case .remove(let offset, _, _): removed.insert(offset)
            case .insert(let offset, _, _): inserted.insert(offset)
            }
        }
        let oldIndexes = Dictionary(old.enumerated().map { ($1.id, $0) }) { first, _ in first }
        // A day divider's first message sits closer to it, so a message that starts or stops
        // following one changes too.
        func followsDay(_ items: [MessageTimeline.Item], _ index: Int) -> Bool {
            guard index > 0, case .day = items[index - 1] else {
                return false
            }
            return true
        }
        var reloaded = IndexSet()
        for (index, item) in new.enumerated() where !inserted.contains(index) {
            if let oldIndex = oldIndexes[item.id],
                old[oldIndex] != item || followsDay(old, oldIndex) != followsDay(new, index)
            {
                reloaded.insert(index)
            }
        }
        self.removed = removed
        self.inserted = inserted
        self.reloaded = reloaded
    }
}
