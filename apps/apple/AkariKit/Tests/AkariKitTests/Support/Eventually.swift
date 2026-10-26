import Foundation
import Testing

/// Polls `condition` on the main actor until it holds, at most 5 s.
@MainActor
func eventually(
    _ condition: @MainActor () -> Bool, sourceLocation: SourceLocation = #_sourceLocation
) async throws {
    let deadline = ContinuousClock.now + .seconds(5)
    while !condition() {
        if ContinuousClock.now > deadline {
            Issue.record("the condition never held", sourceLocation: sourceLocation)
            return
        }
        try await Task.sleep(for: .milliseconds(5))
    }
}
