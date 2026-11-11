import Foundation
import OSLog

/// `-AkariLog images`: why an image or an animation doesn't show, and an animation's frames as
/// they pass, in the unified log and on stderr beside akari-core's lines. Image keys (Discord's
/// IDs), sizes and counts only, never links or file names.
@MainActor
public enum ImageLog {
    public static var enabled = false
    private static let logger = Logger(subsystem: "app.akari", category: "images")

    public static var write: (String) -> Void = { text in
        logger.debug("\(text, privacy: .public)")
        FileHandle.standardError.write(Data("images: \(text)\n".utf8))
    }

    public static func isAsked(by filter: String) -> Bool {
        filter.split(separator: ",").contains { directive in
            directive.split(separator: "=").first?.trimmingCharacters(in: .whitespaces)
                == "images"
        }
    }

    public static func log(_ message: @autoclosure () -> String) {
        guard enabled else {
            return
        }
        write(message())
    }

    /// A key by Discord's IDs.
    public static func name(_ key: ImageKey) -> String {
        switch key {
        case .avatar(let user, _): "avatar (user \(user.rawValue))"
        case .memberAvatar(let guild, let user, _): "member avatar (user \(user.rawValue), server \(guild.rawValue))"
        case .defaultAvatar(let index): "default avatar \(index)"
        case .guildIcon(let guild, _): "server icon (server \(guild.rawValue))"
        case .groupIcon(let channel, _): "group icon (channel \(channel.rawValue))"
        case .roleIcon(let role, _): "role icon (role \(role.rawValue))"
        case .emoji(let emoji, _): "emoji \(emoji.rawValue)"
        case .sticker(let sticker, _): "sticker \(sticker.rawValue)"
        case .attachment(_, let message, let attachment):
            "attachment \(attachment.rawValue) (message \(message.rawValue))"
        case .embedMedia(_, let message, let embed, let slot):
            "embed \(embed) \(name(slot)) (message \(message.rawValue))"
        }
    }

    private static func name(_ slot: EmbedSlot) -> String {
        switch slot {
        case .image: "image"
        case .thumbnail: "thumbnail"
        case .authorIcon: "author icon"
        case .footerIcon: "footer icon"
        case .video: "video"
        }
    }

    /// Why a load failed, without what Discord or the file system said.
    public static func reason(_ error: any Error) -> String {
        switch error {
        case let error as ImageError:
            switch error {
            case .NotFound: "Discord has no such image"
            case .Unavailable: "not available now"
            case .TooLarge: "over the size cap"
            case .NotAnImage: "not an image"
            case .UnknownKey: "its message isn't in the store"
            case .Unsupported: "not supported"
            case .Io: "couldn't be written to disk"
            case .Closed: "the account is closed"
            case .Cancelled: "cancelled"
            case .Failed: "failed"
            }
        case ImageCacheError.tooLarge: "more pixels than Akari decodes"
        case ImageCacheError.undecodable: "the file doesn't decode"
        case is CancellationError: "cancelled"
        default: "failed"
        }
    }
}
