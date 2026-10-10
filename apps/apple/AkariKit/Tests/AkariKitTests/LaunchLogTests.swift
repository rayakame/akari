import Testing

@testable import AkariKit

struct LaunchLogTests {
    @Test
    func eachMilestoneIsLoggedOnce() {
        let name = "test milestone \(UInt64.random(in: 0...UInt64.max))"

        #expect(LaunchLog.mark(name))
        #expect(!LaunchLog.mark(name))
        #expect(LaunchLog.sinceProcessStart() ?? -1 >= 0)
    }
}
