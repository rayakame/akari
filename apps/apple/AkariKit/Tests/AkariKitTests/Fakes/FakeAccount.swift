import AkariKit
import Foundation

private let pageReport = loadReport()

final class FakeAccount: Account, @unchecked Sendable {
    let fakeStore: FakeStore
    let connectError = Locked<GatewayError?>(nil)
    // While set, each load waits for a reply: `nil` succeeds, an error fails it.
    let holdLoads = Locked(false)
    let loadReplies = AsyncQueue<RequestError?>()
    let loadError = Locked<RequestError?>(nil)
    // The window a successful load leaves in the store, as akari-core's load would.
    let windowAfterLoad = Locked<MessageWindow?>(nil)
    // Wins over `windowAfterLoad`: the window each load leaves; `nil` drops the page, as the
    // core does when the window was replaced meanwhile.
    let windowAfter = Locked<(@Sendable (MessageLoad) -> MessageWindow?)?>(nil)
    // What a successful load reports, its outcome set as the core would; `nil` is a load with
    // nothing to load.
    let loadReport = Locked<LoadReport?>(pageReport)
    // Like akari-core: latest and around loads and supersedes place a channel's window, in the
    // order of their placings. An around page that something placed after is dropped, and so is
    // a latest page that would replace the window when the newest placing is a later around load.
    private let placings = Locked<[ChannelId: (newest: UInt64, around: UInt64)]>([:])
    private let reserved = Locked<UInt64>(0)
    // Set: an around page counts as in the window before any later request, as when the core
    // applied it before the list heard.
    let aroundLandsFirst = Locked(false)
    // While set, each load waits here before its body runs, as on Akari's runtime.
    let holdLoadEntry = Locked(false)
    let loadEntries = AsyncQueue<Void>()
    // Fails sends and retries.
    let sendError = Locked<RequestError?>(nil)
    // While set, each send waits for a reply: `nil` succeeds, an error fails it.
    let holdSends = Locked(false)
    let sendReplies = AsyncQueue<RequestError?>()
    let readingCalls = Locked<[[ChannelId]]>([])
    let markedRead = Locked<[ChannelId]>([])
    let markedGuildsRead = Locked<[[GuildId]]>([])
    // The read state a mark as read leaves in the store, as akari-core's local ack would.
    let readStateAfterMarkRead = Locked<ReadState?>(nil)
    // The same for setting a channel as being read.
    let readStateAfterReading = Locked<ReadState?>(nil)

