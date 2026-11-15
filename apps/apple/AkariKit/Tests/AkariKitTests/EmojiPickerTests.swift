import Foundation
import Testing

@testable import AkariKit

// Channel 11 in guild 1 with message 100; guild 2 is another of the user's servers.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class EmojiPickerTests {
    let store = FakeStore()
    let account: FakeAccount
    let reactions: ReactionsModel
    let here: ChannelId = id(11)
    let it: MessageId = id(100)
    var changes: [EmojiPickerModel.Change] = []
    var closes = 0
    static let faces = ["😀", "😃", "😄", "😁", "😆", "😅", "🤣", "😂", "🙂", "🙃", "😉"]

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.add(guild(1, name: "Home"), guild(2, name: "Other"))
            state.list([channel(11)], in: 1)
            state.allow(reacting(), message: 100, in: 11)
            state.recentReactions[id(11)] = Self.faces.map(pickerEmoji)
            state.emojiSections[id(11)] = [
                EmojiSection(
                    guildId: id(1),
                    emojis: [
                        customEmoji(7, "wave"), customEmoji(8, "cat", animated: true),
                        customEmoji(9, "gone", access: .unavailable),
                    ]),
                EmojiSection(
                    guildId: id(2),
                    emojis: [customEmoji(20, "blob", in: 2, access: .nitroLocked), customEmoji(21, "ok", in: 2)]),
            ]
        }
        reactions = ReactionsModel(store: store, account: account)
    }

    func row(_ raw: UInt64 = 100, delivery: Delivery = .sent) -> MessageListModel.Row {
        MessageListModel.Row(id: id(raw), message: message(raw, in: 11, delivery: delivery))
    }

    func picker(from location: ReactionLocation = .picker) throws -> EmojiPickerModel {
        let picker = try #require(reactions.openPicker(for: row(), from: location))
        picker.onChange = { [unowned self] in changes.append($0) }
        picker.onClose = { [unowned self] in closes += 1 }
        return picker
    }

    func at(_ section: Int, _ item: Int) -> EmojiPickerModel.Position {
        EmojiPickerModel.Position(section: section, item: item)
    }

    var calls: [CallLog.Call] { store.log.actions }

    // MARK: Sections

    @Test
    func recentlyUsedComesFirstThenTheServersThenUnicodesGroups() throws {
        let picker = try picker()

        #expect(
            picker.sections.map(\.kind)
                == [.recent, .server(id(1)), .server(id(2))]
                + EmojiCatalog.groups.map { .unicode($0.group) })
        #expect(
            picker.sections.map(\.title)
                == [
                    "Frequently Used", "Home", "Other", "Smileys & Emotion", "People & Body",
                    "Animals & Nature", "Food & Drink", "Travel & Places", "Activities", "Objects",
                    "Symbols", "Flags",
                ])
        #expect(store.reads.contains(.recentReactions(here, EmojiCatalog.newest)))
        #expect(store.reads.contains(.customEmojiSections(here)))
    }

    @Test
    func unicodesGroupsAreTheCatalogsUpToWhatThisSystemDraws() throws {
        let picker = try picker()
        let unicode = picker.sections.filter {
            if case .unicode = $0.kind { true } else { false }
        }

        #expect(unicode.map { $0.items.map(\.glyph) } == EmojiCatalog.groups.map { $0.emojis.map(\.emoji) })
        #expect(unicode.first?.items.first?.name == ":grinning:")
    }

    @Test
    func withNothingUsedThereIsNoFrequentlyUsed() throws {
        store.update { $0.recentReactions[id(11)] = [] }

        #expect(try picker().sections.first?.kind == .server(id(1)))
    }

    @Test
    func aServersSectionHasItsIconAndEachSectionItsSymbol() throws {
        store.update { $0.guilds[id(1)]?.icon = .guildIcon(guildId: id(1), hash: "abc") }
        let sections = try picker().sections

        #expect(sections[1].icon == .guildIcon(guildId: id(1), hash: "abc"))
        #expect(sections[1].symbol == nil)
        #expect(sections[0].symbol == "clock")
        #expect(
            sections.dropFirst(3).map(\.symbol)
                == [
                    "face.smiling", "hand.wave", "pawprint", "fork.knife", "car", "basketball", "lightbulb",
                    "heart", "flag",
                ])
    }

    @Test
    func anItemSaysWhatItIsAndWhyItIsLocked() throws {
        let server = try picker().sections[1].items

        #expect(server.map(\.name) == [":wave:", ":cat:", ":gone:"])
        #expect(server.map(\.isLocked) == [false, false, true])
        #expect(server[0].key == .emoji(emojiId: id(7), animated: false))
        #expect(server[1].animated)
        #expect(server[0].glyph == nil)
        #expect(server[2].reason == "Unavailable: its server lost the boosts it needs")
        #expect(try picker().sections[2].items[0].reason == "Needs Nitro to be used here")
        #expect(server[0].reason == nil)
    }

    @Test
    func voiceOverReadsAnItemsNameWithoutColonsAndUnderscores() throws {
        let picker = try picker()
        let unicode = EmojiPickerModel.Item(pickerEmoji("😍"))

        #expect(picker.sections[1].items[0].spokenName == "wave")
        #expect(unicode.name == ":heart_eyes:")
        #expect(unicode.spokenName == "heart eyes")
    }

    @Test
    func theDetailUnderTheNameIsTheReasonOrTheServer() throws {
        let picker = try picker()

        #expect(picker.detail(of: picker.sections[1].items[0]) == "Home")
        #expect(picker.detail(of: picker.sections[1].items[2]) == "Unavailable: its server lost the boosts it needs")
        #expect(picker.detail(of: picker.sections[0].items[0]) == nil)
    }

    @Test
    func theFirstEmojiIsSelectedWhenItOpens() throws {
        let picker = try picker()

        #expect(picker.selection == at(0, 0))
        #expect(picker.selected?.glyph == "😀")
    }

    // MARK: Opening and closing

    @Test
    func noPickerWhereTheUserMayNotReactWithANewEmoji() {
        store.update { $0.allow(reacting(react: false), message: 100, in: 11) }
        #expect(reactions.openPicker(for: row(), from: .picker) == nil)

        store.update { $0.allow(reacting(), message: 101, in: 11) }
        #expect(reactions.openPicker(for: row(101, delivery: .failed), from: .picker) == nil)
    }

    @Test
    func noPickerWithTheSettingOff() {
        store.update { $0.displaySettings.renderReactions = false }

        #expect(ReactionsModel(store: store, account: account).openPicker(for: row(), from: .picker) == nil)
    }

    @Test
    func openingAnotherClosesTheFirst() throws {
        let first = try picker()

        let second = try #require(reactions.openPicker(for: row(), from: .inlineButton))

        #expect(!first.isOpen)
        #expect(closes == 1)
        #expect(reactions.picker === second)
    }

    @Test
    func closingTellsTheHostOnceAndLetsGo() throws {
        let picker = try picker()

        picker.close()
        picker.close()

        #expect(!picker.isOpen)
        #expect(closes == 1)
        #expect(reactions.picker == nil)
    }

    @Test
    func theMessagesDeletionClosesIt() throws {
        let picker = try picker()

        reactions.apply(EventBatch([.messageDeleted(channelId: id(11), messageId: id(101))]))
        #expect(picker.isOpen)

        reactions.apply(EventBatch([.messageDeleted(channelId: here, messageId: it)]))
        #expect(!picker.isOpen)
        #expect(closes == 1)
    }

    @Test
    func theChannelOrServerGoingClosesIt() throws {
        let first = try picker()
        reactions.apply(EventBatch([.channelRemoved(channelId: here, guildId: id(1))]))
        #expect(!first.isOpen)

        let second = try picker()
        reactions.apply(EventBatch([.guildRemoved(guildId: id(1))]))
        #expect(!second.isOpen)
    }

    @Test
    func turningTheSettingOffClosesIt() throws {
        let picker = try picker()
        store.update { $0.displaySettings.renderReactions = false }

        reactions.apply(EventBatch([.displaySettingsChanged]))

        #expect(!picker.isOpen)
    }

    // MARK: Moving the selection

    @Test
    func rightAndLeftGoThroughTheSectionsInOrder() throws {
        let picker = try picker()

        picker.select(at(0, 8))
        picker.move(.right)
        #expect(picker.selection == at(0, 9))
        picker.select(at(0, 10))
        picker.move(.right)
        #expect(picker.selection == at(1, 0))
        picker.move(.left)
        #expect(picker.selection == at(0, 10))
        picker.select(at(0, 0))
        picker.move(.left)
        #expect(picker.selection == at(0, 0))
    }

    @Test
    func downKeepsTheColumnAndStopsAtAShorterRowsEnd() throws {
        let picker = try picker()

        picker.select(at(0, 1))
        picker.move(.down)
        #expect(picker.selection == at(0, 10))
        picker.select(at(0, 5))
        picker.move(.down)
        #expect(picker.selection == at(0, 10), "the second row has two")
        picker.move(.down)
        #expect(picker.selection == at(1, 1), "the next section's first row, the same column")
        picker.select(at(0, 9))
        picker.move(.down)
        #expect(picker.selection == at(1, 0))
    }

    @Test
    func upGoesToThePreviousSectionsLastRow() throws {
        let picker = try picker()

        picker.select(at(1, 2))
        picker.move(.up)
        #expect(picker.selection == at(0, 10))
        picker.select(at(1, 0))
        picker.move(.up)
        #expect(picker.selection == at(0, 9))
        picker.select(at(0, 12 - 9))
        picker.move(.up)
        #expect(picker.selection == at(0, 3), "the first row of the first section")
    }

    @Test
    func theLastEmojiStaysSelectedGoingOn() throws {
        let picker = try picker()
        let last = at(picker.sections.count - 1, picker.sections[picker.sections.count - 1].items.count - 1)

        picker.select(last)
        picker.move(.right)
        picker.move(.down)

        #expect(picker.selection == last)
    }

    @Test
    func aKeyRevealsTheSelectionAndThePointerDoesnt() throws {
        let picker = try picker()

        picker.move(.right)
        picker.select(at(0, 4))

        #expect(changes == [.selection(.item), .selection(.none)])
    }

    @Test
    func choosingASectionSelectsItsFirstEmojiAndShowsItsTop() throws {
        let picker = try picker()

        picker.jump(to: .server(id(2)))

        #expect(picker.selection == at(2, 0))
        #expect(changes == [.selection(.section)])
    }

    @Test
    func choosingASectionWhileSearchingBrowsesAgain() async throws {
        store.update { $0.emojiSearch = { _ in [pickerEmoji(7, "wave")] } }
        let picker = try picker()
        picker.search("wa")
        try await eventually { picker.sections.first?.kind == .results }

        picker.jump(to: .server(id(1)))

        #expect(picker.query.isEmpty)
        #expect(picker.sections.first?.kind == .recent)
        #expect(picker.selection == at(1, 0))
        #expect(changes.suffix(2) == [.sections(fromTop: true), .selection(.section)])
    }

    // The strip keeps the browsing sections' buttons as they were when the search began.
    @Test
    func aSectionChosenWhileSearchingIsTheOneItsButtonShowed() async throws {
        store.update { $0.emojiSearch = { _ in [pickerEmoji(7, "wave")] } }
        let picker = try picker()
        picker.search("wa")
        try await eventually { picker.sections.first?.kind == .results }
        store.update { $0.recentReactions[id(11)] = [] }
        reactions.apply(EventBatch([.currentUserUpdated]))

        picker.jump(to: .server(id(2)))

        let selection = try #require(picker.selection)
        #expect(picker.sections[selection.section].kind == .server(id(2)))
        #expect(selection.item == 0)
    }

    // MARK: Search

    @Test
    func aSearchShowsWhatTheCoreFindsWithTheFirstSelected() async throws {
        store.update { state in
            state.emojiSearch = { query in query == "wa" ? [pickerEmoji(7, "wave"), pickerEmoji("👋")] : [] }
        }
        let picker = try picker()

        picker.search("wa")
        try await eventually { picker.sections.first?.kind == .results }

        #expect(picker.sections.count == 1)
        #expect(picker.sections[0].items.map(\.name) == [":wave:", ":wave:"])
        #expect(picker.selection == at(0, 0))
        #expect(changes.last == .sections(fromTop: true))
        #expect(calls == [.searchEmoji(here, "wa", EmojiCatalog.newest, limit: EmojiPickerModel.searchLimit)])
    }

    @Test
    func nothingFoundShowsAnEmptySearch() async throws {
        let picker = try picker()

        picker.search("zzz")
        try await eventually { picker.sections.first?.kind == .results }

        #expect(picker.sections[0].items.isEmpty)
        #expect(picker.selection == nil)
        #expect(picker.selected == nil)
        #expect(picker.foundNothing)
    }

    @Test
    func theNewestAnswerWinsAndAnOlderOneNeverReplacesIt() async throws {
        store.update { state in
            state.emojiSearch = { query in query == "w" ? [pickerEmoji("🌊")] : [pickerEmoji(7, "wave")] }
        }
        let held = store.holdSearch("w")
        let picker = try picker()

        picker.search("w")
        picker.search("wa")
        try await eventually { picker.sections.first?.kind == .results }
        held.fire()
        try await eventually { picker.answersHandled == 2 }

        #expect(picker.sections.first?.items.map(\.name) == [":wave:"])
        #expect(picker.query == "wa")
    }

    @Test
    func whatWasFoundStaysUntilTheNextAnswer() async throws {
        store.update { state in
            state.emojiSearch = { query in query == "w" ? [pickerEmoji("🌊")] : [pickerEmoji(7, "wave")] }
        }
        let held = store.holdSearch("wa")
        let picker = try picker()
        picker.search("w")
        try await eventually { picker.sections.first?.kind == .results }

        picker.search("wa")
        try await eventually { self.calls.count == 2 }
        #expect(picker.sections.first?.items.map(\.glyph) == ["🌊"])

        held.fire()
        try await eventually { picker.sections.first?.items.first?.name == ":wave:" }
    }

    @Test
    func anEmptySearchBrowsesAtOnceAndIgnoresAnswersStillComing() async throws {
        store.update { $0.emojiSearch = { _ in [pickerEmoji(7, "wave")] } }
        let held = store.holdSearch("w")
        let picker = try picker()

        picker.search("w")
        picker.search("")
        #expect(picker.sections.first?.kind == .recent)
        #expect(picker.selection == at(0, 0))

        held.fire()
        try await eventually { picker.answersHandled == 1 }
        #expect(picker.sections.first?.kind == .recent)
    }

    @Test
    func anAnswerFromBeforeTheSearchWasClearedNeverShows() async throws {
        store.update { state in
            state.emojiSearch = { query in query == "w" ? [pickerEmoji("🌊")] : [pickerEmoji(7, "wave")] }
        }
        let wave = store.holdSearch("w")
        let x = store.holdSearch("x")
        let picker = try picker()

        picker.search("w")
        picker.search("")
        picker.search("x")
        wave.fire()
        try await eventually { picker.answersHandled == 1 }
        #expect(picker.sections.first?.kind == .recent)

        x.fire()
        try await eventually { picker.sections.first?.items.first?.name == ":wave:" }
    }

    @Test(arguments: ["   ", ":", " :: "])
    func spacesAndColonsAloneSearchNothing(query: String) throws {
        let picker = try picker()
        picker.select(at(2, 1))
        changes = []

        picker.search(query)

        #expect(picker.searches == 0)
        #expect(picker.query == query)
        #expect(picker.sections.first?.kind == .recent)
        #expect(picker.selection == at(2, 1))
        #expect(changes.isEmpty)
    }

    @Test
    func aNewSearchSelectsItsFirstResultAndEndingItTheFirstEmoji() async throws {
        store.update { $0.emojiSearch = { _ in [pickerEmoji(7, "wave"), pickerEmoji("👋"), pickerEmoji("🌊")] } }
        let picker = try picker()
        picker.select(at(0, 2))

        picker.search("wa")
        try await eventually { picker.sections.first?.kind == .results }

        #expect(picker.selection == at(0, 0))
        picker.move(.right)
        picker.move(.right)
        picker.search("")
        #expect(picker.sections.first?.kind == .recent)
        #expect(picker.selection == at(0, 0))
    }

    // MARK: Picking

    @Test
    func aPickReactsFromWhereThePickerWasOpenedAndClosesIt() async throws {
        let picker = try picker(from: .inlineButton)

        #expect(picker.pick(at(1, 0), keepOpen: false))

        #expect(!picker.isOpen)
        #expect(closes == 1)
        try await eventually { self.calls == [.react(self.here, self.it, "wave:7", .inlineButton)] }
    }

    @Test
    func aPickWithShiftKeepsItOpen() async throws {
        let picker = try picker()

        #expect(picker.pick(at(0, 4), keepOpen: true))

        #expect(picker.isOpen)
        #expect(picker.selection == at(0, 4))
        try await eventually { self.calls == [.react(self.here, self.it, "😆", .picker)] }
    }

    @Test
    func enterPicksTheSelection() async throws {
        let picker = try picker()
        picker.move(.right)

        #expect(picker.pick(keepOpen: false))

        try await eventually { self.calls == [.react(self.here, self.it, "😃", .picker)] }
    }

    @Test
    func aLockedEmojiIsNeverPicked() async throws {
        let picker = try picker()

        #expect(!picker.pick(at(1, 2), keepOpen: false))
        #expect(!picker.pick(at(2, 0), keepOpen: false))

        #expect(picker.isOpen)
        #expect(picker.selection == at(2, 0))
        #expect(picker.reacting == nil)
        #expect(calls.isEmpty)
    }

    // What the user typed is what they get: never the list from before the answer.
    @Test
    func returnBeforeTheSearchAnswersPicksTheAnswersFirst() async throws {
        store.update { $0.emojiSearch = { query in query == "fire" ? [pickerEmoji("🔥")] : [] } }
        let held = store.holdSearch("fire")
        let picker = try picker()
        picker.search("fire")

        #expect(picker.pick(keepOpen: false))

        #expect(picker.isOpen)
        #expect(picker.reacting == nil)
        held.fire()
        try await eventually { !picker.isOpen }
        await picker.reacting?.value
        #expect(
            calls == [
                .searchEmoji(here, "fire", EmojiCatalog.newest, limit: EmojiPickerModel.searchLimit),
                .react(here, it, "🔥", .picker),
            ])
    }

    @Test
    func aPickWaitingForTheSearchIsDroppedWhenTheTextChanges() async throws {
        store.update { $0.emojiSearch = { query in [pickerEmoji(query == "fire" ? "🔥" : "🚒")] } }
        let held = store.holdSearch("fire")
        let picker = try picker()
        picker.search("fire")
        #expect(picker.pick(keepOpen: false))

        picker.search("fir")
        try await eventually { picker.answersHandled == 1 }
        held.fire()
        try await eventually { picker.answersHandled == 2 }

        #expect(picker.isOpen)
        #expect(picker.selected?.glyph == "🚒")
        #expect(picker.reacting == nil)
    }

    @Test
    func aPickOfALockedEmojiIsRefused() throws {
        let picker = try picker()
        var refused = 0
        picker.onRefused = { refused += 1 }

        #expect(!picker.pick(at(1, 2), keepOpen: false))
        #expect(refused == 1)
        picker.select(at(2, 0))
        #expect(!picker.pick(keepOpen: false))
        #expect(refused == 2)
        #expect(picker.pick(at(0, 0), keepOpen: true))
        #expect(refused == 2)
    }

    @Test
    func pickingAnEmojiTheUserReactedWithKeepsTheirReaction() async throws {
        store.update { $0.reactions[id(100)] = [reaction("😀", 1, me: true)] }
        let picker = try picker()

        #expect(picker.pick(at(0, 0), keepOpen: false))

        #expect(!picker.isOpen)
        await picker.reacting?.value
        #expect(calls.isEmpty)
    }

    @Test
    func nothingIsPickedOnceClosed() async throws {
        let picker = try picker()
        picker.close()

        #expect(!picker.pick(at(0, 0), keepOpen: false))
        #expect(picker.reacting == nil)
        #expect(calls.isEmpty)
    }

    // MARK: Changes

    @Test
    func aServersEmojiChangeIsReadAgainKeepingTheSelection() throws {
        let picker = try picker()
        picker.select(at(1, 1))
        store.update { state in
            state.emojiSections[id(11)]?[0].emojis.insert(customEmoji(6, "new"), at: 0)
        }

        reactions.apply(EventBatch([.guildEmojisChanged(guildId: id(1))]))

        #expect(picker.sections[1].items.map(\.name) == [":new:", ":wave:", ":cat:", ":gone:"])
        #expect(picker.selected?.name == ":cat:")
        #expect(changes.last == .sections(fromTop: false))
    }

    @Test(arguments: [
        StoreEvent.guildUpdated(guildId: id(2)), .currentMemberUpdated(guildId: id(1)), .currentUserUpdated,
        .serverListChanged, .ready,
    ])
    func whatDecidesWhichEmojiMayBeUsedReadsThemAgain(event: StoreEvent) throws {
        let picker = try picker()
        store.update { $0.emojiSections[id(11)]?[1].emojis[0].access = .usable }

        reactions.apply(EventBatch([event]))

        #expect(!picker.sections[2].items[0].isLocked)
    }

    @Test
    func anotherChannelsChangeLeavesItAlone() throws {
        let picker = try picker()
        store.log.forget()

        reactions.apply(EventBatch([.channelUpdated(channelId: id(12), guildId: id(1))]))

        #expect(store.reads.isEmpty)
        #expect(picker.isOpen)
    }

    @Test
    func aChangeWhileSearchingAsksAgain() async throws {
        store.update { $0.emojiSearch = { _ in [pickerEmoji(7, "wave")] } }
        let picker = try picker()
        picker.search("wa")
        try await eventually { picker.sections.first?.kind == .results }
        store.update { $0.emojiSections[id(11)]?[0].emojis.insert(customEmoji(6, "new"), at: 0) }

        reactions.apply(EventBatch([.guildEmojisChanged(guildId: id(1))]))

        try await eventually { self.calls.count == 2 }
        #expect(picker.sections.first?.kind == .results)
    }

    @Test
    func aChangeDuringASearchShowsWhenItEnds() async throws {
        store.update { $0.emojiSearch = { _ in [pickerEmoji(7, "wave")] } }
        let picker = try picker()
        picker.search("wa")
        try await eventually { picker.sections.first?.kind == .results }
        store.update { $0.emojiSections[id(11)]?[0].emojis.insert(customEmoji(6, "new"), at: 0) }
        reactions.apply(EventBatch([.guildEmojisChanged(guildId: id(1))]))

        picker.search("")

        #expect(picker.sections[1].items.map(\.name) == [":new:", ":wave:", ":cat:", ":gone:"])
    }

    // A DM's channel changes with each of its messages; nothing the lists depend on changes.
    @Test
    func aMessageInADmLeavesTheSearchAndItsSelectionAlone() async throws {
        store.update { state in
            state.talk(dm(12, with: 5), with: user(5))
            state.allow(reacting(), message: 100, in: 12)
            state.recentReactions[id(12)] = Self.faces.map(pickerEmoji)
            state.emojiSearch = { _ in [pickerEmoji(7, "wave"), pickerEmoji("👋")] }
        }
        let dmRow = MessageListModel.Row(id: it, message: message(100, in: 12))
        let picker = try #require(reactions.openPicker(for: dmRow, from: .picker))
        picker.onChange = { [unowned self] in changes.append($0) }
        picker.search("wa")
        try await eventually { picker.sections.first?.kind == .results }
        picker.move(.right)
        changes = []
        store.log.forget()

        reactions.apply(EventBatch([.channelUpdated(channelId: id(12), guildId: nil)]))

        #expect(store.reads.isEmpty)
        #expect(picker.searches == 1)
        #expect(picker.selected?.glyph == "👋")
        #expect(changes.isEmpty)
    }

    @Test
    func aChannelsChangeReadsTheListsAgainOnlyWhenThePermissionsThereChange() throws {
        let picker = try picker()
        store.log.forget()

        reactions.apply(EventBatch([.channelUpdated(channelId: here, guildId: id(1))]))

        #expect(!store.reads.contains(.customEmojiSections(here)))
        #expect(changes.isEmpty)

        store.update { state in
            state.permissions[id(11)] = [.viewChannel, .useExternalEmojis]
            state.emojiSections[id(11)]?[1].emojis[0].access = .usable
        }
        reactions.apply(EventBatch([.channelUpdated(channelId: here, guildId: id(1))]))

        #expect(!picker.sections[2].items[0].isLocked)
        #expect(changes == [.sections(fromTop: false)])
    }

    // The member's change read the lists with its permissions: a channel's change back to the
    // ones from before reads them again.
    @Test
    func aChannelsChangeAfterAMembersChangeComparesWithTheLastRead() throws {
        let picker = try picker()
        store.update { state in
            state.permissions[id(11)] = [.viewChannel, .useExternalEmojis]
            state.emojiSections[id(11)]?[1].emojis[0].access = .usable
        }
        reactions.apply(EventBatch([.currentMemberUpdated(guildId: id(1))]))
        #expect(!picker.sections[2].items[0].isLocked)

        store.update { state in
            state.permissions[id(11)] = nil
            state.emojiSections[id(11)]?[1].emojis[0].access = .nitroLocked
        }
        reactions.apply(EventBatch([.channelUpdated(channelId: here, guildId: id(1))]))

        #expect(picker.sections[2].items[0].isLocked)
    }

    // A thread's permissions are its parent's.
    @Test
    func aThreadsParentsChangeIsTheThreadsOwn() throws {
        store.update { state in
            state.channels[id(13)] = thread(13, in: 11)
            state.allow(reacting(), message: 100, in: 13)
            state.emojiSections[id(13)] = state.emojiSections[id(11)]
        }
        let threadRow = MessageListModel.Row(id: it, message: message(100, in: 13))
        let picker = try #require(reactions.openPicker(for: threadRow, from: .picker))
        store.update { state in
            state.permissions[id(13)] = [.viewChannel, .useExternalEmojis]
            state.emojiSections[id(13)]?[1].emojis[0].access = .usable
        }

        reactions.apply(EventBatch([.channelUpdated(channelId: here, guildId: id(1))]))

        #expect(picker.sections[1].kind == .server(id(2)))
        #expect(!picker.sections[1].items[0].isLocked)
    }

    @Test
    func anUpdateOfAServerThePickerDoesntListReadsNothing() throws {
        _ = try picker()
        store.log.forget()

        reactions.apply(EventBatch([.guildUpdated(guildId: id(3))]))

        #expect(store.reads.isEmpty)
    }

    @Test
    func aChangeThatLeavesTheListsAsTheyWereSendsNothing() throws {
        let picker = try picker()
        picker.select(at(3, 5))

        reactions.apply(EventBatch([.guildUpdated(guildId: id(1)), .currentMemberUpdated(guildId: id(1))]))

        #expect(changes == [.selection(.none)])
        #expect(picker.selection == at(3, 5))
    }

    // 😃 is in Frequently Used too; the one in Smileys stays selected, where the user is.
    @Test
    func aReReadKeepsTheSelectionInItsSectionNotAtTheFirstCopy() throws {
        let picker = try picker()
        let smileys = try #require(picker.sections.firstIndex { $0.kind == .unicode(.smileysAndEmotion) })
        let smiley = try #require(picker.sections[smileys].items.firstIndex { $0.glyph == "😃" })
        picker.select(at(smileys, smiley))
        store.update { $0.emojiSections[id(11)]?[0].emojis.insert(customEmoji(6, "new"), at: 0) }

        reactions.apply(EventBatch([.guildEmojisChanged(guildId: id(1))]))

        #expect(picker.selection == at(smileys, smiley))
        #expect(changes.last == .sections(fromTop: false))
    }

    // A server may have two emoji of one name; the official client shows the second as wave~1.
    @Test
    func aNewEmojiOfTheSameNameLeavesTheSelectionOnItsOwn() throws {
        let picker = try picker()
        picker.select(at(1, 0))
        store.update { $0.emojiSections[id(11)]?[0].emojis.insert(customEmoji(6, "wave"), at: 0) }

        reactions.apply(EventBatch([.guildEmojisChanged(guildId: id(1))]))

        #expect(picker.selection == at(1, 1))
        #expect(picker.selected?.key == .emoji(emojiId: id(7), animated: false))
    }

    @Test
    func anEmojiThatLeftItsSectionLeavesTheSelectionInItsPlace() throws {
        let picker = try picker()
        picker.select(at(1, 1))
        store.update { $0.emojiSections[id(11)]?[0].emojis.remove(at: 1) }

        reactions.apply(EventBatch([.guildEmojisChanged(guildId: id(1))]))

        #expect(picker.selection == at(1, 1))
        #expect(picker.selected?.name == ":gone:")
    }

    @Test
    func aChangeWhileSearchingKeepsTheSelectionAndTheScroll() async throws {
        store.update { $0.emojiSearch = { _ in [pickerEmoji(7, "wave"), pickerEmoji("👋")] } }
        let picker = try picker()
        picker.search("wa")
        try await eventually { picker.sections.first?.kind == .results }
        picker.move(.right)
        changes = []
        store.update { state in
            state.emojiSections[id(11)]?[0].emojis.insert(customEmoji(6, "wavy"), at: 0)
            state.emojiSearch = { _ in [pickerEmoji(6, "wavy"), pickerEmoji(7, "wave"), pickerEmoji("👋")] }
        }

        reactions.apply(EventBatch([.guildEmojisChanged(guildId: id(1))]))
        try await eventually { picker.answersHandled == 2 }

        #expect(picker.sections[0].items.count == 3)
        #expect(picker.selected?.glyph == "👋")
        #expect(changes == [.sections(fromTop: false)])
    }

    @Test
    func aSearchAskedAgainThatFindsTheSameSendsNothing() async throws {
        store.update { $0.emojiSearch = { _ in [pickerEmoji(7, "wave"), pickerEmoji("👋")] } }
        let picker = try picker()
        picker.search("wa")
        try await eventually { picker.sections.first?.kind == .results }
        picker.move(.right)
        changes = []
        store.update { $0.emojiSections[id(11)]?[0].emojis.insert(customEmoji(6, "new"), at: 0) }

        reactions.apply(EventBatch([.guildEmojisChanged(guildId: id(1))]))
        try await eventually { picker.answersHandled == 2 }

        #expect(picker.selected?.glyph == "👋")
        #expect(changes.isEmpty)
    }

    @Test
    func aServerWithoutAnyEmojiLeftLeavesTheSelectionOnTheFirst() throws {
        let picker = try picker()
        picker.select(at(2, 1))
        store.update { $0.emojiSections[id(11)] = [$0.emojiSections[id(11)]![0]] }

        reactions.apply(EventBatch([.guildEmojisChanged(guildId: id(2))]))

        #expect(picker.sections.map(\.kind).prefix(3) == [.recent, .server(id(1)), .unicode(.smileysAndEmotion)])
        #expect(picker.selection == at(0, 0))
    }
}
