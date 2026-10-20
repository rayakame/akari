import Foundation

// Discord's wording, from its client (en-US strings and its system-message renderer, read
// 2026-10-20); docs/ui/message-list.md has the details.
extension Message {
    /// A piece of a notice: the parts Discord draws as names and links, and the text between.
    public enum NoticePart: Equatable, Sendable {
        case text(String)
        /// Who did it.
        case actor(String)
        /// Another name, or Discord's bold text, drawn as names are: in Discord also its links
        /// whose targets Akari doesn't have yet.
        case strong(String)
        /// A new thread's name, which opens it; `nil` when the message doesn't say which.
        case thread(name: String, id: ChannelId?)
        /// "threads" in "See all threads.", for the channel's threads.
        case threads(String)
        /// Jumps to a message: a pin's "a message", a poll result's title.
        case message(String, id: MessageId)
        /// "pinned messages" in "See all pinned messages.", for the channel's pins.
        case pins(String)
        /// A channel, which opens it.
        case channel(String, id: ChannelId)
        /// A server, which opens it.
        case guild(String, id: GuildId)

        var text: String {
            switch self {
            case .text(let text), .actor(let text), .strong(let text), .thread(let text, _),
                .threads(let text), .message(let text, _), .pins(let text),
                .channel(let text, _), .guild(let text, _):
                text
            }
        }
    }

    /// Where a system message is and who reads it: what parts of its line depend on.
    public struct NoticeContext: Equatable, Sendable {
        public enum Place: Equatable, Sendable {
            /// A server's text or announcement channel.
            case channel
            case thread
            /// A thread in a forum or media channel.
            case forumPost
            case groupDM
            case dm
        }

        public var currentUserId: UserId?
        public var place: Place
        public var guildId: GuildId?
        public var guildName: String?
        /// The thread a "started a thread" line names, as the store has it now.
        public var threadName: String?
        /// The channel an AutoMod alert names, if the store has it; its ID while the user may
        /// open it.
        public var alertChannelName: String?
        public var alertChannelId: ChannelId?
        /// A DM's other person.
        public var recipientName: String?
        /// A voice session's participants the store knows, without its author.
        public var participantNames: [String] = []
        /// For dates in a line, such as when security actions end; `nil` is the user's.
        public var calendar: Calendar?

        public init(currentUserId: UserId? = nil, place: Place = .channel) {
            self.currentUserId = currentUserId
            self.place = place
        }
    }

    /// What a system message says in parts, e.g. "Ray" and " just landed."; `nil` for the
    /// messages Discord draws as people's (default, replies, commands, and a few more), empty
    /// for the kinds it shows no line for.
    public func noticeParts(in context: NoticeContext) -> [NoticePart]? {
        parts(in: context).map(Self.merged)
    }

    /// `noticeParts` as one sentence.
    public func notice(in context: NoticeContext) -> String? {
        noticeParts(in: context)?.map(\.text).joined()
    }

