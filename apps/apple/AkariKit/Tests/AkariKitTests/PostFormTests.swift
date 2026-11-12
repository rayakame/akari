import AkariFFI
import Foundation
import Testing

@testable import AkariKit

// Guild 1 with forum 13, whose tags are Bug (501), Idea (502) and Staff (503, moderated); the
// user may post there and apply Bug and Idea. The session is on the forum's page.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class PostFormTests {
    let store = FakeStore()
    let account: FakeAccount
    let forumId: ChannelId = id(13)
    let bug: TagId = id(501)
    let idea: TagId = id(502)
    let staff: TagId = id(503)

    init() {
        account = FakeAccount(store: store)
        world()
    }

    func world(
        kind: ChannelType = .guildForum, allowed: Bool = true, requiresTag: Bool = false,
        template: String? = nil, usable: [TagId]? = nil
    ) {
        var forum = channel(13, kind: kind, name: "ideas")
        forum.forum = ForumSettings(
            tags: [
                ForumTag(id: bug, name: "Bug", moderated: false, emoji: nil),
                ForumTag(id: idea, name: "Idea", moderated: false, emoji: nil),
                ForumTag(id: staff, name: "Staff", moderated: true, emoji: nil),
            ],
            defaultReaction: nil, sort: .latestActivity, layout: .list, tagMatching: .some,
            postSlowmode: 0, template: template)
        let listed = ForumPosts(active: [], archived: [], older: .all, newPosts: 0)
        store.update { state in
            state.connection = .online
            state.currentUser = user(1)
            state.add(guild(1))
            state.list([channel(11), forum], in: 1)
            state.permissions[id(13)] = [.viewChannel, .readMessageHistory, .sendMessages]
            state.forumPages[id(13)] = { _ in listed }
            state.postCreations[id(13)] = PostCreation(
                allowed: allowed, tags: usable ?? [self.bug, self.idea], requiresTag: requiresTag)
            state.windows[id(11)] = window([1])
            state.show(message(1, in: 11))
        }
    }

    func inForum() -> SessionModel {
        let session = SessionModel(
            userId: id(1), account: account, memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: forumId)
        return session
    }

    func form(_ session: SessionModel) throws -> PostFormModel {
        try #require(session.forum?.form)
    }

    func newPost(_ raw: UInt64 = 71) -> Channel {
        var post = thread(raw, in: 13, name: "made")
        post.thread?.ownerId = id(1)
        return post
    }

    var creates: [CallLog.Call] {
        account.log.actions.filter { if case .createPost = $0 { true } else { false } }
    }

    // MARK: Opening and closing

    @Test
    func newPostOpensTheFormWithWhatTheFieldHeld() throws {
        let session = inForum()
        let page = try #require(session.forum)
        page.setQuery("dark mode")

        page.openForm()

        #expect(page.form.isOpen)
        #expect(page.form.title == "dark mode")
        #expect(page.query.isEmpty, "the search ends")
        #expect(page.search == .none)
        session.close()
    }

    @Test
    func withoutPermissionNothingOpens() throws {
        world(allowed: false)
        let session = inForum()
        let page = try #require(session.forum)

        page.openForm()

        #expect(!page.form.isOpen)
        #expect(!page.form.canCreate)
        session.close()
    }

    // Akari has no uploads, which a media post needs.
    @Test
    func aMediaChannelsFormDoesNotOpen() throws {
        world(kind: .guildMedia)
        let session = inForum()
        let page = try #require(session.forum)

        page.openForm()

        #expect(page.form.isMedia)
        #expect(!page.form.canCreate)
        #expect(!page.form.isOpen)
        session.close()
    }

    @Test
    func escapeClosesTheFormAndKeepsWhatWasTyped() throws {
        let session = inForum()
        let form = try form(session)
        session.forum?.openForm()
        form.title = "A title"
        form.message = "Some text"
        form.toggle(tag: bug)

        form.close()

        #expect(!form.isOpen)
        #expect(form.hasDraft)
        #expect(form.title == "A title")
        #expect(form.message == "Some text")
        #expect(form.tags == [bug])
        session.close()
    }

    @Test
    func theXDropsEverything() throws {
        world(template: "Steps:")
        let session = inForum()
        let form = try form(session)
        session.forum?.openForm()
        form.title = "A title"
        form.message = "Some text"
        form.toggle(tag: bug)

        form.discard()

        #expect(!form.isOpen)
        #expect(!form.hasDraft)
        #expect(form.title.isEmpty)
        #expect(form.message == "Steps:", "back to the template")
        #expect(form.tags.isEmpty)
        session.close()
    }

    @Test
    func theDraftStaysWithTheForumForTheSession() throws {
        let session = inForum()
        session.forum?.openForm()
        try form(session).title = "A title"
        try form(session).message = "Some text"
        try form(session).toggle(tag: idea)

        session.open(channel: id(11))
        session.open(channel: forumId)

        let form = try form(session)
        #expect(form.title == "A title")
        #expect(form.message == "Some text")
        #expect(form.tags == [idea])
        #expect(form.isOpen, "a message begun earlier opens the form with the page")
        session.close()
    }

    // MARK: The fields

    @Test
    func theTitleIsOneLineOfAHundredCharacters() throws {
        let session = inForum()
        let form = try form(session)

        form.title = "one\ntwo"
        #expect(form.title == "one two")
        form.title = String(repeating: "x", count: 120)
        #expect(form.title.count == 100)
        session.close()
    }

    @Test
    func theMessageStartsAsTheTemplateWhichIsNoDraft() throws {
        world(template: "  Steps to reproduce:\n")
        let session = inForum()
        let form = try form(session)

        #expect(form.message == "Steps to reproduce:")
        #expect(!form.hasDraft)
        #expect(!form.isOpen)
        session.close()
    }

    @Test
    func atMostFiveTagsAndOnlyThoseTheUserMayApply() throws {
        let many = (601...606).map { id($0) as TagId }
        world(usable: many)
        store.update { state in
            state.channels[id(13)]?.forum?.tags = many.map {
                ForumTag(id: $0, name: "t\($0.rawValue)", moderated: false, emoji: nil)
            }
        }
        let session = inForum()
        let form = try form(session)
        #expect(form.usableTags.count == 6)

        for tag in many {
            form.toggle(tag: tag)
        }
        form.toggle(tag: staff)

        #expect(form.tags == Array(many.prefix(5)), "in the order chosen")
        #expect(!form.canChoose(many[5]))
        #expect(form.canChoose(many[0]), "a chosen one can go")
        form.toggle(tag: many[0])
        #expect(form.canChoose(many[5]))
        session.close()
    }

    @Test
    func moderatedTagsAreOfferedOnlyToThoseWhoMayApplyThem() throws {
        let session = inForum()
        #expect(try form(session).usableTags.map(\.name) == ["Bug", "Idea"])

        store.update { $0.postCreations[id(13)]?.tags = [self.bug, self.idea, self.staff] }
        try form(session).apply(EventBatch([.channelUpdated(channelId: id(13), guildId: id(1))]))

        #expect(try form(session).usableTags.map(\.name) == ["Bug", "Idea", "Staff"])
        session.close()
    }

    // MARK: Post

    func filled(_ session: SessionModel) throws -> PostFormModel {
        let form = try form(session)
        session.forum?.openForm()
        form.title = "  A title "
        form.message = " Some text\n"
        return form
    }

    @Test
    func postIsOnOnceThereIsATitleAndAMessage() throws {
        let session = inForum()
        let form = try form(session)
        session.forum?.openForm()
        #expect(!form.canPost)
        form.title = "A title"
        #expect(!form.canPost)
        form.message = "Some text"
        #expect(form.canPost)
        form.title = "   "
        #expect(!form.canPost)
        session.close()
    }

    @Test
    func aRequiredTagHoldsPostBack() async throws {
        world(requiresTag: true)
        let session = inForum()
        let form = try filled(session)

        #expect(form.tagMissing)
        #expect(!form.canPost)
        await session.submitPost()
        #expect(creates.isEmpty)

        form.toggle(tag: bug)
        #expect(!form.tagMissing)
        #expect(form.canPost)
        session.close()
    }

    @Test
    func postingSendsWhatWasTypedTrimmedWithTheTagsInOrder() async throws {
        account.createReply.withLock { $0 = .success(newPost()) }
        let session = inForum()
        let form = try filled(session)
        form.toggle(tag: idea)
        form.toggle(tag: bug)

        await session.submitPost()

        #expect(creates == [.createPost(forumId, "A title", [idea, bug], "Some text")])
        session.close()
    }

    @Test
    func aCreatedPostOpensBesideTheForumAndTheFormStartsOver() async throws {
        world(template: "Steps:")
        account.createReply.withLock { $0 = .success(newPost()) }
        let session = inForum()
        let form = try filled(session)

        await session.submitPost()

        #expect(session.forum?.forumId == forumId)
        #expect(session.split?.channelId == id(71))
        #expect(account.log.actions.contains(.refreshForum(forumId)), "its card joins the list")
        #expect(!form.isOpen)
        #expect(form.title.isEmpty)
        #expect(form.message == "Steps:")
        #expect(!form.hasDraft)
        session.close()
    }

    @Test
    func aPostCreatedAfterTheUserLeftOpensNothing() async throws {
        account.createReply.withLock { $0 = .success(newPost()) }
        account.holdCreates.withLock { $0 = true }
        let session = inForum()
        _ = try filled(session)

        let submit = Task { await session.submitPost() }
        try await eventually { !self.creates.isEmpty }
        session.open(channel: id(11))
        account.createReplies.send(())
        await submit.value

        #expect(session.messages?.channelId == id(11))
        #expect(session.split == nil)
        session.close()
    }

    @Test
    func emptyFieldsSayWhatIsMissingAndSendNothing() async throws {
        let session = inForum()
        let form = try form(session)
        session.forum?.openForm()

        await session.submitPost()

        #expect(form.titleError == "A title is required")
        #expect(form.messageError == "A message is required")
        #expect(creates.isEmpty)
        form.title = "A title"
        #expect(form.titleError == nil, "typing ends it")
        form.message = "text"
        #expect(form.messageError == nil)
        session.close()
    }

    @Test
    func theTemplateAloneMayBePosted() async throws {
        world(template: "Steps:")
        account.createReply.withLock { $0 = .success(newPost()) }
        let session = inForum()
        let form = try form(session)
        session.forum?.openForm()
        form.title = "A title"

        await session.submitPost()

        #expect(creates == [.createPost(forumId, "A title", [], "Steps:")])
        session.close()
    }

    @Test
    func aMessageOverTheLimitIsNotSent() async throws {
        store.update { $0.lengthLimit = 10 }
        let session = inForum()
        let form = try filled(session)
        form.message = String(repeating: "x", count: 11)

        await session.submitPost()

        #expect(creates.isEmpty)
        #expect(form.messageError != nil)
        session.close()
    }

    @Test(arguments: [
        (CreateThreadError.TooManyThreads(announcement: false), "Cannot create post"),
        (.Request(error: .RateLimited(retryAfter: 1)), "Cannot create post"),
        (.MaybeCreated, "Post may exist"),
        (.Request(error: .InvalidRequest), "Error"),
    ])
    func aRefusalKeepsTheFormAndSaysWhy(error: CreateThreadError, title: String) async throws {
        account.createReply.withLock { $0 = .failure(error) }
        let session = inForum()
        let form = try filled(session)

        await session.submitPost()

        #expect(form.alert?.title == title)
        #expect(form.isOpen)
        #expect(form.title == "  A title ", "what was typed stays")
        #expect(session.split == nil)
        form.dismissAlert()
        #expect(form.alert == nil)
        session.close()
    }

    @Test
    func autoModsWordsStandUnderTheirField() async throws {
        let session = inForum()
        let form = try filled(session)

        account.createReply.withLock { $0 = .failure(.NameBlocked(reason: "No ads in titles")) }
        await session.submitPost()
        #expect(form.titleError == "No ads in titles")

        account.createReply.withLock { $0 = .failure(.MessageBlocked(reason: "")) }
        await session.submitPost()
        #expect(form.messageError == ThreadFormModel.blocked)
        #expect(form.alert == nil)
        session.close()
    }

    @Test
    func slowmodeHoldsPostBackUntilItEnds() async throws {
        let session = inForum()
        let form = try filled(session)
        store.update {
            $0.threadSlowmodes[id(13)] = Slowmode(
                interval: 60, exempt: false, until: Date().addingTimeInterval(60))
        }
        form.apply(EventBatch([.channelUpdated(channelId: id(13), guildId: id(1))]))

        #expect(form.slowmode?.until != nil)
        #expect(!form.canPost)
        await session.submitPost()
        #expect(creates.isEmpty)
        session.close()
    }

    @Test
    func losingThePermissionClosesTheForm() throws {
        let session = inForum()
        let form = try filled(session)

        store.update { $0.postCreations[id(13)]?.allowed = false }
        form.apply(EventBatch([.channelUpdated(channelId: id(13), guildId: id(1))]))

        #expect(!form.isOpen)
        #expect(form.title == "  A title ", "kept, should the permission come back")
        session.close()
    }

    // Review: the form is the forum's for the session, so a create in flight is still the
    // form's when the user comes back.
    @Test
    func leavingAndReturningDuringACreateCannotPostTwice() async throws {
        account.createReply.withLock { $0 = .success(newPost()) }
        account.holdCreates.withLock { $0 = true }
        let session = inForum()
        let form = try filled(session)

        let submit = Task { await session.submitPost() }
        try await eventually { !self.creates.isEmpty }
        session.open(channel: id(11))
        session.open(channel: forumId)

        let shown = try self.form(session)
        #expect(shown === form)
        #expect(shown.submitting)
        #expect(!shown.canPost)
        await session.submitPost()
        #expect(creates.count == 1)

        account.createReplies.send(())
        await submit.value
        #expect(!shown.isOpen)
        #expect(shown.title.isEmpty)
        #expect(session.split?.channelId == id(71), "the forum is open again: its post opens")
        session.close()
    }

    @Test
    func aPostCreatedAfterTheUserLeftRefreshesNothing() async throws {
        account.createReply.withLock { $0 = .success(newPost()) }
        account.holdCreates.withLock { $0 = true }
        let session = inForum()
        _ = try filled(session)

        let submit = Task { await session.submitPost() }
        try await eventually { !self.creates.isEmpty }
        session.open(channel: id(11))
        account.createReplies.send(())
        await submit.value

        #expect(!account.log.actions.contains(.refreshForum(forumId)))
        session.close()
    }

    // Review: the template itself is no draft, also after typing and deleting back to it.
    @Test
    func typingBackToTheTemplateLeavesNoDraft() throws {
        world(template: "Steps:")
        let session = inForum()
        try form(session).message = "Steps: 1"
        try form(session).message = "Steps:"
        #expect(try !form(session).hasDraft)

        world(template: "New steps:")
        try form(session).apply(EventBatch([.channelUpdated(channelId: id(13), guildId: id(1))]))

        #expect(try form(session).message == "New steps:")
        #expect(try !form(session).hasDraft)
        session.close()
    }

    // The session's forms follow their forum while its page is closed.
    @Test
    func aFormReadsItsRulesAgainWhenItsPageComesBack() throws {
        let session = inForum()
        #expect(try form(session).usableTags.count == 2)
        session.open(channel: id(11))
        store.update { $0.postCreations[id(13)]?.tags = [self.bug] }

        session.open(channel: forumId)

        #expect(try form(session).usableTags.map(\.name) == ["Bug"])
        session.close()
    }
}
