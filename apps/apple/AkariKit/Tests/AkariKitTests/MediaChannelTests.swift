import AkariFFI
import Foundation
import Testing

@testable import AkariKit

// Guild 1 with text channel 11 and media channel 14, whose default layout Discord leaves at
// "not set" and whose post is 61.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class MediaChannelTests {
    let store = FakeStore()
    let account: FakeAccount
    let mediaId: ChannelId = id(14)

    init() {
        account = FakeAccount(store: store)
        var media = channel(14, kind: .guildMedia, name: "clips")
        media.forum = ForumSettings(
            tags: [], defaultReaction: nil, sort: .latestActivity, layout: .list, tagMatching: .some,
            postSlowmode: 0, template: nil)
        var post = thread(61, in: 14, name: "a clip")
        post.thread?.joined = false
        let listed = ForumPosts(active: [id(61)], archived: [], older: .all, newPosts: 0)
        store.update { state in
            state.connection = .online
            state.currentUser = user(1)
            state.add(guild(1))
            state.list([channel(11), media], in: 1)
            state.channels[id(61)] = post
            state.permissions[id(14)] = [.viewChannel, .readMessageHistory]
            state.forumPages[id(14)] = { _ in listed }
            state.windows[id(11)] = window([1])
            state.windows[id(61)] = window([6])
            state.show(message(1, in: 11), message(6, in: 61))
        }
    }

    func inMedia() -> SessionModel {
        let session = SessionModel(
            userId: id(1), account: account, memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: mediaId)
        return session
    }

    @Test
    func aMediaChannelOpensItsPageAsAForumDoes() throws {
        let session = inMedia()

        let page = try #require(session.forum)
        #expect(page.forumId == mediaId)
        #expect(page.isMedia)
        #expect(page.posts.map(\.id) == [id(61)])
        #expect(account.log.actions.contains(.openForum(mediaId)))
        session.close()
    }

    @Test
    func itsPageIsAGalleryWhateverIsChosen() throws {
        let session = inMedia()
        let page = try #require(session.forum)
        #expect(page.settings.layout == .gallery)

        page.setLayout(.list)
        #expect(page.settings.layout == .gallery)
        page.resetToDefault()
        #expect(page.settings.layout == .gallery)

        session.open(channel: id(11))
        session.open(channel: mediaId)
        #expect(session.forum?.settings.layout == .gallery)
        session.close()
    }

    @Test
    func aForumIsNotMedia() throws {
        store.update { state in
            state.list([channel(11), channel(13, kind: .guildForum)], in: 1)
        }
        let session = inMedia()
        session.open(channel: id(13))

        #expect(session.forum?.isMedia == false)
        session.close()
    }

    @Test
    func aMediaPostOpensBesideItsChannel() async {
        let session = inMedia()

        await session.openBeside(thread: id(61), from: mediaId)

        #expect(session.forum?.forumId == mediaId)
        #expect(session.split?.channelId == id(61))
        #expect(session.split?.postBar != nil)
        session.close()
    }
}
