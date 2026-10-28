import AkariFFI
import Foundation
import Testing

@testable import AkariKit

// The media viewer's model (D146): which items a click opens and where it starts, next and
// previous wrapping, the neighbours to preload, and the actions on an item.
@MainActor
struct ViewerModelTests {
    let account = FakeAccount()

    func attachment(
        _ raw: UInt64, _ name: String, kind: AttachmentKind = .image, display: AttachmentDisplay = .media,
        size: UInt64 = 1_572_864, description: String? = nil, cover: MediaCover? = nil,
        title: String? = nil, animated: Bool = false
    ) -> AkariFFI.Attachment {
        AkariFFI.Attachment(
            id: AttachmentId(rawValue: raw), filename: name, title: title, description: description,
            contentType: nil, size: size, kind: kind, width: 1920, height: 1080, placeholder: nil,
            spoiler: cover == .spoiler, animated: animated,
            key: .attachment(channelId: id(10), messageId: id(1), attachmentId: AttachmentId(rawValue: raw)),
            display: display, cover: cover)
    }

    func image(_ embed: UInt8, slot: EmbedSlot = .image) -> EmbedImage {
        EmbedImage(
            key: .embedMedia(channelId: id(10), messageId: id(1), embed: embed, slot: slot), width: 640,
            height: 480, placeholder: nil, animated: false)
    }

    func embed(kind: EmbedKind = .image, media: [EmbedImage] = [], video: EmbedVideo? = nil) -> Embed {
        Embed(
            index: 0, kind: kind, inline: true, color: nil, url: "https://example.com/", provider: nil,
            author: nil, title: nil, description: nil, fields: [], media: media, video: video,
            thumbnail: nil, footer: nil, tall: false, cover: nil)
    }

