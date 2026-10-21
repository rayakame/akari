import AkariKit
import AppKit
import Testing

@testable import Akari

// Each system line's icon as Discord's client draws it (its system-message renderer, canary
// 645048, read 2026-10-21), in SF Symbols: the shape, its color and its size.
@MainActor
struct NoticeIconTests {
    let me = UserId(rawValue: 1)
    let noon = berlinTime(2026, 10, 10, 12, 0)

    func icon(_ kind: MessageType, call: MessageCall? = nil) -> NoticeIcon {
        var message = row(1, by: author(2, "Ray"), at: noon, kind: kind).message
        message.call = call
        return NoticeIcon.of(message, reader: me)
    }

    @Test
    func thePollResultHasLeftAlignedBars() {
        #expect(
            icon(.pollResult)
                == NoticeIcon(symbol: "text.alignleft", color: Palette.interactiveText, size: 16))
    }

    @Test
    func thePinIsFilled() {
        #expect(
            icon(.channelPinnedMessage)
                == NoticeIcon(
                    symbol: "pin.fill", color: Palette.channelDefault, size: 18, slanted: true))
    }

    @Test
    func joinsAreGreenArrowsAndLeavesRedOnes() {
        for kind: MessageType in [
            .userJoin, .recipientAdd, .channelFollowAdd, .roleSubscriptionPurchase,
            .purchaseNotification, .guildJoinRequestAcceptNotification,
        ] {
            #expect(
                icon(kind) == NoticeIcon(symbol: "arrow.right", color: Palette.noticeIn, size: 16),
                "\(kind)")
        }
        for kind: MessageType in [
            .recipientRemove, .guildJoinRequestRejectNotification,
            .guildJoinRequestWithdrawnNotification,
        ] {
            #expect(
                icon(kind) == NoticeIcon(symbol: "arrow.left", color: Palette.noticeOut, size: 16),
                "\(kind)")
        }
    }

    @Test
    func aMissedCallIsGreyAndMirroredAnAnsweredOneGreen() {
        let missed = MessageCall(participants: [UserId(rawValue: 2)], endedTimestamp: noon)
        let answered = MessageCall(participants: [UserId(rawValue: 2), me], endedTimestamp: noon)

        #expect(
            icon(.call, call: missed)
                == NoticeIcon(
                    symbol: "phone.fill", color: Palette.missedCall, size: 16, mirrored: true))
        #expect(
            icon(.call, call: answered)
                == NoticeIcon(symbol: "phone.fill", color: Palette.noticeIn, size: 16))
    }

    @Test
    func theRestFollowDiscordsShapesAndColors() {
        let expected: [(MessageType, NoticeIcon)] = [
            (.channelNameChange, NoticeIcon(symbol: "pencil", color: Palette.channelDefault, size: 16)),
            (.channelIconChange, NoticeIcon(symbol: "pencil", color: Palette.channelDefault, size: 16)),
            (.premiumGuildSubscription, NoticeIcon(symbol: "hexagon.fill", color: Palette.boost, size: 16)),
            (.premiumGuildSubscriptionTier3, NoticeIcon(symbol: "hexagon.fill", color: Palette.boost, size: 16)),
            (.guildDiscoveryDisqualified, NoticeIcon(symbol: "xmark", color: Palette.noticeOut, size: 16)),
            (.guildDiscoveryRequalified, NoticeIcon(symbol: "checkmark", color: Palette.noticeIn, size: 16)),
            (.guildDiscoveryGracePeriodInitialWarning, NoticeIcon(symbol: "exclamationmark.circle.fill", color: Palette.warning, size: 16)),
            (.guildDiscoveryGracePeriodFinalWarning, NoticeIcon(symbol: "exclamationmark.circle.fill", color: Palette.warning, size: 16)),
            (.threadCreated, NoticeIcon(symbol: "text.bubble", color: Palette.channelDefault, size: 18)),
            (.threadStarterMessage, NoticeIcon(symbol: "text.bubble", color: Palette.channelDefault, size: 18)),
            (.stageStart, NoticeIcon(symbol: "person.wave.2.fill", color: Palette.statusPositive, size: 24)),
            (.stageEnd, NoticeIcon(symbol: "person.wave.2.fill", color: Palette.textMuted, size: 24)),
            (.stageTopic, NoticeIcon(symbol: "person.wave.2.fill", color: Palette.textMuted, size: 24)),
            (.stageSpeaker, NoticeIcon(symbol: "mic.fill", color: Palette.textMuted, size: 24)),
            (.stageRaiseHand, NoticeIcon(symbol: "hand.raised.fill", color: Palette.textMuted, size: 24)),
            (.guildApplicationPremiumSubscription, NoticeIcon(symbol: "ticket.fill", color: Palette.channelDefault, size: 16)),
            (.reportToModDeletedMessage, NoticeIcon(symbol: "trash.fill", color: Palette.channelDefault, size: 20)),
            (.reportToModTimeoutUser, NoticeIcon(symbol: "clock.badge.exclamationmark.fill", color: Palette.channelDefault, size: 20)),
            (.reportToModKickUser, NoticeIcon(symbol: "person.fill.badge.minus", color: Palette.channelDefault, size: 20)),
            (.reportToModBanUser, NoticeIcon(symbol: "hammer.fill", color: Palette.channelDefault, size: 20)),
            (.reportToModClosedReport, NoticeIcon(symbol: "checkmark", color: Palette.channelDefault, size: 20)),
            (.voiceSession, NoticeIcon(symbol: "speaker.wave.2.fill", color: Palette.statusPositive, size: 24)),
            (.friendRequestAccepted, NoticeIcon(symbol: "person.fill.checkmark", color: Palette.statusPositive, size: 18)),
            (.autoModerationAction, NoticeIcon(symbol: "info.circle", color: Palette.channelDefault, size: 16)),
        ]
        for (kind, icon) in expected {
            #expect(self.icon(kind) == icon, "\(kind)")
        }
    }

    // A name macOS 14 doesn't know would draw nothing.
    @Test
    func everySymbolExists() {
        let kinds: [MessageType] = [.`default`, .recipientAdd, .recipientRemove, .call,
            .channelNameChange, .channelIconChange, .channelPinnedMessage, .userJoin,
            .premiumGuildSubscription, .premiumGuildSubscriptionTier1,
            .premiumGuildSubscriptionTier2, .premiumGuildSubscriptionTier3, .channelFollowAdd,
            .guildDiscoveryDisqualified, .guildDiscoveryRequalified,
            .guildDiscoveryGracePeriodInitialWarning, .guildDiscoveryGracePeriodFinalWarning,
            .threadCreated, .reply, .chatInputCommand, .threadStarterMessage, .guildInviteReminder,
            .contextMenuCommand, .autoModerationAction, .roleSubscriptionPurchase,
            .interactionPremiumUpsell, .stageStart, .stageEnd, .stageSpeaker, .stageRaiseHand,
            .stageTopic, .guildApplicationPremiumSubscription, .premiumReferral,
            .guildIncidentAlertModeEnabled, .guildIncidentAlertModeDisabled,
            .guildIncidentReportRaid, .guildIncidentReportFalseAlarm, .guildDeadchatRevivePrompt,
            .customGift, .guildGamingStatsPrompt, .purchaseNotification, .pollResult, .changelog,
            .nitroNotification, .channelLinkedToLobby, .giftingPrompt, .inGameMessageNux,
            .guildJoinRequestAcceptNotification, .guildJoinRequestRejectNotification,
            .guildJoinRequestWithdrawnNotification, .hdStreamingUpgraded,
            .reportToModDeletedMessage, .reportToModTimeoutUser, .reportToModKickUser,
            .reportToModBanUser, .reportToModClosedReport, .premiumGroupInvite, .voiceSession,
            .guildBoostUpsell, .friendRequestAccepted, .mediaMentionMessage,
        ]
        for kind in kinds {
            let symbol = icon(kind).symbol
            #expect(NSImage(systemSymbolName: symbol, accessibilityDescription: nil) != nil, "\(symbol)")
        }
    }

    // Discord's glyphs sit inside their icon box; SF's wide ones would spill past it.
    @Test
    func anIconFitsItsBox() throws {
        let list = MessageTableTests()
        list.show([row(1, by: author(2, "Ray"), at: noon, kind: .stageStart, content: "Town hall")])
        let cell = try #require(try list.cell(of: 1) as? NoticeCell)

        let frame = try #require(cell.iconFrame)
        #expect(frame.width == 24 && frame.height == 24, "\(frame)")
        #expect(cell.iconFitsItsFrame)
    }

    @Test
    func theLineDrawsItsIcon() throws {
        let list = MessageTableTests()
        list.show([row(1, by: author(2, "Ray"), at: noon, kind: .userJoin, content: "")])
        let cell = try #require(try list.cell(of: 1) as? NoticeCell)

        #expect(cell.shownIcon == NoticeIcon(symbol: "arrow.right", color: Palette.noticeIn, size: 16))
    }
}
