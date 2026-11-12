/// What the line over a reply shows of the message it answers.
public struct ReplyLine: Equatable, Sendable {
    public enum Content: Equatable, Sendable {
        /// The message's text on one line; `edited` appends "(edited)".
        case text(RichText, edited: Bool)
        /// The official client's words where there is no text to show, in italics.
        case placeholder(String)
    }

    public static let deleted = "Original message was deleted"
    public static let notLoaded = "Message could not be loaded"

    /// The answered message's own channel: a thread's first row answers in the parent channel.
    public let channelId: ChannelId
    public let messageId: MessageId
    public let target: ReplyTarget
    /// The reply mentions the answered message's author: the name gets its "@".
    public let mentionsAuthor: Bool
    /// The author's name by the rule the list names people with; `nil` unless loaded.
    public let authorName: String?

    public init(
        channelId: ChannelId, messageId: MessageId, target: ReplyTarget, mentionsAuthor: Bool,
        authorName: String?
    ) {
        self.channelId = channelId
        self.messageId = messageId
        self.target = target
        self.mentionsAuthor = mentionsAuthor
        self.authorName = authorName
    }

    private var preview: ReplyPreview? {
        if case .loaded(let preview) = target { preview } else { nil }
    }

    /// Whose avatar the line shows; a deleted or not loaded message shows a badge instead.
    public var author: User? {
        preview?.author
    }

    /// The name as shown, with "@" when the reply mentions them.
    public var name: String? {
        guard preview != nil, let authorName else {
            return nil
        }
        return (mentionsAuthor ? "@" : "") + authorName
    }

    /// A click on the line goes to the message; not once it is deleted.
    public var jumps: Bool {
        target != .deleted
    }

    public var content: Content {
        switch target {
        case .deleted: return .placeholder(Self.deleted)
        case .notLoaded: return .placeholder(Self.notLoaded)
        case .loaded(let preview):
            if preview.kind == .channelPinnedMessage {
                return .placeholder("Message pinned.")
            }
            if let text = preview.formatted {
                return .text(text, edited: preview.edited)
            }
            let words =
                switch Self.media(of: preview) {
                case .sticker: "Click to see sticker"
                case .command: "Click to see command"
                case .voice: "Click to see voice message"
                case .attachment: "Click to see attachment"
                case nil: Self.isPlain(preview.kind) ? "Click to see attachment" : "Click to see message"
                }
            return .placeholder(words)
        }
    }

    /// An SF Symbol before the content: a forward's arrow.
    public var leadingSymbol: String? {
        preview?.forwarded == true ? "arrowshape.turn.up.right" : nil
    }

    /// An SF Symbol after the content, for what the message has besides its text.
    public var trailingSymbol: String? {
        guard let preview, preview.kind != .channelPinnedMessage else {
            return nil
        }
        switch Self.media(of: preview) {
        case .sticker: return "face.smiling"
        case .command: return "terminal"
        case .voice: return "mic.fill"
        case .attachment: return "photo"
        case nil: return preview.formatted == nil && Self.isPlain(preview.kind) ? "photo" : nil
        }
    }

    /// The line for VoiceOver.
    public var accessibilityLabel: String {
        let text =
            switch content {
            case .text(let text, _): text.text
            case .placeholder(let words): words
            }
        return authorName.map { "Replying to \($0): \(text)" } ?? text
    }

    private enum Media {
        case sticker, command, voice, attachment
    }

    // The official client's order: the first that applies.
    private static func media(of preview: ReplyPreview) -> Media? {
        if preview.hasSticker {
            .sticker
        } else if preview.kind == .chatInputCommand || preview.kind == .contextMenuCommand {
            .command
        } else if preview.voiceMessage {
            .voice
        } else if preview.hasAttachments || preview.hasEmbeds {
            .attachment
        } else {
            nil
        }
    }

    private static func isPlain(_ kind: MessageType) -> Bool {
        kind == .default || kind == .reply
    }
}