    func model(_ items: [ViewerModel.Item], at index: Int = 0, hidesDownloads: Bool = false) throws -> ViewerModel {
        try #require(
            ViewerModel(
                items: items, index: index, author: user(2), authorName: "Ray",
                timestamp: Date(timeIntervalSince1970: 1_700_000_000), hidesDownloads: hidesDownloads,
                account: account))
    }

    // Module 256905: the message's images and videos, not its files, from the clicked one; a
    // covered item stays out (D143: nothing under a cover shows).
    @Test
    func aMessagesMediaOpensFromTheClickedOne() throws {
        var source = message(1)
        source.attachments = [
            attachment(1, "a.png"), attachment(2, "notes.zip", kind: .file, display: .file),
            attachment(3, "clip.mp4", kind: .video), attachment(4, "secret.png", cover: .spoiler),
            attachment(5, "b.png"),
        ]

        let opened = try #require(
            ViewerModel.attachments(of: source, from: AttachmentId(rawValue: 3)) { $0.cover == nil })

        #expect(opened.items.map(\.key) == [0, 2, 4].map { source.attachments[$0].key })
        #expect(opened.index == 1)
        #expect(opened.items.map(\.kind) == [.image, .video, .image])
        #expect(opened.items[1].video == source.attachments[2].key, "a video plays its original")
        #expect(ViewerModel.attachments(of: source, from: AttachmentId(rawValue: 4)) { $0.cover == nil } == nil)
    }

    // A multi-image embed opens its images; a single image or a GIFV opens alone.
    @Test
    func anEmbedOpensItsImages() throws {
        let gallery = embed(kind: .rich, media: [image(0), image(1), image(2)])
        let opened = try #require(ViewerModel.embed(gallery, from: image(1).key))
        #expect(opened.items.map(\.key) == [image(0).key, image(1).key, image(2).key])
        #expect(opened.index == 1)

        let gifv = embed(
            kind: .gifv,
            video: EmbedVideo(
                poster: image(0, slot: .thumbnail), width: 498, height: 280, proxied: true,
                key: .embedMedia(channelId: id(10), messageId: id(1), embed: 0, slot: .video)))
        let alone = try #require(ViewerModel.embed(gifv, from: image(0, slot: .thumbnail).key))
        #expect(alone.items.count == 1)
        #expect(alone.items[0].kind == .gifv)
        #expect(alone.items[0].video == .embedMedia(channelId: id(10), messageId: id(1), embed: 0, slot: .video))

        // A video the media proxy doesn't serve plays nowhere: its poster opens as an image.
        let unproxied = embed(
            kind: .gifv,
            video: EmbedVideo(
                poster: image(0, slot: .thumbnail), width: 498, height: 280, proxied: false,
                key: .embedMedia(channelId: id(10), messageId: id(1), embed: 0, slot: .video)))
        let poster = try #require(ViewerModel.embed(unproxied, from: image(0, slot: .thumbnail).key))
        #expect(poster.items.map(\.kind) == [.image])
        #expect(poster.items[0].video == nil)
    }

    // ← ↑ previous and → ↓ Space next, wrapping. Module 947056 preloads the previous item with
    // two or more, the next with three or more, images only.
    @Test
    func nextAndPreviousWrapAndNeighboursPreload() throws {
        var source = message(1)
        source.attachments = [attachment(1, "a.png"), attachment(2, "b.png"), attachment(3, "c.png")]
        let opened = try #require(ViewerModel.attachments(of: source, from: AttachmentId(rawValue: 1)) { _ in true })
        let viewer = try model(opened.items, at: opened.index)

        #expect(viewer.preloads.map(\.key) == [source.attachments[2].key, source.attachments[1].key])
        let two = try model(Array(opened.items.prefix(2)))
        #expect(two.preloads.map(\.key) == [source.attachments[1].key], "the previous, wrapping")
        let video = try model([opened.items[0], ViewerModel.Item(attachment(4, "clip.mp4", kind: .video))])
        #expect(video.preloads.isEmpty, "videos aren't preloaded")
        viewer.previous()
        #expect(viewer.index == 2, "wraps to the last")
        viewer.next()
        #expect(viewer.index == 0, "and back")
        viewer.select(1)
        #expect(viewer.current.key == source.attachments[1].key)

        let one = try model([opened.items[0]])
        one.next()
        #expect(one.index == 0)
        #expect(one.preloads.isEmpty)
    }

    // View Details (attachments only): "Filename" (the title and the extension when Discord gave
    // one), "Size" with whole megabytes rounded up (module 255438), "Description (Alt Text)".
    @Test
    func detailsNameTheFile() throws {
        let item = ViewerModel.Item(attachment(1, "IMG_1.png", description: "a cat", title: "cat"))
        #expect(ViewerModel.details(of: item).map(\.title) == ["Filename", "Size", "Description (Alt Text)"])
        #expect(ViewerModel.details(of: item).map(\.value) == ["cat.png", "1920x1080 (2 MB)", "a cat"])
        let small = ViewerModel.Item(attachment(2, "a.png", size: 300_000))
        #expect(ViewerModel.details(of: small).map(\.value) == ["a.png", "1920x1080 (1 MB)"])
        let big = ViewerModel.Item(attachment(3, "b.mov", kind: .video, size: 2_340_000_000))
        #expect(ViewerModel.details(of: big).last?.value == "1920x1080 (2.3 GB)")
        #expect(ViewerModel.details(of: ViewerModel.Item(image(0))).isEmpty, "attachments only")
    }

    // Module 947056: Zoom for images and GIFVs; Save for videos and images but GIFVs; Copy Image for
    // still images; Copy Media Link and Open in Browser for all; View Details for attachments. With
    // the channel's download options hidden only Zoom stays, and videos have nothing.
    @Test
    func theActionsFollowTheItem() throws {
        let still = ViewerModel.Item(attachment(1, "a.png"))
        let gif = ViewerModel.Item(attachment(2, "a.gif", animated: true))
        let video = ViewerModel.Item(attachment(3, "clip.mp4", kind: .video))
        let gifv = ViewerModel.Item(image(0, slot: .thumbnail), video: .embedMedia(channelId: id(10), messageId: id(1), embed: 0, slot: .video))
        let embedded = ViewerModel.Item(image(1))
        let actions = { (item: ViewerModel.Item, hidden: Bool) in ViewerModel.actions(for: item, hidesDownloads: hidden) }

        #expect(actions(still, false) == [.zoom, .save, .openInBrowser, .copyImage, .copyLink, .details])
        #expect(actions(gif, false) == [.zoom, .save, .openInBrowser, .copyLink, .details])
        #expect(actions(video, false) == [.save, .openInBrowser, .copyLink, .details])
        #expect(actions(gifv, false) == [.zoom, .openInBrowser, .copyLink])
        #expect(actions(embedded, false) == [.zoom, .save, .openInBrowser, .copyImage, .copyLink])
        #expect(actions(still, true) == [.zoom])
        #expect(actions(video, true).isEmpty)
        #expect(actions(embedded, true) == [.zoom, .save, .openInBrowser, .copyImage, .copyLink], "embeds keep theirs")
    }

    // Open in Browser and Copy Media Link use the original; a video's is what plays.
    @Test
    func theOriginalIsTheItemsOwn() async throws {
        let video = attachment(3, "clip.mp4", kind: .video)
        account.originalLinks.withLock { $0[video.key] = "https://cdn.discordapp.com/attachments/1/3/clip.mp4" }
        let viewer = try model([ViewerModel.Item(video)])

        #expect(await viewer.originalURL(of: viewer.current)?.absoluteString == "https://cdn.discordapp.com/attachments/1/3/clip.mp4")
        #expect(viewer.actions.contains(.openInBrowser))
        #expect(try model([ViewerModel.Item(video)], hidesDownloads: true).actions.isEmpty)
    }

    // The row's author and time, its channel's download options; a thread's starter is its
    // parent's message.
    @Test
    func theListOpensARowsMedia() throws {
        let store = FakeStore()
        var shown = message(1, by: user(2))
        shown.attachments = [attachment(1, "a.png"), attachment(2, "b.png")]
        let list = MessageListModel(channelId: id(10), userId: id(1), account: account, store: store)
        let row = MessageListModel.Row(id: shown.id, message: shown, authorName: "Ray")
        let opened = try #require(ViewerModel.attachments(of: shown, from: AttachmentId(rawValue: 2)) { _ in true })

        let viewer = try #require(list.viewer(opened.items, at: opened.index, of: row))
        #expect(viewer.index == 1)
        #expect(viewer.author == user(2))
        #expect(viewer.authorName == "Ray")
        #expect(viewer.timestamp == shown.timestamp)
        #expect(!viewer.hidesDownloads)
        #expect(store.reads.contains(.hidesMediaDownloads(id(10))))

        store.update { $0.hidingDownloads.insert(id(10)) }
        #expect(try #require(list.viewer(opened.items, at: 0, of: row)).hidesDownloads)
    }

    @Test
    func anEmptyListOpensNothing() {
        #expect(ViewerModel(items: [], index: 0, author: user(2), authorName: "Ray", timestamp: Date(), hidesDownloads: false, account: account) == nil)
    }
}
