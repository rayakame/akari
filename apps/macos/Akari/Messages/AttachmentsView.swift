import AkariKit
import AppKit

/// A message's attachments under its text (D141): its images and videos in Discord's mosaic,
/// scaled down as a whole to fit the column, then its files. Every frame comes from metadata,
/// so the row's height is final before anything loads.
final class AttachmentsView: NSView {
    struct Layout: Equatable {
        var mosaic: NSRect?
        var cells: [NSRect] = []
        var files: [NSRect] = []
        var height: CGFloat = 0
    }

    /// From the mosaic to the first file row, and from the text when there's no mosaic (2 of
    /// Discord's 12 and 10 are the stack's).
    static let filesAfterMosaic: CGFloat = 12
    static let filesAfterText: CGFloat = 8
    static let fileGap: CGFloat = 8
    static let mosaicRadius: CGFloat = 8
    static let cellRadius: CGFloat = 2

    /// Where each attachment goes in a column `width` points wide.
    static func layout(_ attachments: [Attachment], width: CGFloat) -> Layout {
        var layout = Layout()
        let media = attachments.filter { $0.display == .media }
        let files = attachments.filter { $0.display == .file }
        var y: CGFloat = 0
        if !media.isEmpty, width > 0 {
            let fitted = mediaFitWidth(mosaic: mosaic(media), width: UInt32(max(1, width.rounded(.down))))
            layout.mosaic = NSRect(
                x: 0, y: 0, width: CGFloat(fitted.size.width), height: CGFloat(fitted.size.height))
            layout.cells = fitted.cells.map {
                NSRect(x: CGFloat($0.x), y: CGFloat($0.y), width: CGFloat($0.width), height: CGFloat($0.height))
            }
            y = CGFloat(fitted.size.height)
        }
        if !files.isEmpty {
            y += media.isEmpty ? filesAfterText : filesAfterMosaic
            for index in files.indices {
                layout.files.append(
                    NSRect(x: 0, y: y, width: min(FileRowView.width, width), height: FileRowView.height))
                y += FileRowView.height + (index == files.count - 1 ? 0 : fileGap)
            }
        }
        layout.height = y
        return layout
    }

    // One item alone keeps its own box (a video at least 150 × 110); more share the mosaic.
    private static func mosaic(_ media: [Attachment]) -> MediaMosaic {
        let sizes = media.map { MediaSize(width: $0.width, height: $0.height) }
        guard media.count == 1, let only = media.first, let size = sizes.first else {
            return mediaMosaic(items: sizes)
        }
        let box = mediaBox(source: size, video: only.kind == .video)
        return MediaMosaic(size: box, cells: [MediaRect(x: 0, y: 0, width: box.width, height: box.height)])
    }

    private let mosaic = MosaicView()
    private(set) var tiles: [MediaTileView] = []
    private(set) var files: [FileRowView] = []
    private var attachments: [Attachment] = []
    private var layoutCache: (width: CGFloat, layout: Layout)?
    var images: ImageCache? {
        didSet {
            if images !== oldValue {
                tiles.forEach { $0.images = images }
            }
        }
    }
    var placeholders: PlaceholderCache? {
        didSet { tiles.forEach { $0.placeholders = placeholders } }
    }
    /// A tile's click or button, with the tile.
    var onMedia: (Attachment, MediaTileView.Action, MediaTileView) -> Void = { _, _, _ in }
    var onOpenFile: (Attachment) -> Void = { _ in }

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        mosaic.wantsLayer = true
        mosaic.layer?.cornerRadius = Self.mosaicRadius
        mosaic.layer?.masksToBounds = true
        addSubview(mosaic)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    /// The column's width, which the cell sets before its layout: the height follows from it.
    var preferredWidth: CGFloat = 0 {
        didSet {
            if preferredWidth != oldValue {
                invalidateIntrinsicContentSize()
            }
        }
    }

    override var intrinsicContentSize: NSSize {
        NSSize(width: NSView.noIntrinsicMetric, height: layout(for: preferredWidth).height)
    }

    /// The height at `width`, as `layout()` places everything.
    func height(width: CGFloat) -> CGFloat {
        layout(for: width).height
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        let local = convert(point, from: superview)
        for tile in tiles where !tile.isHidden {
            if let hit = tile.hitTest(convert(local, to: mosaic)) {
                return hit
            }
        }
        for file in files where !file.isHidden && file.frame.contains(local) {
            return file.hitTest(local) ?? file
        }
        return nil
    }

    /// `covers` gives each media item's cover, the session's reveals applied.
    func configure(
        _ attachments: [Attachment], covers: (Attachment) -> MediaTileView.Cover?, showsAltText: Bool
    ) {
        self.attachments = attachments
        layoutCache = nil
        let media = attachments.filter { $0.display == .media }
        let shownFiles = attachments.filter { $0.display == .file }
        while tiles.count < media.count {
            let tile = MediaTileView()
            tile.images = images
            tile.placeholders = placeholders
            tile.onAction = { [weak self, weak tile] action in
                guard let self, let tile, let attachment = tile.content?.attachment else {
                    return
                }
                self.onMedia(attachment, action, tile)
            }
            tiles.append(tile)
            mosaic.addSubview(tile)
        }
        for (index, tile) in tiles.enumerated() {
            guard index < media.count else {
                tile.configure(nil)
                tile.isHidden = true
                continue
            }
            let attachment = media[index]
            tile.isHidden = false
            tile.cornerRadius = media.count == 1 ? Self.mosaicRadius : Self.cellRadius
            tile.configure(
                MediaTileView.Content(
                    attachment: attachment, cover: covers(attachment), showsAltText: showsAltText,
                    single: media.count == 1))
        }
        mosaic.isHidden = media.isEmpty
        while files.count < shownFiles.count {
            let row = FileRowView()
            row.onOpen = { [weak self, weak row] in
                if let self, let attachment = row?.attachment {
                    self.onOpenFile(attachment)
                }
            }
            files.append(row)
            addSubview(row)
        }
        for (index, row) in files.enumerated() {
            row.isHidden = index >= shownFiles.count
            row.configure(index < shownFiles.count ? shownFiles[index] : nil)
        }
        invalidateIntrinsicContentSize()
        needsLayout = true
    }

    private func layout(for width: CGFloat) -> Layout {
        if let layoutCache, layoutCache.width == width {
            return layoutCache.layout
        }
        let layout = Self.layout(attachments, width: width)
        layoutCache = (width, layout)
        return layout
    }

    override func layout() {
        super.layout()
        let layout = layout(for: bounds.width)
        mosaic.frame = layout.mosaic ?? .zero
        for (tile, cell) in zip(tiles, layout.cells) {
            tile.frame = cell
        }
        for (row, frame) in zip(files, layout.files) {
            row.frame = frame
        }
    }
}

// Cells are placed from the top, as the layout gives them.
private final class MosaicView: NSView {
    override var isFlipped: Bool { true }
}