    private func parts(in context: NoticeContext) -> [NoticePart]? {
        let name = author.displayName
        let actor = NoticePart.actor(name)
        let mentioned = mentions.first?.displayName
        switch kind {
        case .default, .reply, .chatInputCommand, .contextMenuCommand, .threadStarterMessage,
            .interactionPremiumUpsell, .customGift, .changelog, .mediaMentionMessage:
            return nil
        case .unknown(let raw):
            // A chat wallpaper set or removed, an emoji added: Discord draws no line.
            return [56, 57, 63].contains(raw) ? [] : nil
        // Not drawn by Discord, a card Akari can't draw yet, or made by the client itself.
        case .guildInviteReminder, .guildDeadchatRevivePrompt, .guildGamingStatsPrompt,
            .hdStreamingUpgraded, .premiumReferral, .premiumGroupInvite, .nitroNotification,
            .giftingPrompt, .inGameMessageNux, .guildBoostUpsell, .channelLinkedToLobby:
            return []
        case .recipientAdd:
            let thread = context.place == .thread || context.place == .forumPost
            return [
                actor, .text(" added "), .strong(mentioned ?? (thread ? "???" : "unknown-user")),
                .text(thread ? " to the thread." : " to the group."),
            ]
        case .recipientRemove:
            if context.place == .thread || context.place == .forumPost {
                return [
                    actor, .text(" removed "), .strong(mentioned ?? "???"),
                    .text(" from the thread."),
                ]
            }
            guard let target = mentions.first, target.id != author.id else {
                return [actor, .text(" left the group.")]
            }
            return [
                actor, .text(" removed "), .strong(target.displayName), .text(" from the group."),
            ]
        case .call:
            return callParts(actor, in: context)
        case .channelNameChange:
            switch context.place {
            case .groupDM where content.isEmpty:
                return [actor, .text(" removed the custom group name. "), .strong("Edit Group")]
            case .groupDM:
                return [
                    actor, .text(" changed the group name: "), .strong(content), .text(". "),
                    .strong("Edit Group"),
                ]
            case .forumPost:
                return [actor, .text(" changed the post title: "), .strong(content)]
            default:
                return [actor, .text(" changed the channel name: "), .strong(content)]
            }
        case .channelIconChange:
            return [actor, .text(" changed the group icon. "), .strong("Edit Group")]
        case .channelPinnedMessage:
            let pinned: [NoticePart] =
                if let id = reference?.messageId {
                    [
                        .text(" pinned "), .message("a message", id: id),
                        .text(" to this channel. See all "),
                    ]
                } else {
                    [.text(" pinned a message to this channel. See all ")]
                }
            return [actor] + pinned + [.pins("pinned messages"), .text(".")]
        case .userJoin:
            return joinParts(actor)
        case .premiumGuildSubscription:
            return boostParts(actor, level: nil, in: context)
        case .premiumGuildSubscriptionTier1:
            return boostParts(actor, level: 1, in: context)
        case .premiumGuildSubscriptionTier2:
            return boostParts(actor, level: 2, in: context)
        case .premiumGuildSubscriptionTier3:
            return boostParts(actor, level: 3, in: context)
        case .channelFollowAdd:
            return [
                actor, .text(" has added "), .strong(content),
                .text(" to this channel. Its most important updates will show up here."),
            ]
        case .guildDiscoveryDisqualified:
            return [
                Self.server(in: context),
                .text(
                    " has been removed from Server Discovery because it no longer passes all"
                        + " the requirements. Check "),
                .strong("Server Settings"), .text(" for more details."),
            ]
        case .guildDiscoveryRequalified:
            return [
                .text(
                    "This server is eligible for Server Discovery again and has been"
                        + " automatically relisted!")
            ]
        case .guildDiscoveryGracePeriodInitialWarning:
            return [
                Self.server(in: context),
                .text(
                    " has failed Discovery activity requirements for 1 week. If it fails for 4"
                        + " weeks in a row, it will be automatically removed from Discovery."),
            ]
        case .guildDiscoveryGracePeriodFinalWarning:
            return [
                Self.server(in: context),
                .text(
                    " has failed Discovery activity requirements for 3 weeks in a row. If it"
                        + " fails for 1 more week, it will be removed from Discovery."),
            ]
        case .threadCreated:
            return [
                actor, .text(" started a thread: "),
                .thread(name: context.threadName ?? content, id: reference?.channelId),
                .text(". See all "), .threads("threads"), .text("."),
            ]
        case .autoModerationAction:
            let channel: NoticePart =
                if let name = context.alertChannelName {
                    context.alertChannelId.map { .channel(name, id: $0) } ?? .strong(name)
                } else {
                    .strong("unknown-channel")
                }
            let outcome = embedField("decision_outcome") == "blocked" ? "blocked" : "flagged"
            return [.strong("AutoMod"), .text(" has \(outcome) a message in "), channel]
        case .roleSubscriptionPurchase:
            return subscriptionParts(actor, in: context)
        case .stageStart:
            return [actor, .text(" started "), .strong(content)]
        case .stageEnd:
            return [actor, .text(" ended "), .strong(content)]
        case .stageSpeaker:
            return [actor, .text(" is now a speaker.")]
        case .stageRaiseHand:
            return [actor, .text(" requested to speak.")]
        case .stageTopic:
            return [actor, .text(" changed the Stage topic: "), .strong(content)]
        case .guildApplicationPremiumSubscription:
            let app = applicationName ?? "a deleted application"
            return [actor, .text(" upgraded \(app) to premium for this server! 🎉")]
        case .guildIncidentAlertModeEnabled:
            return [actor, .text(" enabled security actions until \(securityEnd(in: context)).")]
        case .guildIncidentAlertModeDisabled:
            return [actor, .text(" disabled security actions.")]
        case .guildIncidentReportRaid:
            return [actor, .text(" reported a raid in \(context.guildName ?? "").")]
        case .guildIncidentReportFalseAlarm:
            return [actor, .text(" resolved an Activity Alert.")]
        case .purchaseNotification:
            guard let purchasedProduct else {
                return []
            }
            return [actor, .text(" has purchased "), .strong(purchasedProduct), .text("!")]
        case .pollResult:
            let title = Self.pollTitle(embedField("poll_question_text") ?? "")
            let linked = reference?.messageId.map { NoticePart.message(title, id: $0) }
            return [actor, .text("'s poll "), linked ?? .strong(title), .text(" has closed.")]
        // Discord's own words when the client doesn't have the join request.
        case .guildJoinRequestAcceptNotification:
            return [.text("This application was approved! Welcome!")]
        case .guildJoinRequestRejectNotification:
            return [.text("This application was rejected.")]
        case .guildJoinRequestWithdrawnNotification:
            return [.text("This application has been withdrawn.")]
        case .reportToModDeletedMessage:
            return [actor, .text(" deleted the message")]
        case .reportToModTimeoutUser:
            return [actor, .text(" timed out "), .strong(mentioned ?? "???")]
        case .reportToModKickUser:
            return [actor, .text(" kicked "), .strong(mentioned ?? "???")]
        case .reportToModBanUser:
            return [actor, .text(" banned "), .strong(mentioned ?? "???")]
        case .reportToModClosedReport:
            return [actor, .text(" resolved this flag")]
        case .voiceSession:
            return voiceSessionParts(actor, in: context)
        case .friendRequestAccepted:
            return friendParts(in: context)
        }
    }

