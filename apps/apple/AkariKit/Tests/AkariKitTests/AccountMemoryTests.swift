import AkariKit
import Foundation
import Testing

struct AccountMemoryTests {
    @Test
    func accountMemoryRemembersAndForgets() {
        let defaults = MemoryDefaults()
        let memory = AccountMemory(defaults: defaults)
        let user = UserId(rawValue: 18_446_744_073_709_551_615)

        #expect(memory.lastAccount == nil)
        memory.lastAccount = user
        #expect(AccountMemory(defaults: defaults).lastAccount == user)
        memory.lastAccount = nil
        #expect(AccountMemory(defaults: defaults).lastAccount == nil)
    }

    @Test
    func spotsArePerAccountAndForgotten() {
        let defaults = MemoryDefaults()
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

    // The thread beside each channel in the split view, IDs only.
    @Test
    func splitsArePerAccountAndForgotten() {
        let defaults = MemoryDefaults()
        let memory = AccountMemory(defaults: defaults)
        let splits = [
            ChannelId(rawValue: 11): ChannelId(rawValue: 31),
            ChannelId(rawValue: 18_446_744_073_709_551_615): ChannelId(rawValue: 32),
        ]

        memory.remember(splits: splits, of: UserId(rawValue: 1))

        let later = AccountMemory(defaults: defaults)
        #expect(later.splits(of: UserId(rawValue: 1)) == splits)
        #expect(later.splits(of: UserId(rawValue: 2)).isEmpty)
        memory.forget(UserId(rawValue: 1))
        #expect(later.splits(of: UserId(rawValue: 1)).isEmpty)
        memory.remember(splits: splits, of: UserId(rawValue: 1))
        memory.remember(splits: [:], of: UserId(rawValue: 1))
        #expect(defaults.keys.allSatisfy { !$0.contains("split") }, "an empty map leaves no key")
    }
}
