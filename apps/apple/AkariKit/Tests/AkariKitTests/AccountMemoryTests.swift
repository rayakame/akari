import AkariKit
import Foundation
import Testing

struct AccountMemoryTests {
    @Test
    func accountMemoryRemembersAndForgets() throws {
        let suite = "app.akari.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let memory = AccountMemory(defaults: defaults)
        let user = UserId(rawValue: 18_446_744_073_709_551_615)

        #expect(memory.lastAccount == nil)
        memory.lastAccount = user
        #expect(AccountMemory(defaults: defaults).lastAccount == user)
        memory.lastAccount = nil
        #expect(AccountMemory(defaults: defaults).lastAccount == nil)
    }
}