    private func callParts(_ actor: NoticePart, in context: NoticeContext) -> [NoticePart] {
        let running = call.map { $0.endedTimestamp == nil } ?? false
        let missed =
            !running && call.map { !$0.participants.contains { $0 == context.currentUserId } }
            == true
        let lasted = lastedFor.map { " that lasted \($0)" } ?? ""
        var parts: [NoticePart] =
            missed
            ? [.text("You missed a call from "), actor, .text("\(lasted).")]
            : [actor, .text(" started a call\(lasted).")]
        // Akari can't join calls: the action is strong text.
        if running {
            parts += [.text(" — "), .strong("Join the call")]
        }
        return parts
    }

    private var lastedFor: String? {
        call?.endedTimestamp.map { Self.humanized($0.timeIntervalSince(timestamp)) }
    }

    private static let welcomes = [
        ("", " joined the party."), ("", " is here."),
        ("Welcome, ", ". We hope you brought pizza."), ("A wild ", " appeared."),
        ("", " just landed."), ("", " just slid into the server."), ("", " just showed up!"),
        ("Welcome ", ". Say hi!"), ("", " hopped into the server."), ("Everyone welcome ", "!"),
        ("Glad you're here, ", "."), ("Good to see you, ", "."), ("Yay you made it, ", "!"),
    ]

    private func joinParts(_ actor: NoticePart) -> [NoticePart] {
        if author.bot {
            guard let adder = self.actor else {
                return [actor, .text(" was added to the server.")]
            }
            return [
                .actor(adder.displayName), .text(" has added "), .strong(author.displayName),
                .text(" to the server."),
            ]
        }
        // The ID's time through a double, as JavaScript's Number(id) takes it.
        let millis = (Double(id.rawValue) / 4_194_304).rounded(.down) + 1_420_070_400_000
        let (before, after) = Self.welcomes[Int(millis.truncatingRemainder(dividingBy: 13))]
        return [.text(before), actor, .text(after)]
    }

    private func boostParts(
        _ actor: NoticePart, level: Int?, in context: NoticeContext
    ) -> [NoticePart] {
        // JavaScript's Number(content): empty or not a number is one boost.
        let number = Double(content.trimmingCharacters(in: .whitespacesAndNewlines)) ?? 0
        let count = !number.isFinite || number == 0 ? 1 : number
        // Whole numbers below 1e21 print without an exponent, as in JavaScript.
        let written =
            count == count.rounded() && abs(count) < 1e21
            ? String(format: "%.0f", count) : String(count)
        var parts: [NoticePart] =
            count > 1
            ? [actor, .text(" just boosted the server "), .strong(written), .text(" times!")]
            : [actor, .text(" just boosted the server!")]
        if let level, let guild = context.guildName {
            parts += [.text(" \(guild) has achieved "), .strong("Level \(level)!")]
        }
        return parts
    }

    private func subscriptionParts(_ actor: NoticePart, in context: NoticeContext) -> [NoticePart]
    {
        let tier = NoticePart.strong(roleSubscription?.tierName ?? "")
        let server = NoticePart.strong(context.guildName ?? "")
        let months = Int(roleSubscription?.totalMonthsSubscribed ?? 0)
        let renewal = roleSubscription?.isRenewal == true
        if months > 0 {
            return [
                actor, .text(renewal ? " renewed " : " joined "), tier,
                .text(" and has been a subscriber of "), server,
                .text(" for \(months) \(months == 1 ? "month" : "months")!"),
            ]
        }
        return renewal
            ? [actor, .text(" renewed "), tier, .text(" in their "), server, .text(" membership!")]
            : [actor, .text(" joined "), tier, .text(" as a subscriber of "), server, .text("!")]
    }

