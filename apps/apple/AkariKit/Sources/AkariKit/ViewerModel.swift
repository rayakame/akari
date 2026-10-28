import AkariFFI
import Foundation
import Observation

/// The media viewer (D146): the items a click opens, the one showing, and the actions on it.
@MainActor @Observable
public final class ViewerModel {
    public struct Item: Equatable, Sendable, Identifiable {
        public enum Kind: Equatable, Sendable {
            case image
            case video
            case gifv
        }

        public let kind: Kind
        /// The image; a video's or a GIFV's poster.
        public let key: ImageKey
        /// What plays; its original is the copy that plays.
        public let video: ImageKey?
        /// Pixels.
        public let width: UInt32
        public let height: UInt32
        /// Discord's thumbhash.
        public let placeholder: String?
        public let animated: Bool
        /// The file's name, an attachment's.
        public let filename: String?
        /// The name Discord shows: the title and the extension when it gave one.
        public let shownName: String?
        /// Bytes, an attachment's.
        public let size: UInt64?
        /// Alt text.
        public let description: String?
        public let isAttachment: Bool

        public var id: ImageKey { key }

        public init(_ attachment: Attachment) {
            let video = attachment.kind == .video
            kind = video ? .video : .image
            key = attachment.key
            self.video = video ? attachment.key : nil
            width = attachment.width
            height = attachment.height
            placeholder = attachment.placeholder
            animated = attachment.animated
            filename = attachment.filename
            shownName = FileType.shownName(attachment)
            size = attachment.size
            description = attachment.description
            isAttachment = true
        }

        public init(_ image: EmbedImage, video: ImageKey? = nil) {
            kind = video == nil ? .image : .gifv
            key = image.key
            self.video = video
            width = image.width
            height = image.height
            placeholder = image.placeholder
            animated = image.animated
            filename = nil
            shownName = nil
            size = nil
            description = nil
            isAttachment = false
        }
    }

    public let items: [Item]
    public private(set) var index: Int
    public let author: User
    /// The header's name, as the list resolves it.
    public let authorName: String
    public let timestamp: Date
    /// The channel hides download options: no Save, Copy Image or Open in Browser.
    public let hidesDownloads: Bool
    @ObservationIgnored private let account: Account

    public init?(
        items: [Item], index: Int, author: User, authorName: String, timestamp: Date,
        hidesDownloads: Bool, account: Account
    ) {
        guard items.indices.contains(index) else {
            return nil
        }
        self.items = items
        self.index = index
        self.author = author
        self.authorName = authorName
        self.timestamp = timestamp
        self.hidesDownloads = hidesDownloads
        self.account = account
    }

    public enum Action: Equatable, Sendable {
        case zoom
        case save
        case openInBrowser
        case copyImage
        case copyLink
        case details
    }

    public struct Detail: Equatable, Sendable {
        public let title: String
        public let value: String
    }

    public var current: Item { items[index] }

    public var actions: [Action] { Self.actions(for: current, hidesDownloads: hidesDownloads) }

    /// What the viewer asks for early (module 947056): the previous image with two items or more,
    /// the next with three or more.
    public var preloads: [Item] {
        var preloads: [Item] = []
        if items.count >= 2 {
            preloads.append(items[(index + items.count - 1) % items.count])
        }
        if items.count >= 3 {
            preloads.append(items[(index + 1) % items.count])
        }
        return preloads.filter { $0.kind != .video }
    }

    public func next() {
        index = (index + 1) % items.count
    }

    public func previous() {
        index = (index + items.count - 1) % items.count
    }

    public func select(_ index: Int) {
        if items.indices.contains(index) {
            self.index = index
        }
    }

    /// The original for Open in Browser and Copy Media Link; a video's is what plays.
    public func originalURL(of item: Item) async -> URL? {
        guard let link = try? await account.originalUrl(key: item.video ?? item.key) else {
            return nil
        }
        return URL(string: link)
    }

    /// Saves the original to `destination`; cancelling the task leaves the file as it was.
    public func save(_ item: Item, to destination: URL) async throws -> UInt64 {
        try await account.saveOriginal(item.video ?? item.key, to: destination)
    }

    /// A message's images and videos that `shows` lets through, in its order, from the clicked
    /// one; `nil` when that one isn't among them.
    public static func attachments(
        of message: Message, from id: AttachmentId, shows: (Attachment) -> Bool
    ) -> (items: [Item], index: Int)? {
        let media = message.attachments.filter { $0.display == .media && shows($0) }
        guard let index = media.firstIndex(where: { $0.id == id }) else {
            return nil
        }
        return (media.map(Item.init), index)
    }

    /// An embed's images (a gallery's, or its one), or its GIFV, from the clicked one.
    public static func embed(_ embed: Embed, from key: ImageKey) -> (items: [Item], index: Int)? {
        if let video = embed.video, embed.kind == .gifv, video.poster.key == key {
            return ([Item(video.poster, video: video.proxied ? video.key : nil)], 0)
        }
        let images = embed.media + (embed.thumbnail.map { [$0] } ?? [])
        guard let clicked = images.first(where: { $0.key == key }) else {
            return nil
        }
        if embed.media.contains(clicked) {
            return (embed.media.map { Item($0) }, embed.media.firstIndex(of: clicked) ?? 0)
        }
        return ([Item(clicked)], 0)
    }

    /// The viewer's buttons and menu items for `item` (module 947056); a channel that hides
    /// download options leaves its attachments only Zoom.
    public static func actions(for item: Item, hidesDownloads: Bool) -> [Action] {
        if hidesDownloads, item.isAttachment {
            return item.kind == .video ? [] : [.zoom]
        }
        var actions: [Action] = []
        if item.kind != .video {
            actions.append(.zoom)
        }
        if item.kind != .gifv {
            actions.append(.save)
        }
        actions.append(.openInBrowser)
        if item.kind == .image, !item.animated {
            actions.append(.copyImage)
        }
        actions.append(.copyLink)
        if item.isAttachment {
            actions.append(.details)
        }
        return actions
    }

    /// View Details, an attachment's: its name, its size in pixels and Discord's megabytes, its
    /// description.
    public static func details(of item: Item) -> [Detail] {
        guard item.isAttachment else {
            return []
        }
        var details = [
            Detail(title: "Filename", value: item.shownName ?? item.filename ?? ""),
            Detail(title: "Size", value: "\(item.width)x\(item.height) (\(megabytes(item.size ?? 0)))"),
        ]
        if let description = item.description, !description.isEmpty {
            details.append(Detail(title: "Description (Alt Text)", value: description))
        }
        return details
    }

    // Module 255438: whole megabytes rounded up, from 1000 on gigabytes with one decimal.
    static func megabytes(_ bytes: UInt64) -> String {
        let megabytes = (Double(bytes) / 1_000_000).rounded(.up)
        guard megabytes >= 1000 else {
            return "\(Int(megabytes)) MB"
        }
        let gigabytes = (megabytes / 100).rounded() / 10
        return gigabytes == gigabytes.rounded() ? "\(Int(gigabytes)) GB" : "\(gigabytes) GB"
    }
}
