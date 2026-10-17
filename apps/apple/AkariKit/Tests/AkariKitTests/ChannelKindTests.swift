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

    @Test(arguments: [
        (ChannelType.newsThread, true), (.publicThread, true), (.privateThread, true),
        (.guildText, false), (.guildForum, false), (.dm, false), (.unknown(19), false),
    ])
    func announcementPublicAndPrivateThreadsAreThreads(kind: ChannelType, isThread: Bool) {
        #expect(channel(1, kind: kind).isThread == isThread)
    }
}