    private func voiceSessionParts(_ actor: NoticePart, in context: NoticeContext) -> [NoticePart]
    {
        guard let lasted = lastedFor else {
            return [actor, .text(" started "), .strong("a voice hangout"), .text(".")]
        }
        let names = context.participantNames
        var parts = [actor]
        switch names.count {
        case 0:
            break
        case 1:
            parts += [.text(" and "), .strong(names[0])]
        case 2:
            parts += [.text(", "), .strong(names[0]), .text(", and "), .strong(names[1])]
        default:
            parts += [
                .text(", "), .strong(names[0]), .text(", and "),
                .strong("\(names.count - 1) others"),
            ]
        }
        parts.append(.text((names.isEmpty ? " was" : " were") + " hanging out for \(lasted)."))
        return parts
    }

    // Only in a DM, about its other person.
    private func friendParts(in context: NoticeContext) -> [NoticePart] {
        guard context.place == .dm else {
            return []
        }
        let other = NoticePart.strong(context.recipientName ?? "???")
        if author.id == context.currentUserId {
            return content.isEmpty
                ? [.text("You accepted "), other, .text("'s friend request.")]
                : [
                    .text("You accepted "), other, .text("'s friend request. They said: "),
                    .strong(content),
                ]
        }
        return content.isEmpty
            ? [other, .text(" accepted your friend request.")]
            : [other, .text(" accepted your friend request. You said: "), .strong(content)]
    }

    private func securityEnd(in context: NoticeContext) -> String {
        guard !content.isEmpty else {
            return ""
        }
        guard let end = Self.isoDate(content) else {
            // What JavaScript's Date gives for text that isn't a date.
            return "Invalid Date"
        }
        let calendar = context.calendar ?? .current
        let style = Date.FormatStyle(
            locale: calendar.locale ?? .current, calendar: calendar, timeZone: calendar.timeZone)
        return end.formatted(
            style.month(.twoDigits).day(.twoDigits).year().hour().minute(.twoDigits))
    }

    private static func isoDate(_ text: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let date = formatter.date(from: text) {
            return date
        }
        formatter.formatOptions = [.withInternetDateTime]
        return formatter.date(from: text)
    }

    private static func server(in context: NoticeContext) -> NoticePart {
        guard let name = context.guildName else {
            return .strong("this server")
        }
        return context.guildId.map { .guild(name, id: $0) } ?? .strong(name)
    }

    private func embedField(_ name: String) -> String? {
        embedFields.first { $0.name == name }?.value
    }

    // JavaScript's \s.
    private static let jsSpaces: Set<Unicode.Scalar> = Set(
        [0x9, 0xA, 0xB, 0xC, 0xD, 0x20, 0xA0, 0x1680, 0x2028, 0x2029, 0x202F, 0x205F, 0x3000, 0xFEFF]
            .compactMap(Unicode.Scalar.init) + (0x2000...0x200A).compactMap(Unicode.Scalar.init))

    // Discord's cut: over 50 UTF-16 units, 49 code points (as many units without astral ones)
    // without trailing spaces and dots, then "…".
    static func pollTitle(_ title: String) -> String {
        guard title.utf16.count > 50 else {
            return title
        }
        var kept = Array(title.unicodeScalars.prefix(49))
        while let last = kept.last, last == "." || jsSpaces.contains(last) {
            kept.removeLast()
        }
        return String(String.UnicodeScalarView(kept)) + "…"
    }

    /// A call's length as Discord words it: moment 2.30.1's `humanize()` in English.
    static func humanized(_ seconds: TimeInterval) -> String {
        // JavaScript's Math.round: halves go up.
        func round(_ value: Double) -> Int {
            Int((value + 0.5).rounded(.down))
        }
        let seconds = abs(seconds)
        let months = seconds / 86400 * 4800 / 146_097
        let (minute, hour, day) = (round(seconds / 60), round(seconds / 3600), round(seconds / 86400))
        let (month, year) = (round(months), round(months / 12))
        return if round(seconds) < 45 {
            "a few seconds"
        } else if minute <= 1 {
            "a minute"
        } else if minute < 45 {
            "\(minute) minutes"
        } else if hour <= 1 {
            "an hour"
        } else if hour < 22 {
            "\(hour) hours"
        } else if day <= 1 {
            "a day"
        } else if day < 26 {
            "\(day) days"
        } else if month <= 1 {
            "a month"
        } else if month < 11 {
            "\(month) months"
        } else if year <= 1 {
            "a year"
        } else {
            "\(year) years"
        }
    }

    // One text part between two others, none empty.
    private static func merged(_ parts: [NoticePart]) -> [NoticePart] {
        parts.reduce(into: []) { merged, part in
            guard case .text(let text) = part else {
                merged.append(part)
                return
            }
            if text.isEmpty {
                return
            }
            if case .text(let previous)? = merged.last {
                merged[merged.count - 1] = .text(previous + text)
            } else {
                merged.append(part)
            }
        }
    }
}
