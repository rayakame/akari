import Testing

@testable import AkariKit

@MainActor
struct ChannelListModelTests {
    let store = FakeStore()

    init() {
        store.update { state in
            state.add(guild(1), guild(2))
            state.list([channel(10, kind: .guildCategory), channel(11), channel(12)], in: 1)
            state.list([channel(20, guild: 2)], in: 2)
        }
    }

    func loaded() -> ChannelListModel {
        let model = ChannelListModel(guildId: id(1), store: store)
        store.log.forget()
        return model
    }

    @Test
    func channelsFollowTheStoreList() {
        let model = ChannelListModel(guildId: id(1), store: store)

        #expect(model.channels.map(\.id) == [id(10), id(11), id(12)])
        #expect(
            store.reads == [
                .channelList(id(1)), .channels([id(10), id(11), id(12)]),
                .channelBadges([id(10), id(11), id(12)]), .mutedChannels([id(10), id(11), id(12)]),
                .guildSettings(id(1)),
            ])
    }

    @Test
    func anUpdatedChannelIsReadAlone() {
        let model = loaded()
        store.update { $0.channels[id(11)] = channel(11, name: "renamed") }

        model.apply(
            EventBatch([
                .channelUpdated(channelId: id(11), guildId: id(1)),
                .channelUpdated(channelId: id(11), guildId: id(1)),
            ])
        )

        #expect(store.reads == [.channelList(id(1)), .channels([id(11)])])
        #expect(model.channels.map(\.name) == ["channel-10", "renamed", "channel-12"])
    }

    @Test
    func eventsForOtherGuildsReadNothing() {
        let model = loaded()

        model.apply(
            EventBatch([
                .channelUpdated(channelId: id(20), guildId: id(2)),
                .currentMemberUpdated(guildId: id(2)),
                .guildUpdated(guildId: id(2)),
                .channelUpdated(channelId: id(30), guildId: nil),
                .messageInserted(channelId: id(11), messageId: id(100)),
            ])
        )

        #expect(store.reads.isEmpty)
        #expect(model.channels.map(\.id) == [id(10), id(11), id(12)])
    }

    @Test
    func aPermissionChangeRereadsTheList() {
        let model = loaded()
        store.update { state in
            state.list(
                [channel(10, kind: .guildCategory), channel(11), channel(13), channel(12)], in: 1
            )
        }

        model.apply(EventBatch([.currentMemberUpdated(guildId: id(1))]))

        #expect(model.channels.map(\.id) == [id(10), id(11), id(13), id(12)])
        #expect(
            store.reads == [
                .channelList(id(1)), .channels([id(13)]), .channelBadges([id(13)]),
                .mutedChannels([id(10), id(11), id(13), id(12)]),
            ])

        store.log.forget()
        store.update { $0.channelLists[id(1)] = [id(10), id(11), id(12)] }
        model.apply(EventBatch([.guildUpdated(guildId: id(1))]))

        #expect(model.channels.map(\.id) == [id(10), id(11), id(12)])
        #expect(store.reads == [.channelList(id(1)), .mutedChannels([id(10), id(11), id(12)])])
    }

    @Test
    func channelRowsKnowTheirMute() {
        store.update { $0.mutedChannels = [id(10), id(12)] }
        let model = loaded()
        #expect(model.muted == [id(10), id(12)])
        store.update { $0.mutedChannels = [id(11)] }

        model.apply(EventBatch([.guildSettingsChanged(guildId: id(1))]))

        #expect(model.muted == [id(11)])
        #expect(store.reads == [.mutedChannels([id(10), id(11), id(12)]), .guildSettings(id(1))])
    }

    @Test
    func settingsOfOtherGuildsReadNothing() {
        let model = loaded()

        model.apply(
            EventBatch([
                .guildSettingsChanged(guildId: id(2)), .guildSettingsChanged(guildId: nil),
            ]))

        #expect(store.reads.isEmpty)
    }

    @Test
    func hideMutedChannelsLeavesThemOut() {
        store.update { state in
            state.mutedChannels = [id(10), id(11), id(12)]
            state.guildSettings[id(1)] = settings(hideMutedChannels: true)
        }
        let model = loaded()

        #expect(model.hidesMuted)
        #expect(model.shown(open: nil).map(\.id) == [id(10)], "categories stay")
        #expect(model.shown(open: id(12)).map(\.id) == [id(10), id(12)], "the open one stays")
        store.update { $0.guildSettings[id(1)] = settings() }
        model.apply(EventBatch([.guildSettingsChanged(guildId: id(1))]))
        #expect(model.shown(open: nil).map(\.id) == [id(10), id(11), id(12)])
    }

    @Test
    func aMuteInTheSameBatchAsANewChannelIsRead() {
        let model = loaded()
        store.update { state in
            state.list(
                [channel(10, kind: .guildCategory), channel(11), channel(13), channel(12)], in: 1)
            state.mutedChannels = [id(10)]
        }

        model.apply(
            EventBatch([
                .channelAdded(channelId: id(13), guildId: id(1)),
                .guildSettingsChanged(guildId: id(1)),
            ]))

        #expect(model.muted == [id(10)])
    }

    @Test
    func channelRowsCarryTheirBadges() {
        store.update { $0.channelBadges = [id(11): badge(mentions: 2)] }

        let model = ChannelListModel(guildId: id(1), store: store)

        #expect(model.badge(of: id(11)) == badge(mentions: 2))
        #expect(model.badge(of: id(12)) == noBadge)
    }

    @Test
    func aBadgeEventRereadsOnlyItsRow() {
        let model = loaded()
        store.update { $0.channelBadges[id(12)] = badge() }

        model.apply(
            EventBatch([
                .badgeChanged(channelId: id(12), guildId: id(1)),
                .badgeChanged(channelId: id(20), guildId: id(2)),
            ]))

        #expect(store.reads == [.channelBadges([id(12)])])
        #expect(model.badge(of: id(12)) == badge())
    }

    func collapsedCategory(muted: Bool) -> [ChannelId] {
        store.update { state in
            state.list(
                [
                    channel(10, kind: .guildCategory), channel(11, parent: 10),
                    channel(12, parent: 10), channel(13, parent: 10), channel(14, parent: 10),
                    channel(15, parent: 10),
                ], in: 1)
            // The core leaves a muted channel's row read; its mentions still show.
            state.channelBadges = [
                id(11): badge(), id(12): badge(unread: false, mentions: 1), id(14): noBadge,
            ]
            state.mutedChannels = muted ? [id(10), id(14)] : [id(14)]
        }
        let model = ChannelListModel(guildId: id(1), store: store)
        let collapsed = CollapsedCategories(defaults: MemoryDefaults())
        collapsed.toggle(id(10), in: id(1))
        return collapsed.visible(model.channels, in: id(1)) {
            model.staysWhenCollapsed($0, open: id(15))
        }.map(\.id)
    }

    @Test
    func aCollapsedCategoryShowsUnreadAndMentionedChannels() {
        #expect(collapsedCategory(muted: false) == [id(10), id(11), id(12), id(15)])
    }

    @Test
    func aMutedCollapsedCategoryShowsOnlyMentions() {
        #expect(collapsedCategory(muted: true) == [id(10), id(12), id(15)])
    }

    // A category, a channel with two threads (the newest first, as the core lists them), and
    // another channel.
    func threaded(_ change: (inout FakeStore.State) -> Void = { _ in }) -> ChannelListModel {
        store.update { state in
            state.list(
                [
                    channel(10, kind: .guildCategory), channel(11, parent: 10),
                    thread(32, in: 11), thread(31, in: 11), channel(12, parent: 10),
                ], in: 1)
            change(&state)
        }
        let model = ChannelListModel(guildId: id(1), store: store)
        store.log.forget()
        return model
    }

    func collapsedRows(_ model: ChannelListModel, open: ChannelId? = nil) -> [ChannelId] {
        let collapsed = CollapsedCategories(defaults: MemoryDefaults())
        collapsed.toggle(id(10), in: id(1))
        return collapsed.visible(model.shown(open: open), in: id(1)) {
            model.staysWhenCollapsed($0, open: open)
        }.map(\.id)
    }

    @Test
    func threadsFollowTheirParentNewestFirst() {
        let model = threaded()

        #expect(model.shown(open: nil).map(\.id) == [id(10), id(11), id(32), id(31), id(12)])
    }

    @Test
    func aCollapsedCategoryKeepsUnreadThreadsWithTheirParent() {
        let model = threaded { $0.channelBadges = [id(32): badge()] }

        #expect(collapsedRows(model) == [id(10), id(11), id(32)])
    }

    @Test
    func aCollapsedMutedCategoryKeepsUnreadThreadsNotOnlyMentioned() {
        let model = threaded { state in
            state.channelBadges = [id(32): badge(), id(12): badge()]
            state.mutedChannels = [id(10)]
        }

        #expect(collapsedRows(model) == [id(10), id(11), id(32)])
    }

    @Test
    func aMutedParentHidesItsReadThreadsUntilItIsOpen() {
        let model = threaded { state in
            state.channelBadges = [id(32): badge()]
            state.mutedChannels = [id(11)]
        }

        #expect(model.shown(open: nil).map(\.id) == [id(10), id(11), id(32), id(12)])
        #expect(
            model.shown(open: id(11)).map(\.id) == [id(10), id(11), id(32), id(31), id(12)])
        #expect(
            model.shown(open: id(31)).map(\.id) == [id(10), id(11), id(32), id(31), id(12)],
            "the open thread always shows")
    }

    @Test
    func hideMutedChannelsKeepsAParentWithUnreadThreads() {
        let model = threaded { state in
            state.channelBadges = [id(32): badge()]
            state.mutedChannels = [id(11), id(12)]
            state.guildSettings[id(1)] = settings(hideMutedChannels: true)
        }

        #expect(model.shown(open: nil).map(\.id) == [id(10), id(11), id(32)])
        store.update { $0.channelBadges = [:] }
        model.apply(EventBatch([.badgeChanged(channelId: id(32), guildId: id(1))]))
        #expect(model.shown(open: nil).map(\.id) == [id(10)])
    }

    @Test
    func aMutedThreadUnderHideMutedIsHidden() {
        let model = threaded { state in
            state.channelBadges = [id(32): badge(unread: false, mentions: 1)]
            state.mutedChannels = [id(32)]
            state.guildSettings[id(1)] = settings(hideMutedChannels: true)
        }

        #expect(model.shown(open: nil).map(\.id) == [id(10), id(11), id(31), id(12)])
        #expect(
            model.shown(open: id(32)).map(\.id) == [id(10), id(11), id(32), id(31), id(12)])
    }

    @Test
    func theOpenThreadsRowMovesToTheTopWhenItStopsBeingListed() {
        let model = threaded()
        model.setOpen(id(31))
        store.update { state in
            state.channelLists[id(1)] = [id(10), id(11), id(32), id(12)]
        }

        model.apply(EventBatch([.threadListingChanged(guildId: id(1), threadId: id(31))]))

        #expect(model.channels.map(\.id) == [id(10), id(11), id(32), id(12)])
        #expect(model.shown(open: id(31)).map(\.id) == [id(10), id(11), id(31), id(32), id(12)])
    }

    @Test
    func theKeptRowGoesWhenTheUserOpensSomethingElse() {
        let model = threaded()
        model.setOpen(id(31))
        store.update { $0.channelLists[id(1)] = [id(10), id(11), id(32), id(12)] }
        model.apply(EventBatch([.threadListingChanged(guildId: id(1), threadId: id(31))]))

        model.setOpen(id(12))

        #expect(model.shown(open: id(12)).map(\.id) == [id(10), id(11), id(32), id(12)])
    }

    @Test
    func anUnlistedThreadOpenFullWidthGetsTheTopRow() {
        let model = threaded { $0.channels[id(33)] = thread(33, in: 11) }

        model.setOpen(id(33))

        #expect(store.reads == [.channels([id(33)])])
        #expect(
            model.shown(open: id(33)).map(\.id)
                == [id(10), id(11), id(33), id(32), id(31), id(12)])
    }

    @Test
    func theKeptRowsBadgeFollowsItsThread() {
        let model = threaded { $0.channelBadges = [id(31): badge(mentions: 1)] }
        model.setOpen(id(31))
        store.update { state in
            state.channelLists[id(1)] = [id(10), id(11), id(32), id(12)]
            state.channelBadges = [:]
        }

        model.apply(
            EventBatch([
                .badgeChanged(channelId: id(31), guildId: id(1)),
                .threadListingChanged(guildId: id(1), threadId: id(31)),
            ]))

        #expect(model.badge(of: id(31)) == noBadge)
    }

    @Test
    func theKeptThreadIsFoundById() {
        let model = threaded { $0.channels[id(33)] = thread(33, in: 11) }
        model.setOpen(id(33))

        #expect(model.channel(id(33))?.id == id(33))
        #expect(model.channel(id(12))?.id == id(12))
        model.setOpen(id(12))
        #expect(model.channel(id(33)) == nil)
    }

    @Test
    func aNewSessionRereadsTheKeptThread() {
        let model = threaded()
        model.setOpen(id(31))
        var renamed = thread(31, in: 11)
        renamed.name = "renamed"
        store.update { state in
            state.channelLists[id(1)] = [id(10), id(11), id(32), id(12)]
            state.channels[id(31)] = renamed
        }

        model.apply(EventBatch([.ready]))

        #expect(model.channel(id(31))?.name == "renamed")
    }

    @Test
    func aThreadsMuteChangeRereadsMuted() {
        let model = threaded()
        store.update { $0.mutedChannels = [id(32)] }

        model.apply(EventBatch([.channelUpdated(channelId: id(32), guildId: id(1))]))

        #expect(model.muted == [id(32)])
        #expect(store.reads.contains(.mutedChannels([id(32)])))
    }
}
