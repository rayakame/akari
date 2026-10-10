import AkariKit
import AppKit
import Testing

@testable import Akari

struct ChannelSymbolsTests {
    @Test(arguments: [
        ChannelType.guildText, .dm, .guildVoice, .groupDm, .guildCategory, .guildNews,
        .guildStore, .newsThread, .publicThread, .privateThread, .guildStageVoice,
        .guildDirectory, .guildForum, .guildMedia, .lobby, .ephemeralDm, .unknown(99),
    ])
    func everyKindHasASymbolThatExists(kind: ChannelType) {
        let name = ChannelSymbols.name(for: kind)

        #expect(NSImage(systemSymbolName: name, accessibilityDescription: nil) != nil, "\(name)")
    }

    @Test
    func theBadgeExistsAndTextChannelsUseANumberSign() {
        #expect(
            NSImage(systemSymbolName: ChannelSymbols.nsfwBadge, accessibilityDescription: nil)
                != nil)
        #expect(ChannelSymbols.name(for: .guildText) == "number")
        #expect(ChannelSymbols.name(for: .guildVoice) != ChannelSymbols.name(for: .guildText))
    }
}
