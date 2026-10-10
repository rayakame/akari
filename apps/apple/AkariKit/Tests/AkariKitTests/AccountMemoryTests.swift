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

    @Test
    func spotsArePerAccountAndForgotten() throws {
        let suite = "app.akari.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let memory = AccountMemory(defaults: defaults)
        let guild = AccountMemory.Spot(
            place: .guild(GuildId(rawValue: 7)), channel: ChannelId(rawValue: 70))
        let home = AccountMemory.Spot(place: .home, channel: nil)

        memory.remember(guild, of: UserId(rawValue: 1))
        memory.remember(home, of: UserId(rawValue: 2))

        let later = AccountMemory(defaults: defaults)
        #expect(later.lastSpot(of: UserId(rawValue: 1)) == guild)
        #expect(later.lastSpot(of: UserId(rawValue: 2)) == home)
        #expect(later.lastSpot(of: UserId(rawValue: 3)) == nil)
        memory.remember(nil, of: UserId(rawValue: 1))
        #expect(later.lastSpot(of: UserId(rawValue: 1)) == nil)
    }
}
