import Observation

/// A guild's channel list.
@MainActor @Observable
public final class ChannelListModel {
    public let guildId: GuildId
    /// Display order; categories are channels of kind `.guildCategory`.
    public private(set) var channels: [Channel] = []
    /// Channels and categories muted now by their own settings; a muted category doesn't
    /// mute its channels.
    public private(set) var muted: Set<ChannelId> = []
    /// The user hides muted channels in this guild.
    public private(set) var hidesMuted = false
    /// What each channel's row shows; channels without one show nothing.
    public private(set) var badges: [ChannelId: Badge] = [:]
    // The thread open full width, which keeps a row while the core doesn't list it.
    private var openThread: Channel?

    @ObservationIgnored private let store: Store

    init(guildId: GuildId, store: Store) {
        self.guildId = guildId
        self.store = store
        reload(rereading: nil)
        readSettings()
    }

    /// The list as shown, as the official client shows it:
    /// - the open thread (see `setOpen(_:)`) keeps a row at the top of its parent's threads
    ///   while it isn't listed;
    /// - a muted channel that isn't open shows only its unread and mentioned threads;
    /// - with "hide muted channels", muted channels and threads are left out, except `open` and
    ///   a channel with threads that show. Categories stay.
    public func shown(open: ChannelId?) -> [Channel] {
        var list = channels
        if let kept = openThread, kept.id == open, !list.contains(where: { $0.id == kept.id }),
            let parent = list.firstIndex(where: { $0.id == kept.parentId })
        {
            list.insert(kept, at: parent + 1)
        }
        let threads = Set(
            list.filter { $0.isThread && showsThread($0, open: open) }.compactMap(\.parentId))
        return list.filter { channel in
            if channel.isThread {
                return showsThread(channel, open: open)
            }
            return !hidesMuted || channel.kind == .guildCategory || channel.id == open
                || !muted.contains(channel.id) || threads.contains(channel.id)
        }
    }

    /// A listed channel, or the open thread while it keeps its row.
    public func channel(_ id: ChannelId) -> Channel? {
        channels.first { $0.id == id } ?? openThread.flatMap { $0.id == id ? $0 : nil }
    }

    // The channel open full width, so an open thread keeps its row in `shown(open:)`.
    func setOpen(_ channel: ChannelId?) {
        guard channel != openThread?.id else {
            return
        }
        openThread = channel.flatMap { id in
            channels.first { $0.id == id } ?? store.channels(ids: [id]).first
        }.flatMap { $0.isThread ? $0 : nil }
        if let kept = openThread?.id, !channels.contains(where: { $0.id == kept }) {
            readBadges([kept])
            muted.remove(kept)
            muted.formUnion(store.mutedChannels(ids: [kept]))
        }
    }

    // The listed channels and the kept open thread: every row that can show.
    private var rowIds: [ChannelId] {
        var ids = channels.map(\.id)
        if let kept = openThread?.id, !ids.contains(kept) {
            ids.append(kept)
        }
        return ids
    }

    private func showsThread(_ thread: Channel, open: ChannelId?) -> Bool {
        if thread.id == open {
            return true
        }
        if hidesMuted && muted.contains(thread.id) {
            return false
        }
        guard let parent = thread.parentId, parent != open, muted.contains(parent) else {
            return true
        }
        return Self.isPending(badge(of: thread.id))
    }

    private static func isPending(_ badge: Badge) -> Bool {
        badge.unread || badge.mentions > 0
    }

    public func badge(of channel: ChannelId) -> Badge {
        badges[channel] ?? Badge(unread: false, mentions: 0)
    }

    /// Whether a channel stays in view under its collapsed category: the open one, one with
    /// mentions, and an unread one unless its category is muted. A thread stays when it's
    /// unread too, muted category or not, and keeps its channel with it.
    public func staysWhenCollapsed(_ channel: Channel, open: ChannelId?) -> Bool {
        let badge = badge(of: channel.id)
        if channel.isThread {
            return channel.id == open || Self.isPending(badge)
        }
        return channel.id == open || badge.mentions > 0
            || (badge.unread && !(channel.parentId.map(muted.contains) ?? false))
    }

    func apply(_ batch: EventBatch) {
        if batch.everythingChanged {
            reload(rereading: nil)
            readSettings()
            return
        }
        let changed = batch.channelsChanged[guildId]
        // Member and role changes can show or hide channels.
        if changed != nil || batch.membersChanged.contains(guildId)
            || batch.guildsChanged.contains(guildId)
        {
            reload(rereading: changed ?? [])
        }
        if batch.guildSettingsChanged.contains(guildId) {
            readSettings()
        }
        if let changed = batch.badgesChanged[guildId] {
            readBadges(rowIds.filter(changed.contains))
        }
    }

    // `nil` re-reads every channel, e.g. after a new session.
    private func reload(rereading changed: Set<ChannelId>?) {
        let ids = store.channelList(guildId: guildId)
        var held = Dictionary(channels.map { ($0.id, $0) }) { first, _ in first }
        let unread = ids.filter { id in
            held[id] == nil || changed?.contains(id) != false
        }
        if !unread.isEmpty {
            for channel in store.channels(ids: unread) {
                held[channel.id] = channel
            }
        }
        let listed = channels.map(\.id)
        channels = ids.compactMap { held[$0] }
        let known = Set(listed)
        readBadges(changed == nil ? rowIds : unread.filter { !known.contains($0) })
        if changed != nil, channels.map(\.id) != listed {
            muted = Set(store.mutedChannels(ids: rowIds))
        } else if changed != nil {
            // A thread's own mute comes with its channel update.
            let threads = unread.filter { held[$0]?.isThread == true }
            if !threads.isEmpty {
                muted.subtract(threads)
                muted.formUnion(store.mutedChannels(ids: threads))
            }
        }
        if let kept = openThread, changed?.contains(kept.id) != false {
            openThread =
                channels.first { $0.id == kept.id } ?? store.channels(ids: [kept.id]).first ?? kept
        }
    }

    private func readBadges(_ ids: [ChannelId]) {
        guard !ids.isEmpty else {
            return
        }
        for read in store.channelBadges(ids: ids) {
            badges[read.channelId] = read.badge
        }
    }

    private func readSettings() {
        muted = Set(store.mutedChannels(ids: rowIds))
        hidesMuted = store.guildSettings(guildId: guildId).hideMutedChannels
    }
}