    init(store: FakeStore = FakeStore()) {
        fakeStore = store
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    var log: CallLog { fakeStore.log }

    override func store() -> Store {
        fakeStore
    }

    override func connect() throws {
        log.append(.connect)
        if let error = connectError.current {
            throw error
        }
    }

    override func disconnect() {
        log.append(.disconnect)
    }

    // Like akari-core: the closed state is the subscription's last event.
    override func close() {
        log.append(.close)
        fakeStore.subscription.send(.connection(state: .closed(error: nil)))
        fakeStore.subscription.end()
    }

    override func viewChannel(channelId: ChannelId) {
        log.append(.view(channelId))
    }

    override func setReading(channelIds: [ChannelId]) {
        readingCalls.withLock { $0.append(channelIds) }
        if let state = readStateAfterReading.current {
            fakeStore.update { store in
                for channelId in channelIds {
                    store.readStates[channelId] = state
                }
            }
        }
    }

    override func markRead(channelId: ChannelId) {
        markedRead.withLock { $0.append(channelId) }
        if let state = readStateAfterMarkRead.current {
            fakeStore.update { $0.readStates[channelId] = state }
        }
    }

    override func markGuildsRead(guildIds: [GuildId]) {
        markedGuildsRead.withLock { $0.append(guildIds) }
    }

    override func loadMessages(channelId: ChannelId, load: MessageLoad, placing: UInt64?)
        async throws -> LoadReport?
    {
        log.append(.load(channelId, load))
        if holdLoadEntry.current {
            _ = await loadEntries.next()
        }
        let placing: UInt64? =
            switch load {
            case .latest: place(channelId, at: placing ?? reservePlacing())
            case .around: place(channelId, at: placing ?? reservePlacing(), around: true)
            default: nil
            }
        let error = holdLoads.current ? await loadReplies.next() ?? nil : loadError.current
        if let error {
            throw error
        }
        guard var report = loadReport.current else {
            return nil
        }
        let landedFirst: Bool =
            if case .around = load { aroundLandsFirst.current } else { false }
        if let placing, !landedFirst, overtaken(load, placing: placing, in: channelId) {
            report.outcome = .overtaken
            return report
        }
        report.outcome = .applied
        if let after = windowAfter.current {
            guard let window = after(load) else {
                report.outcome = .outdated
                return report
            }
            fakeStore.update { $0.windows[channelId] = window }
        } else if let window = windowAfterLoad.current {
            fakeStore.update { $0.windows[channelId] = window }
        }
        return report
    }

    override func supersedeAroundLoads(channelId: ChannelId) {
        log.append(.supersede(channelId))
        place(channelId, at: reservePlacing())
    }

    override func reservePlacing() -> UInt64 {
        reserved.withLock { reserved in
            reserved += 1
            return reserved
        }
    }

    private func overtaken(_ load: MessageLoad, placing: UInt64, in channel: ChannelId) -> Bool {
        let placed = placings.current[channel, default: (0, 0)]
        switch load {
        case .around:
            return placing < placed.newest
        case .latest:
            var replaces = false
            fakeStore.update { state in
                replaces = state.windows[channel].map { !$0.latest || $0.stale } ?? false
            }
            return replaces && placed.newest == placed.around && placed.around > placing
        default:
            return false
        }
    }

    @discardableResult
    private func place(_ channel: ChannelId, at placing: UInt64, around: Bool = false) -> UInt64 {
        placings.withLock { placings in
            var placed = placings[channel, default: (0, 0)]
            placed.newest = max(placed.newest, placing)
            if around {
                placed.around = max(placed.around, placing)
            }
            placings[channel] = placed
            return placing
        }
    }

    // Refuses queueing, as the core does before anything is queued.
    let queueError = Locked<RequestError?>(nil)
    // Runs inside queueMessage, so a test can look at the caller's state at that moment.
    let onQueue = Locked<(@Sendable () -> Void)?>(nil)

    override func queueMessage(channelId: ChannelId, content: String) throws -> MessageId {
        onQueue.current?()
        if let error = queueError.current {
            throw error
        }
        log.append(.send(channelId, content))
        return id(900)
    }

    override func deliverMessage(channelId: ChannelId, pendingId: MessageId) async throws
        -> MessageId
    {
        log.append(.deliver(channelId, pendingId))
        let held = holdSends.current ? await sendReplies.next() ?? nil : nil
        if let error = held ?? sendError.current {
            throw error
        }
        return id(901)
    }

    override func sendMessage(channelId: ChannelId, content: String) async throws -> MessageId {
        log.append(.send(channelId, content))
        let held = holdSends.current ? await sendReplies.next() ?? nil : nil
        if let error = held ?? sendError.current {
            throw error
        }
        return id(900)
    }

    override func retryMessage(channelId: ChannelId, pendingId: MessageId) async throws
        -> MessageId
    {
        log.append(.retry(channelId, pendingId))
        if let error = sendError.current {
            throw error
        }
        return id(901)
    }

    override func discardMessage(channelId: ChannelId, pendingId: MessageId) {
        log.append(.discard(channelId, pendingId))
    }

    // The threads a fetch or check finds, put into the store as akari-core would; Discord refuses
    // the rest, which also leave the store.
    let fetchedThreads = Locked<[ChannelId: Channel]>([:])
    // Found, but in a guild the store doesn't have: akari-core answers false without refusing.
    let unplacedThreads = Locked<Set<ChannelId>>([])
    let threadFetchError = Locked<RequestError?>(nil)
    // While set, each thread fetch or check waits for one value here.
    let holdThreadFetches = Locked(false)
    let threadFetches = AsyncQueue<Void>()

    override func fetchThread(parentId: ChannelId, threadId: ChannelId) async throws -> Bool {
        log.append(.fetchThread(parentId, threadId))
        return try await askForThread(threadId)
    }

    override func checkThread(parentId: ChannelId, threadId: ChannelId) async throws -> Bool {
        log.append(.checkThread(parentId, threadId))
        return try await askForThread(threadId)
    }

    private func askForThread(_ threadId: ChannelId) async throws -> Bool {
        if holdThreadFetches.current {
            _ = await threadFetches.next()
        }
        if let error = threadFetchError.current {
            throw error
        }
        if unplacedThreads.current.contains(threadId) {
            return false
        }
        guard let thread = fetchedThreads.current[threadId] else {
            fakeStore.update { state in
                state.unavailableThreads.insert(threadId)
                state.channels[threadId] = nil
            }
            return false
        }
        fakeStore.update { $0.channels[threadId] = thread }
        return true
    }

    // Thrown by the next joins and leaves; without one the store has the thread joined or left.
    // What a page of older threads leaves in the store, as akari-core would.
    let browserAfterOlderLoad = Locked<ThreadBrowser?>(nil)
    let olderLoadError = Locked<RequestError?>(nil)
    // A search's answer: Discord's IDs, `nil` while not indexed; an error fails it.
    let searchReply = Locked<Result<[ChannelId]?, RequestError>>(.success([]))
    // Answers for particular names, before `searchReply`.
    let searchReplies = Locked<[String: [ChannelId]]>([:])
    // While set, each search waits here before answering.
    let holdSearches = Locked(false)
    let searchEntries = AsyncQueue<Void>()

    override func loadOlderThreads(parentId: ChannelId) async throws {
        log.append(.loadOlderThreads(parentId))
        if let error = olderLoadError.current {
            throw error
        }
        if let browser = browserAfterOlderLoad.current {
            fakeStore.update { $0.threadBrowsers[parentId] = browser }
        }
    }

    override func searchThreads(parentId: ChannelId, name: String) async throws -> [ChannelId]? {
        log.append(.searchThreads(parentId, name))
        if holdSearches.current {
            _ = await searchEntries.next()
        }
        if let reply = searchReplies.current[name] {
            return reply
        }
        return try searchReply.current.get()
    }

    override func setBrowsing(parentId: ChannelId?) {
        log.append(.setBrowsing(parentId))
    }

    let membershipError = Locked<RequestError?>(nil)

    override func joinThread(threadId: ChannelId, location: ThreadLocation) async throws {
        log.append(.joinThread(threadId, location))
        try changeMembership(threadId, joined: true)
    }

    override func leaveThread(threadId: ChannelId, location: ThreadLocation) async throws {
        log.append(.leaveThread(threadId, location))
        try changeMembership(threadId, joined: false)
    }

    private func changeMembership(_ thread: ChannelId, joined: Bool) throws {
        if let error = membershipError.current {
            throw error
        }
        fakeStore.update { $0.channels[thread]?.thread?.joined = joined }
    }

    // While set, each cache flush waits for one value here.
    let holdFlushes = Locked(false)
    let flushes = AsyncQueue<Void>()

    override func flushCache() async {
        log.append(.flushCache)
        if holdFlushes.current {
            _ = await flushes.next()
        }
    }
}
