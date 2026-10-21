import AkariKit
import AppKit

/// A system line's icon as Discord's client draws it (its system-message renderer, read
/// 2026-10-21), in SF Symbols. Threads keep Akari's own thread symbol: Discord's is its asset.
struct NoticeIcon: Equatable {
    let symbol: String
    let color: NSColor
    /// Discord's icon box, in points; the symbol fits inside it.
    let size: CGFloat
    var mirrored = false
    /// Turned 45° clockwise, as Discord's pushpin leans.
    var slanted = false

    // A missed call's handset faces the other way.
    static func of(_ message: Message, reader: UserId?) -> NoticeIcon {
        let muted = Palette.channelDefault
        switch message.kind {
        case .userJoin, .recipientAdd, .channelFollowAdd, .roleSubscriptionPurchase,
            .purchaseNotification, .guildJoinRequestAcceptNotification:
            return NoticeIcon(symbol: "arrow.right", color: Palette.noticeIn, size: 16)
        case .recipientRemove, .guildJoinRequestRejectNotification,
            .guildJoinRequestWithdrawnNotification:
            return NoticeIcon(symbol: "arrow.left", color: Palette.noticeOut, size: 16)
        case .call:
            return message.isMissedCall(by: reader)
                ? NoticeIcon(symbol: "phone.fill", color: Palette.missedCall, size: 16, mirrored: true)
                : NoticeIcon(symbol: "phone.fill", color: Palette.noticeIn, size: 16)
        case .channelNameChange, .channelIconChange:
            return NoticeIcon(symbol: "pencil", color: muted, size: 16)
        case .channelPinnedMessage:
            return NoticeIcon(symbol: "pin.fill", color: muted, size: 18, slanted: true)
        case .premiumGuildSubscription, .premiumGuildSubscriptionTier1,
            .premiumGuildSubscriptionTier2, .premiumGuildSubscriptionTier3:
            return NoticeIcon(symbol: "hexagon.fill", color: Palette.boost, size: 16)
        case .guildDiscoveryDisqualified:
            return NoticeIcon(symbol: "xmark", color: Palette.noticeOut, size: 16)
        case .guildDiscoveryRequalified:
            return NoticeIcon(symbol: "checkmark", color: Palette.noticeIn, size: 16)
        case .guildDiscoveryGracePeriodInitialWarning, .guildDiscoveryGracePeriodFinalWarning:
            return NoticeIcon(symbol: "exclamationmark.circle.fill", color: Palette.warning, size: 16)
        case .threadCreated, .threadStarterMessage:
            return NoticeIcon(symbol: "text.bubble", color: muted, size: 18)
        case .stageStart:
            return NoticeIcon(symbol: "person.wave.2.fill", color: Palette.statusPositive, size: 24)
        case .stageEnd, .stageTopic:
            return NoticeIcon(symbol: "person.wave.2.fill", color: Palette.textMuted, size: 24)
        case .stageSpeaker:
            return NoticeIcon(symbol: "mic.fill", color: Palette.textMuted, size: 24)
        case .stageRaiseHand:
            return NoticeIcon(symbol: "hand.raised.fill", color: Palette.textMuted, size: 24)
        case .guildApplicationPremiumSubscription:
            return NoticeIcon(symbol: "ticket.fill", color: muted, size: 16)
        case .pollResult:
            return NoticeIcon(symbol: "text.alignleft", color: Palette.interactiveText, size: 16)
        case .reportToModDeletedMessage:
            return NoticeIcon(symbol: "trash.fill", color: muted, size: 20)
        case .reportToModTimeoutUser:
            return NoticeIcon(symbol: "clock.badge.exclamationmark.fill", color: muted, size: 20)
        case .reportToModKickUser:
            return NoticeIcon(symbol: "person.fill.badge.minus", color: muted, size: 20)
        case .reportToModBanUser:
            return NoticeIcon(symbol: "hammer.fill", color: muted, size: 20)
        case .reportToModClosedReport:
            return NoticeIcon(symbol: "checkmark", color: muted, size: 20)
        case .voiceSession:
            return NoticeIcon(symbol: "speaker.wave.2.fill", color: Palette.statusPositive, size: 24)
        case .friendRequestAccepted:
            return NoticeIcon(symbol: "person.fill.checkmark", color: Palette.statusPositive, size: 18)
        // AutoMod's lines are a message of their own in Discord's client, which Akari doesn't
        // draw yet.
        default:
            return NoticeIcon(symbol: "info.circle", color: muted, size: 16)
        }
    }

    var image: NSImage? {
        guard
            let symbol = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
                .withSymbolConfiguration(.init(pointSize: size, weight: .regular))
        else {
            return nil
        }
        guard mirrored || slanted else {
            return symbol
        }
        // Around the glyph itself: a symbol's image has room around it, which its alignment
        // rect leaves out.
        let glyph = symbol.alignmentRect
        // A w × h glyph turned 45° fills a square of (w + h) / √2.
        let side = (glyph.width + glyph.height) / 2.squareRoot()
        let size = slanted ? NSSize(width: side, height: side) : glyph.size
        let image = NSImage(size: size, flipped: false) { [mirrored, slanted] rect in
            let transform = NSAffineTransform()
            transform.translateX(by: rect.midX, yBy: rect.midY)
            if mirrored {
                transform.scaleX(by: -1, yBy: 1)
            }
            if slanted {
                transform.rotate(byDegrees: -45)
            }
            transform.concat()
            symbol.draw(
                in: NSRect(
                    x: -glyph.midX, y: -glyph.midY, width: symbol.size.width,
                    height: symbol.size.height))
            return true
        }
        image.isTemplate = true
        return image
    }
}
