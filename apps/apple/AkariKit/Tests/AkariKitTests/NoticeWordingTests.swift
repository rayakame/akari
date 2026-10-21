import Foundation
import Testing

@testable import AkariKit

// Discord's wording for every system message (its client, en-US, read 2026-10-20): names and
// bold parts strong, links only where Akari has the target.
struct NoticeWordingTests {
    let ray = user(2, name: "Ray")
    let mira = user(3, name: "Mira")
    let me = user(1, name: "Me")
    // 2026-10-20 12:00:00 UTC.
    let sent = Date(timeIntervalSince1970: 1_792_497_600)

    func message(
        _ kind: MessageType, id raw: UInt64 = 1, by author: User? = nil, content: String = "",
        mentions: [User] = [], call: MessageCall? = nil, subscription: RoleSubscription? = nil,
        product: String? = nil, actor: User? = nil, application: String? = nil,
        fields: [EmbedField] = [], reference: MessageReference? = nil
    ) -> Message {
        Message(
            id: id(raw), channelId: id(10), kind: kind, author: author ?? ray, fromWebhook: false,
            content: content, timestamp: sent, editedTimestamp: nil, pinned: false,
            mentionEveryone: false, attachments: [], embedCount: fields.isEmpty ? 0 : 1,
            stickerNames: [], componentsV2: false, delivery: .sent,
            formatted: formattedText(content: content), mentionsMe: false, reference: reference,
            hasThread: false, mentions: mentions, call: call, roleSubscription: subscription,
            purchasedProduct: product, actor: actor, applicationName: application,
            embedFields: fields)
    }

    func context(
        _ place: Message.NoticeContext.Place = .channel, guild: String? = "Lab"
    ) -> Message.NoticeContext {
        var context = Message.NoticeContext(currentUserId: me.id, place: place)
        context.guildName = guild
        return context
    }

    func ended(after seconds: TimeInterval, participants: [User]) -> MessageCall {
        MessageCall(
            participants: participants.map(\.id), endedTimestamp: sent + seconds)
    }

    // MARK: Added and removed

