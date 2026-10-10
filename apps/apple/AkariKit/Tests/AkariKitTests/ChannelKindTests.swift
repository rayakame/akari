import Testing

@testable import AkariKit

struct ChannelKindTests {
    @Test(arguments: [
        (ChannelType.guildText, true), (.guildNews, true), (.dm, true), (.groupDm, true),
        (.newsThread, true), (.publicThread, true), (.privateThread, true),
        (.guildVoice, false), (.guildCategory, false), (.guildStore, false),
        (.guildStageVoice, false), (.guildDirectory, false), (.guildForum, false),
        (.guildMedia, false), (.lobby, false), (.ephemeralDm, false), (.unknown(99), false),
    ])
    func onlyTextLikeChannelsOpenAMessageList(kind: ChannelType, opens: Bool) {
        #expect(channel(1, kind: kind).opensMessageList == opens)
    }
}
