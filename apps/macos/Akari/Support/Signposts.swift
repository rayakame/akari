import OSLog

// Points of Interest in Instruments, so Time Profiler and Animation Hitches line up with the
// table's own work. Free until a recording asks for them; names and no data.
enum Signposts {
    static let signposter = OSSignposter(subsystem: "app.akari", category: .pointsOfInterest)

    static func interval<T>(_ name: StaticString, _ body: () throws -> T) rethrows -> T {
        let state = signposter.beginInterval(name)
        defer { signposter.endInterval(name, state) }
        return try body()
    }

    static func event(_ name: StaticString) {
        signposter.emitEvent(name)
    }
}