    @Test
    func someoneAddedToAThreadOrAGroupIsNamed() {
        let added = message(.recipientAdd, mentions: [mira])

        #expect(added.noticeParts(in: context(.thread)) == [
            .actor("Ray"), .text(" added "), .strong("Mira"), .text(" to the thread."),
        ])
        #expect(added.notice(in: context(.groupDM)) == "Ray added Mira to the group.")
    }

    // A forum post is a thread too.
    @Test
    func aForumPostsMembersAreAddedToTheThread() {
        #expect(
            message(.recipientAdd, mentions: [mira]).notice(in: context(.forumPost))
                == "Ray added Mira to the thread.")
        #expect(
            message(.recipientRemove, mentions: [mira]).notice(in: context(.forumPost))
                == "Ray removed Mira from the thread.")
    }

    @Test
    func someoneRemovedFromAThreadOrAGroupIsNamed() {
        let removed = message(.recipientRemove, mentions: [mira])

        #expect(removed.notice(in: context(.thread)) == "Ray removed Mira from the thread.")
        #expect(removed.notice(in: context(.groupDM)) == "Ray removed Mira from the group.")
    }

    // Discord's choice: the author named, or nobody, means they left.
    @Test
    func leavingAGroupSaysSo() {
        #expect(
            message(.recipientRemove, mentions: [ray]).notice(in: context(.groupDM))
                == "Ray left the group.")
        #expect(message(.recipientRemove).notice(in: context(.groupDM)) == "Ray left the group.")
    }

    @Test
    func aMissingNameReadsAsDiscordsPlaceholder() {
        #expect(message(.recipientAdd).notice(in: context(.thread)) == "Ray added ??? to the thread.")
        #expect(
            message(.recipientAdd).notice(in: context(.groupDM))
                == "Ray added unknown-user to the group.")
    }

    // The list resolves names as headers show them; a person it has no name for goes by their
    // display name.
    @Test
    func aLineNamesPeopleAsItsContextDoes() {
        var here = context(.thread)
        here.names = [ray.id: "Rayo", mira.id: "Mimi"]

        #expect(
            message(.recipientAdd, mentions: [mira]).notice(in: here)
                == "Rayo added Mimi to the thread.")
        #expect(
            message(.recipientAdd, mentions: [me]).notice(in: here) == "Rayo added Me to the thread.")
    }

    // MARK: Calls

    @Test
    func anEndedCallSaysHowLongItLasted() {
        let call = message(.call, call: ended(after: 125, participants: [ray, me]))

        #expect(call.notice(in: context(.dm)) == "Ray started a call that lasted 2 minutes.")
    }

    @Test
    func aCallTheReaderMissedSaysSo() {
        let missed = message(.call, call: ended(after: 30, participants: [ray]))

        #expect(missed.noticeParts(in: context(.dm)) == [
            .text("You missed a call from "), .actor("Ray"), .text(" that lasted a few seconds."),
        ])
    }

    // The apps draw a missed call's icon by the same rule.
    @Test
    func aCallIsMissedOnlyOnceItEndedWithoutTheReader() {
        let reader = me.id
        let missed = message(.call, call: ended(after: 30, participants: [ray]))
        let answered = message(
            .call, call: MessageCall(participants: [ray.id, reader], endedTimestamp: Date()))
        let running = message(.call, call: MessageCall(participants: [ray.id], endedTimestamp: nil))

        #expect(missed.isMissedCall(by: reader))
        #expect(!answered.isMissedCall(by: reader))
        #expect(!running.isMissedCall(by: reader))
    }

    // Akari can't join calls: "Join the call" is strong text without a link.
    @Test
    func aRunningCallOffersToJoin() {
        let running = message(.call, call: MessageCall(participants: [ray.id], endedTimestamp: nil))

        #expect(running.noticeParts(in: context(.dm)) == [
            .actor("Ray"), .text(" started a call. — "), .strong("Join the call"),
        ])
    }

    @Test
    func aCallWithoutItsRecordJustStarted() {
        #expect(message(.call).notice(in: context(.dm)) == "Ray started a call.")
    }

    // moment 2.30.1's humanize(), as Discord's client words a call's length.
    @Test(arguments: [
        (0.0, "a few seconds"), (44.4, "a few seconds"), (44.5, "a minute"), (89, "a minute"),
        (90, "2 minutes"), (44 * 60, "44 minutes"), (44.5 * 60, "an hour"), (89 * 60, "an hour"),
        (90 * 60, "2 hours"), (21 * 3600, "21 hours"), (21.5 * 3600, "a day"),
        (35 * 3600, "a day"), (36 * 3600, "2 days"), (25 * 86400, "25 days"),
        (25.5 * 86400, "a month"), (46 * 86400, "2 months"), (320 * 86400, "a year"),
        (548 * 86400, "2 years"),
    ] as [(TimeInterval, String)])
    func callLengthsReadAsDiscordsDo(seconds: TimeInterval, words: String) {
        #expect(Message.humanized(seconds) == words)
    }

    // MARK: Names and icons

    @Test
    func aRenamedChannelPostOrGroupIsNamedStrong() {
        let renamed = message(.channelNameChange, content: "lobby")

        #expect(renamed.noticeParts(in: context()) == [
            .actor("Ray"), .text(" changed the channel name: "), .strong("lobby"),
        ])
        #expect(renamed.notice(in: context(.thread)) == "Ray changed the channel name: lobby")
        #expect(renamed.notice(in: context(.forumPost)) == "Ray changed the post title: lobby")
        #expect(renamed.noticeParts(in: context(.groupDM)) == [
            .actor("Ray"), .text(" changed the group name: "), .strong("lobby"), .text(". "),
            .strong("Edit Group"),
        ])
        #expect(
            message(.channelNameChange).notice(in: context(.groupDM))
                == "Ray removed the custom group name. Edit Group")
    }

    @Test
    func aGroupsNewIcon() {
        #expect(message(.channelIconChange).noticeParts(in: context(.groupDM)) == [
            .actor("Ray"), .text(" changed the group icon. "), .strong("Edit Group"),
        ])
    }

    // MARK: Joins

    // Discord picks the line from the message's time: its ID's milliseconds since 1970, mod 13.
    @Test
    func aJoinTakesTheWelcomeLineItsTimePicks() {
        let lines = [
            "Ray joined the party.", "Ray is here.", "Welcome, Ray. We hope you brought pizza.",
            "A wild Ray appeared.", "Ray just landed.", "Ray just slid into the server.",
            "Ray just showed up!", "Welcome Ray. Say hi!", "Ray hopped into the server.",
            "Everyone welcome Ray!", "Glad you're here, Ray.", "Good to see you, Ray.",
            "Yay you made it, Ray!",
        ]
        // 1420070400000 % 13 == 5: Discord's epoch itself picks line 5.
        for offset in UInt64(0)..<13 {
            let join = message(.userJoin, id: offset << 22)
            #expect(join.notice(in: context()) == lines[Int((offset + 5) % 13)])
        }
        #expect(
            message(.userJoin, id: 11 << 22).noticeParts(in: context()) == [
                .text("A wild "), .actor("Ray"), .text(" appeared."),
            ])
    }

    // Number(id) is a double: above 2^53 an ID just below a millisecond's end rounds into the
    // next one.
    @Test
    func theWelcomeLineFollowsTheIdsDoubleRounding() {
        let rounded = message(.userJoin, id: (1 << 60) + (1 << 22) - 1)
        let next = message(.userJoin, id: (1 << 60) + (1 << 22))
        let before = message(.userJoin, id: 1 << 60)

        #expect(rounded.notice(in: context()) == next.notice(in: context()))
        #expect(rounded.notice(in: context()) != before.notice(in: context()))
    }

    @Test
    func aBotsJoinNamesWhoAddedIt() {
        let bot = User(
            id: id(9), username: "akaribot", globalName: nil, displayName: "Akari Bot", bot: true,
            system: false)

        #expect(message(.userJoin, by: bot, actor: mira).noticeParts(in: context()) == [
            .actor("Mira"), .text(" has added "), .strong("Akari Bot"), .text(" to the server."),
        ])
        #expect(
            message(.userJoin, by: bot).notice(in: context())
                == "Akari Bot was added to the server.")
    }

    // MARK: Boosts

    @Test
    func boostsSayHowOftenAndTheLevelReached() {
        #expect(message(.premiumGuildSubscription).notice(in: context()) == "Ray just boosted the server!")
        #expect(
            message(.premiumGuildSubscription, content: "3").noticeParts(in: context()) == [
                .actor("Ray"), .text(" just boosted the server "), .strong("3"), .text(" times!"),
            ])
        #expect(
            message(.premiumGuildSubscriptionTier2).noticeParts(in: context()) == [
                .actor("Ray"), .text(" just boosted the server! Lab has achieved "),
                .strong("Level 2!"),
            ])
        #expect(
            message(.premiumGuildSubscriptionTier3, content: "2").notice(in: context())
                == "Ray just boosted the server 2 times! Lab has achieved Level 3!")
        #expect(
            message(.premiumGuildSubscriptionTier1).notice(in: context(guild: nil))
                == "Ray just boosted the server!")
    }

    // Number("inf") is NaN in JavaScript; Swift's Double reads it as infinity.
    @Test
    func aBoostCountThatIsntANumberReadsAsOne() {
        #expect(
            message(.premiumGuildSubscription, content: "inf").notice(in: context())
                == "Ray just boosted the server!")
        #expect(
            message(.premiumGuildSubscription, content: "1e19").notice(in: context())
                == "Ray just boosted the server 10000000000000000000 times!")
    }

    // MARK: Server notices

    @Test
    func aFollowedChannelIsNamedStrong() {
        #expect(
            message(.channelFollowAdd, content: "Lab #news").noticeParts(in: context()) == [
                .actor("Ray"), .text(" has added "), .strong("Lab #news"),
                .text(" to this channel. Its most important updates will show up here."),
            ])
    }

    @Test
    func discoveryNoticesNameTheServer() {
        var here = context()
        here.guildId = id(1)

        #expect(message(.guildDiscoveryDisqualified).noticeParts(in: here) == [
            .guild("Lab", id: id(1)),
            .text(
                " has been removed from Server Discovery because it no longer passes all the"
                    + " requirements. Check "),
            .strong("Server Settings"), .text(" for more details."),
        ])
        #expect(
            message(.guildDiscoveryRequalified).notice(in: here)
                == "This server is eligible for Server Discovery again and has been automatically"
                + " relisted!")
        #expect(
            message(.guildDiscoveryGracePeriodInitialWarning).notice(in: here)
                == "Lab has failed Discovery activity requirements for 1 week. If it fails for 4"
                + " weeks in a row, it will be automatically removed from Discovery.")
        #expect(
            message(.guildDiscoveryGracePeriodFinalWarning).notice(in: here)
                == "Lab has failed Discovery activity requirements for 3 weeks in a row. If it"
                + " fails for 1 more week, it will be removed from Discovery.")
        #expect(
            message(.guildDiscoveryDisqualified).noticeParts(in: context(guild: nil))?.first
                == .strong("this server"))
    }

    @Test
    func anAutoModAlertNamesItsChannel() {
        var here = context()
        here.alertChannelName = "general"
        here.alertChannelId = id(11)
        let blocked = message(
            .autoModerationAction,
            fields: [
                EmbedField(name: "decision_outcome", value: "blocked"),
                EmbedField(name: "channel_id", value: "11"),
            ])

        #expect(blocked.noticeParts(in: here) == [
            .strong("AutoMod"), .text(" has blocked a message in "),
            .channel("general", id: id(11)),
        ])
        #expect(
            message(.autoModerationAction).notice(in: context())
                == "AutoMod has flagged a message in unknown-channel")
    }

    @Test
    func aRoleSubscriptionSaysTheTierAndHowLong() {
        let joined = RoleSubscription(tierName: "Gold", totalMonthsSubscribed: 1, isRenewal: false)
        let renewed = RoleSubscription(tierName: "Gold", totalMonthsSubscribed: 3, isRenewal: true)
        let new = RoleSubscription(tierName: "Gold", totalMonthsSubscribed: 0, isRenewal: false)
        let again = RoleSubscription(tierName: "Gold", totalMonthsSubscribed: 0, isRenewal: true)

        #expect(message(.roleSubscriptionPurchase, subscription: joined).noticeParts(in: context()) == [
            .actor("Ray"), .text(" joined "), .strong("Gold"),
            .text(" and has been a subscriber of "), .strong("Lab"), .text(" for 1 month!"),
        ])
        #expect(
            message(.roleSubscriptionPurchase, subscription: renewed).notice(in: context())
                == "Ray renewed Gold and has been a subscriber of Lab for 3 months!")
        #expect(
            message(.roleSubscriptionPurchase, subscription: new).notice(in: context())
                == "Ray joined Gold as a subscriber of Lab!")
        #expect(
            message(.roleSubscriptionPurchase, subscription: again).notice(in: context())
                == "Ray renewed Gold in their Lab membership!")
    }

    @Test
    func stageLinesNameTheirTopic() {
        #expect(message(.stageStart, content: "Q&A").noticeParts(in: context()) == [
            .actor("Ray"), .text(" started "), .strong("Q&A"),
        ])
        #expect(message(.stageEnd, content: "Q&A").notice(in: context()) == "Ray ended Q&A")
        #expect(message(.stageSpeaker).notice(in: context()) == "Ray is now a speaker.")
        #expect(message(.stageRaiseHand).notice(in: context()) == "Ray requested to speak.")
        #expect(
            message(.stageTopic, content: "AMA").notice(in: context())
                == "Ray changed the Stage topic: AMA")
    }

    @Test
    func anAppUpgradeNamesTheApp() {
        #expect(
            message(.guildApplicationPremiumSubscription, application: "Akari Bot")
                .notice(in: context())
                == "Ray upgraded Akari Bot to premium for this server! 🎉")
        #expect(
            message(.guildApplicationPremiumSubscription).notice(in: context())
                == "Ray upgraded a deleted application to premium for this server! 🎉")
    }

    @Test
    func securityLinesSayWhatHappened() {
        var utc = context()
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        // Akari's English on a German Mac; the platform's format, as for every date.
        calendar.locale = Locale(identifier: "en_DE")
        utc.calendar = calendar

        #expect(
            message(.guildIncidentAlertModeEnabled, content: "2026-10-21T08:30:00.000000+00:00")
                .notice(in: utc) == "Ray enabled security actions until 21.10.2026, 8:30.")
        #expect(
            message(.guildIncidentAlertModeDisabled).notice(in: utc)
                == "Ray disabled security actions.")
        #expect(
            message(.guildIncidentReportRaid).notice(in: utc) == "Ray reported a raid in Lab.")
        #expect(
            message(.guildIncidentReportFalseAlarm).notice(in: utc)
                == "Ray resolved an Activity Alert.")
    }

    @Test
    func aPurchaseNamesTheProduct() {
        #expect(message(.purchaseNotification, product: "Badge").noticeParts(in: context()) == [
            .actor("Ray"), .text(" has purchased "), .strong("Badge"), .text("!"),
        ])
    }

    // The poll's title jumps to the poll, as a pin's "a message" jumps to the pin.
    @Test
    func aPollsResultNamesItsAuthorAndTitle() {
        let poll = MessageReference(messageId: id(5), channelId: id(10), guildId: id(1))
        let result = message(
            .pollResult, fields: [EmbedField(name: "poll_question_text", value: "Lunch?")],
            reference: poll)

        #expect(result.noticeParts(in: context()) == [
            .actor("Ray"), .text("'s poll "), .message("Lunch?", id: id(5)), .text(" has closed."),
        ])
    }

    @Test
    func aLongPollTitleIsCutAtFifty() {
        let long = String(repeating: "a", count: 47) + " . bcd"
        let result = message(
            .pollResult, fields: [EmbedField(name: "poll_question_text", value: long)])

        #expect(result.noticeParts(in: context())?[2] == .strong(String(repeating: "a", count: 47) + "…"))
    }

    // Discord counts UTF-16 units for the limit and code points for the cut.
    @Test
    func aPollTitleCutsAsDiscordsDoes() {
        let fifty = String(repeating: "b", count: 50)
        let emoji = String(repeating: "🙂", count: 26)

        #expect(Message.pollTitle(fifty) == fifty)
        #expect(Message.pollTitle(fifty + "c") == String(repeating: "b", count: 49) + "…")
        #expect(Message.pollTitle(emoji) == emoji + "…")
    }

    @Test
    func joinRequestsReadAsDiscordsWithoutTheRequest() {
        #expect(
            message(.guildJoinRequestAcceptNotification).notice(in: context())
                == "This application was approved! Welcome!")
        #expect(
            message(.guildJoinRequestRejectNotification).notice(in: context())
                == "This application was rejected.")
        #expect(
            message(.guildJoinRequestWithdrawnNotification).notice(in: context())
                == "This application has been withdrawn.")
    }

    @Test
    func reportsToModeratorsNameTheMember() {
        #expect(message(.reportToModBanUser, mentions: [mira]).noticeParts(in: context()) == [
            .actor("Ray"), .text(" banned "), .strong("Mira"),
        ])
        #expect(
            message(.reportToModKickUser, mentions: [mira]).notice(in: context()) == "Ray kicked Mira")
        #expect(
            message(.reportToModTimeoutUser, mentions: [mira]).notice(in: context())
                == "Ray timed out Mira")
        #expect(
            message(.reportToModDeletedMessage).notice(in: context()) == "Ray deleted the message")
        #expect(
            message(.reportToModClosedReport).notice(in: context()) == "Ray resolved this flag")
    }

    @Test
    func aVoiceSessionNamesWhoHungOut() {
        func session(_ names: [String], finished: Bool = true) -> String? {
            var here = context()
            here.participantNames = names
            let call =
                finished
                ? ended(after: 3600, participants: [])
                : MessageCall(participants: [], endedTimestamp: nil)
            return message(.voiceSession, call: call).notice(in: here)
        }

        #expect(session([], finished: false) == "Ray started a voice hangout.")
        #expect(session([]) == "Ray was hanging out for an hour.")
        #expect(session(["Mira"]) == "Ray and Mira were hanging out for an hour.")
        #expect(session(["Mira", "Noa"]) == "Ray, Mira, and Noa were hanging out for an hour.")
        #expect(session(["Mira", "Noa", "Ika"]) == "Ray, Mira, and 2 others were hanging out for an hour.")
    }

    @Test
    func anAcceptedFriendRequestSaysWhoAccepted() {
        var dm = context(.dm)
        dm.recipientName = "Ray"

        #expect(
            message(.friendRequestAccepted, by: me).notice(in: dm)
                == "You accepted Ray's friend request.")
        #expect(
            message(.friendRequestAccepted, by: me, content: "hi").noticeParts(in: dm) == [
                .text("You accepted "), .strong("Ray"), .text("'s friend request. They said: "),
                .strong("hi"),
            ])
        #expect(
            message(.friendRequestAccepted, content: "yo").notice(in: dm)
                == "Ray accepted your friend request. You said: yo")
        #expect(message(.friendRequestAccepted).noticeParts(in: context()) == [])
    }

    // MARK: Kinds Discord draws differently

    // Discord draws nothing in the channel for these, or a card Akari can't draw yet.
    @Test(arguments: [
        MessageType.guildInviteReminder, .guildDeadchatRevivePrompt, .guildGamingStatsPrompt,
        .hdStreamingUpgraded, .premiumReferral, .premiumGroupInvite, .nitroNotification,
        .giftingPrompt, .inGameMessageNux, .channelLinkedToLobby, .guildBoostUpsell,
        .purchaseNotification, .unknown(56), .unknown(57), .unknown(63),
    ])
    func kindsDiscordShowsNoLineForHaveNone(kind: MessageType) {
        #expect(message(kind).noticeParts(in: context()) == [])
    }

    // Discord draws these as messages people write.
    @Test(arguments: [
        MessageType.interactionPremiumUpsell, .customGift, .changelog, .mediaMentionMessage,
    ])
    func kindsDiscordDrawsAsMessagesAreMessages(kind: MessageType) {
        #expect(message(kind, content: "hi").noticeParts(in: context()) == nil)
    }
}
