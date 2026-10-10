import AkariKit

/// SF Symbols for Discord's channel icons (docs/ui/channel-list.md).
enum ChannelSymbols {
    /// Added to a channel's symbol when it's marked NSFW.
    static let nsfwBadge = "exclamationmark.triangle.fill"

    static func name(for kind: ChannelType) -> String {
        switch kind {
        case .guildText: "number"
        case .guildNews: "megaphone"
        case .guildVoice: "speaker.wave.2"
        case .guildStageVoice: "person.wave.2"
        case .guildForum: "bubble.left.and.text.bubble.right"
        case .guildMedia: "photo.on.rectangle"
        case .guildDirectory: "folder"
        case .guildCategory: "folder"
        case .newsThread, .publicThread, .privateThread: "text.bubble"
        case .dm: "at"
        case .groupDm: "person.2.fill"
        default: "questionmark.square.dashed"
        }
    }

    /// What a row that doesn't open says on hover.
    static func unsupported(_ kind: ChannelType) -> String {
        switch kind {
        case .guildVoice: "Voice channels aren't supported yet"
        case .guildStageVoice: "Stage channels aren't supported yet"
        case .guildForum: "Forum channels aren't supported yet"
        case .guildMedia: "Media channels aren't supported yet"
        case .guildDirectory: "Directory channels aren't supported yet"
        default: "This kind of channel isn't supported yet"
        }
    }
}
