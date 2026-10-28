import AkariKit
import AppKit

/// A message's embeds under its attachments (D145), 4 apart: each a card, or its media alone.
/// Every frame comes from the embeds' numbers and their measured text, so the row's height is
/// final before anything loads.
final class EmbedsView: NSView {
    static let gap: CGFloat = 4

    private(set) var views: [EmbedView] = []
    private var shown: [EmbedView.Shown] = []
    private var heightCache: (width: CGFloat, height: CGFloat)?
    var images: ImageCache? {
        didSet { views.forEach { $0.images = images } }
    }
    var placeholders: PlaceholderCache? {
        didSet { views.forEach { $0.placeholders = placeholders } }
    }
    var onAction: (Embed, EmbedView.Action) -> Void = { _, _ in }
    var onLink: (URL) -> Void = { _ in }
    var onExpression: (ExpressionTarget, NSRect) -> Void = { _, _ in }
    var messageMenu: () -> [NSMenuItem] = { [] }
    var loopSource: (ImageKey) async -> URL? = { _ in nil }
    /// The list's say for the message's custom emoji, as for its text.
    var animatesEmoji = false {
        didSet { views.forEach { $0.animatesEmoji = animatesEmoji } }
    }
    var detectsExpressions = true {
        didSet { views.forEach { $0.detectsExpressions = detectsExpressions } }
    }

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
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
        NSSize(width: NSView.noIntrinsicMetric, height: height(width: preferredWidth))
    }

    /// Every media tile, thumbnails among them, for the list's motion rule.
    var tiles: [MediaTileView] {
        views.flatMap(\.allTiles)
    }

    func height(width: CGFloat) -> CGFloat {
        if let heightCache, heightCache.width == width {
            return heightCache.height
        }
        let heights = shown.map { EmbedView.layout($0, width: width).size.height }
        let height = heights.reduce(0, +) + Self.gap * CGFloat(max(heights.count - 1, 0))
        heightCache = (width, height)
        return height
    }

    /// `keepingSelection` for the same message shown again; `measuring` (the sizing cell) only
    /// keeps what heights need.
    func configure(_ shown: [EmbedView.Shown], keepingSelection: Bool = false, measuring: Bool = false) {
        self.shown = shown
        heightCache = nil
        guard !measuring else {
            return
        }
        while views.count < shown.count {
            let view = EmbedView()
            view.images = images
            view.placeholders = placeholders
            view.onAction = { [weak self] embed, action in self?.onAction(embed, action) }
            view.onLink = { [weak self] url in self?.onLink(url) }
            view.onExpression = { [weak self] target, rect in self?.onExpression(target, rect) }
            view.messageMenu = { [weak self] in self?.messageMenu() ?? [] }
            view.loopSource = { [weak self] key in await self?.loopSource(key) }
            view.animatesEmoji = animatesEmoji
            view.detectsExpressions = detectsExpressions
            views.append(view)
            addSubview(view)
        }
        for (index, view) in views.enumerated() {
            view.isHidden = index >= shown.count
            view.configure(index < shown.count ? shown[index] : nil, keepingSelection: keepingSelection)
        }
        invalidateIntrinsicContentSize()
        needsLayout = true
    }

    override func layout() {
        super.layout()
        var y: CGFloat = 0
        for (view, shown) in zip(views, shown) {
            let size = EmbedView.layout(shown, width: bounds.width).size
            view.frame = NSRect(x: 0, y: y, width: size.width, height: size.height)
            y += size.height + Self.gap
        }
    }
}

/// One embed: a card (Discord's grid: provider, author, title, description, fields, media,
/// thumbnail, footer), or its image, gallery, GIFV or video alone.
final class EmbedView: NSView {
    enum Action {
        /// The original of an image, in the browser.
        case open(ImageKey)
        /// The embed's or its video's page.
        case openLink(URL)
        case reveal
        case toggleSensitive
    }

    /// An embed with its texts built for the list, and its cover with the session's reveals.
    struct Shown {
        let embed: Embed
        var cover: MediaTileView.Cover?
        var provider: NSAttributedString?
        var author: NSAttributedString?
        var title: NSAttributedString?
        var description: NSAttributedString?
        var fields: [(name: NSAttributedString, value: NSAttributedString, inline: Bool)] = []
        var footer: NSAttributedString?
        /// When a relative time in its text changes next.
        var nextChange: Date?
    }

    struct Layout: Equatable {
        struct Field: Equatable {
            var name: NSRect
            var value: NSRect
        }

        var size: NSSize = .zero
        var provider: NSRect?
        var authorIcon: NSRect?
        var author: NSRect?
        var title: NSRect?
        var description: NSRect?
        var fields: [Field] = []
        /// The image, gallery or video.
        var media: NSRect?
        /// A gallery's cells, in the media's coordinates; one cell for anything else.
        var cells: [NSRect] = []
        var thumbnail: NSRect?
        var footerIcon: NSRect?
        var footer: NSRect?
    }

    // Discord's embed CSS (198496): the grid's padding, the rows' margins, the bar and border.
    static let bar: CGFloat = 4
    static let border: CGFloat = 1
    static let padding = NSEdgeInsets(top: 2, left: 12, bottom: 16, right: 16)
    static let rowGap: CGFloat = 8
    static let mediaGap: CGFloat = 16
    static let thumbnailGap: CGFloat = 16
    static let maxInner: CGFloat = 516
    static let radius: CGFloat = 4
    static let inlineSpoilerRadius: CGFloat = 8
    static let authorIcon: CGFloat = 24
    static let footerIcon: CGFloat = 20
    static let iconGap: CGFloat = 8
    static let fieldGap: CGFloat = 8
    static let fieldNameGap: CGFloat = 2
    static let galleryGap: CGFloat = 4
    static let pillHeight: CGFloat = 48

    /// Where everything of `shown` goes in a column `width` wide.
    static func layout(_ shown: Shown, width: CGFloat) -> Layout {
        let embed = shown.embed
        if embed.inline {
            var layout = Layout()
            let box = mediaBox(embed, limit: width)
            layout.media = NSRect(origin: .zero, size: box.size)
            layout.cells = box.cells
            layout.size = box.size
            return layout
        }
        return card(shown, width: width)
    }

    // The media's box and cells, scaled down as a whole to `limit`.
    private static func mediaBox(_ embed: Embed, limit: CGFloat) -> (size: NSSize, cells: [NSRect]) {
        let height = embed.tall ? 450.0 : 300.0
        if embed.media.count > 1, embed.video == nil {
            let first = embed.media[0]
            let fitted = embedMediaSize(
                source: MediaSize(width: first.width, height: first.height), tall: embed.tall)
            // A gallery is as wide as the card allows (Discord: the card's 520 or the image's width).
            let width = min(limit, fitted.width >= 300 ? CGFloat(fitted.width) : 400)
            return (NSSize(width: width, height: height), gallery(embed.media.count, width: width, height: height))
        }
        let size: MediaSize
        if let video = embed.video, embed.kind != .gifv {
            size = embedVideoSize(source: MediaSize(width: video.width, height: video.height), tall: embed.tall)
        } else if let poster = embed.video?.poster ?? embed.media.first {
            size = embedMediaSize(source: MediaSize(width: poster.width, height: poster.height), tall: embed.tall)
        } else {
            return (.zero, [])
        }
        var box = NSSize(width: CGFloat(size.width), height: CGFloat(size.height))
        if box.width > limit, limit > 0 {
            box = NSSize(width: limit, height: (box.height * limit / box.width).rounded())
        }
        return (box, [NSRect(origin: .zero, size: box)])
    }

    // Two columns 4 apart: two side by side, three as one left and two right, four as 2 × 2.
    static func gallery(_ count: Int, width: CGFloat, height: CGFloat) -> [NSRect] {
        let column = ((width - galleryGap) / 2).rounded(.down)
        let right = width - column - galleryGap
        let half = ((height - galleryGap) / 2).rounded(.down)
        let lower = height - half - galleryGap
        switch count {
        case 2:
            return [
                NSRect(x: 0, y: 0, width: column, height: height),
                NSRect(x: column + galleryGap, y: 0, width: right, height: height),
            ]
        case 3:
            return [
                NSRect(x: 0, y: 0, width: column, height: height),
                NSRect(x: column + galleryGap, y: 0, width: right, height: half),
                NSRect(x: column + galleryGap, y: half + galleryGap, width: right, height: lower),
            ]
        default:
            return [
                NSRect(x: 0, y: 0, width: column, height: half),
                NSRect(x: column + galleryGap, y: 0, width: right, height: half),
                NSRect(x: 0, y: half + galleryGap, width: column, height: lower),
                NSRect(x: column + galleryGap, y: half + galleryGap, width: right, height: lower),
            ]
        }
    }

    private static func card(_ shown: Shown, width: CGFloat) -> Layout {
        let embed = shown.embed
        let measure = TextMeasurer.shared
        let chrome = bar + border + padding.left + padding.right
        let thumbnail = embed.thumbnail.map {
            let size = embedThumbnailSize(source: MediaSize(width: $0.width, height: $0.height))
            return NSSize(width: CGFloat(size.width), height: CGFloat(size.height))
        }
        let beside = thumbnail.map { $0.width + thumbnailGap } ?? 0
        let media = embed.media.isEmpty && embed.video == nil ? nil : mediaBox(embed, limit: .greatestFiniteMagnitude)
        // As wide as its content (Discord's max-content), at most 516 inside; a wide image or a
        // video makes it the media's width.
        var inner: CGFloat
        if let media, media.size.width >= 300 || embed.video != nil {
            inner = media.size.width
        } else {
            let texts = [shown.provider, shown.title, shown.description, shown.footer].compactMap { $0 }
            var widest = texts.map { measure.width($0) }.max() ?? 0
            if let author = shown.author {
                let icon = embed.author?.icon == nil ? 0 : authorIcon + iconGap
                widest = max(widest, icon + measure.width(author))
            }
            if shown.footer != nil, embed.footer?.icon != nil {
                widest = max(widest, footerIcon + iconGap + measure.width(shown.footer ?? NSAttributedString()))
            }
            // A row's columns are equal: its widest field decides.
            for row in fieldRows(shown.fields) {
                let widths = row.map { max(measure.width($0.name), measure.width($0.value)) }
                let count = CGFloat(row.count)
                widest = max(widest, (widths.max() ?? 0) * count + fieldGap * (count - 1))
            }
            inner = widest + beside
            if let media {
                inner = max(inner, media.size.width)
            }
        }
        inner = max(1, min(inner, maxInner - padding.left - padding.right, width - chrome).rounded(.up))
        let textWidth = max(1, inner - beside)
        var layout = Layout()
        let left = bar + padding.left
        var y = border + padding.top
        func row(_ height: CGFloat) -> CGFloat {
            y += rowGap
            defer { y += height }
            return y
        }
        if let provider = shown.provider {
            let height = measure.height(provider, width: textWidth)
            layout.provider = NSRect(x: left, y: row(height), width: textWidth, height: height)
        }
        // Icon rows are flex rows: as tall as the icon, the text centered beside it.
        if let author = shown.author {
            let icon = embed.author?.icon == nil ? 0 : authorIcon + iconGap
            let text = measure.height(author, width: textWidth - icon)
            let height = max(text, icon > 0 ? authorIcon : 0)
            let top = row(height)
            if icon > 0 {
                layout.authorIcon = NSRect(
                    x: left, y: top + ((height - authorIcon) / 2).rounded(), width: authorIcon, height: authorIcon)
            }
            layout.author = NSRect(
                x: left + icon, y: top + ((height - text) / 2).rounded(), width: textWidth - icon, height: text)
        }
        if let title = shown.title {
            let height = measure.height(title, width: textWidth)
            layout.title = NSRect(x: left, y: row(height), width: textWidth, height: height)
        }
        if let description = shown.description {
            let height = measure.height(description, width: textWidth)
            layout.description = NSRect(x: left, y: row(height), width: textWidth, height: height)
        }
        let rows = fieldRows(shown.fields)
        if !rows.isEmpty {
            y += rowGap
            for (index, fields) in rows.enumerated() {
                if index > 0 {
                    y += fieldGap
                }
                let count = CGFloat(fields.count)
                let column = ((textWidth - fieldGap * (count - 1)) / count).rounded(.down)
                var tallest: CGFloat = 0
                for (place, field) in fields.enumerated() {
                    let x = left + CGFloat(place) * (column + fieldGap)
                    let name = measure.height(field.name, width: column)
                    let value = measure.height(field.value, width: column)
                    layout.fields.append(
                        Layout.Field(
                            name: NSRect(x: x, y: y, width: column, height: name),
                            value: NSRect(x: x, y: y + name + fieldNameGap, width: column, height: value)))
                    tallest = max(tallest, name + fieldNameGap + value)
                }
                y += tallest
            }
        }
        if let thumbnail {
            let top = border + padding.top + rowGap
            layout.thumbnail = NSRect(
                x: left + textWidth + thumbnailGap, y: top, width: thumbnail.width, height: thumbnail.height)
            y = max(y, top + thumbnail.height)
        }
        if let media {
            y += mediaGap
            var size = media.size
            if size.width > inner {
                size = NSSize(width: inner, height: (size.height * inner / size.width).rounded())
            }
            let cells =
                embed.media.count > 1 && embed.video == nil
                ? gallery(embed.media.count, width: size.width, height: size.height) : [NSRect(origin: .zero, size: size)]
            layout.media = NSRect(x: left, y: y, width: size.width, height: size.height)
            layout.cells = cells
            y += size.height
        }
        if let footer = shown.footer {
            let icon = embed.footer?.icon == nil ? 0 : footerIcon + iconGap
            let text = measure.height(footer, width: inner - icon)
            let height = max(text, icon > 0 ? footerIcon : 0)
            let top = row(height)
            if icon > 0 {
                layout.footerIcon = NSRect(
                    x: left, y: top + ((height - footerIcon) / 2).rounded(), width: footerIcon, height: footerIcon)
            }
            layout.footer = NSRect(
                x: left + icon, y: top + ((height - text) / 2).rounded(), width: inner - icon, height: text)
        }
        y += padding.bottom + border
        layout.size = NSSize(width: inner + chrome, height: y)
        return layout
    }

    // Inline fields fill a row up to three; one that isn't has a row of its own.
    static func fieldRows<Field>(_ fields: [Field]) -> [[Field]] where Field: InlineField {
        var rows: [[Field]] = []
        var open: [Field] = []
        for field in fields {
            if !field.isInline, !open.isEmpty {
                rows.append(open)
                open = []
            }
            open.append(field)
            if !field.isInline || open.count == 3 {
                rows.append(open)
                open = []
            }
        }
        if !open.isEmpty {
            rows.append(open)
        }
        return rows
    }

    private static func fieldRows(
        _ fields: [(name: NSAttributedString, value: NSAttributedString, inline: Bool)]
    ) -> [[ShownField]] {
        fieldRows(fields.map { ShownField(name: $0.name, value: $0.value, isInline: $0.inline) })
    }

    private(set) var shown: Shown?
    private let card = CALayer()
    private let barLayer = CALayer()
    private let provider = MessageTextView()
    private let author = MessageTextView()
    private let authorIcon = ImageLayerView()
    private let title = MessageTextView()
    private let descriptionView = MessageTextView()
    private var fieldViews: [(name: MessageTextView, value: MessageTextView)] = []
    private let mediaView = NSView()
    private(set) var tiles: [MediaTileView] = []
    private let thumbnail = MediaTileView()
    private let footer = MessageTextView()
    private let footerIcon = ImageLayerView()
    private let pill = VideoPill()
    private let cover = MediaTileView()
    let eye = OverlayButton()
    var images: ImageCache? {
        didSet {
            guard images !== oldValue else {
                return
            }
            for view in [provider, author, title, descriptionView, footer] + fieldViews.flatMap({ [$0.name, $0.value] }) {
                view.images = images
            }
            (tiles + [thumbnail, cover]).forEach { $0.images = images }
            authorIcon.images = images
            footerIcon.images = images
        }
    }
    var placeholders: PlaceholderCache? {
        didSet { (tiles + [thumbnail]).forEach { $0.placeholders = placeholders } }
    }
    var onAction: (Embed, Action) -> Void = { _, _ in }
    var onLink: (URL) -> Void = { _ in }
    var onExpression: (ExpressionTarget, NSRect) -> Void = { _, _ in }
    var messageMenu: () -> [NSMenuItem] = { [] }
    var loopSource: (ImageKey) async -> URL? = { _ in nil }
    var animatesEmoji = false {
        didSet { textViews.forEach { $0.animatesEmoji = animatesEmoji } }
    }
    var detectsExpressions = true {
        didSet { textViews.forEach { $0.detectsExpressions = detectsExpressions } }
    }
    private var layoutCache: (width: CGFloat, layout: Layout)?

    /// Its media and thumbnail tiles.
    var allTiles: [MediaTileView] {
        tiles + [thumbnail]
    }

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        card.cornerRadius = Self.radius
        card.borderWidth = Self.border
        card.masksToBounds = true
        card.addSublayer(barLayer)
        layer?.addSublayer(card)
        for view in [provider, author, title, descriptionView, footer] {
            configureText(view)
        }
        for icon in [authorIcon, footerIcon] {
            icon.fit = .contain
            icon.wantsLayer = true
            addSubview(icon)
        }
        authorIcon.cornerRadius = Self.authorIcon / 2
        footerIcon.cornerRadius = Self.footerIcon / 2
        mediaView.wantsLayer = true
        mediaView.layer?.cornerRadius = Self.radius
        mediaView.layer?.masksToBounds = true
        addSubview(mediaView)
        thumbnail.cornerRadius = Self.radius
        thumbnail.onAction = { [weak self] action in
            if action == .open, let key = self?.shown?.embed.thumbnail?.key {
                self?.act(.open(key))
            }
        }
        addSubview(thumbnail)
        pill.onPlay = { [weak self] in self?.openVideo() }
        pill.onOpen = { [weak self] in self?.openVideo() }
        addSubview(pill)
        cover.spoilerRadius = Self.radius
        cover.cornerRadius = Self.radius
        cover.onAction = { [weak self] action in
            switch action {
            case .reveal: self?.act(.reveal)
            case .toggleSensitive: self?.act(.toggleSensitive)
            default: break
            }
        }
        addSubview(cover)
        eye.target = self
        eye.action = #selector(eyeClicked)
        eye.cornerRadius = 8
        eye.symbol = "eye"
        eye.frameSize = NSSize(width: 32, height: 32)
        eye.setAccessibilityLabel("Hide sensitive content")
        addSubview(eye)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    private func configureText(_ view: MessageTextView) {
        view.onLink = { [weak self] url in self?.onLink(url) }
        view.onExpression = { [weak self] target, rect in self?.onExpression(target, rect) }
        view.messageMenu = { [weak self] in self?.messageMenu() ?? [] }
        view.animatesEmoji = animatesEmoji
        view.detectsExpressions = detectsExpressions
        addSubview(view)
    }

    /// The text views, for the list's cursor and hover.
    var textViews: [MessageTextView] {
        [provider, author, title, descriptionView, footer] + fieldViews.flatMap { [$0.name, $0.value] }
    }

    func configure(_ shown: Shown?, keepingSelection: Bool = false) {
        self.shown = shown
        layoutCache = nil
        keepsSelection = keepingSelection
        guard let shown else {
            tiles.forEach { $0.configure(nil) }
            thumbnail.configure(nil)
            cover.configure(nil)
            authorIcon.show(nil)
            footerIcon.show(nil)
            return
        }
        let embed = shown.embed
        let isCard = !embed.inline
        let cardCover = isCard ? shown.cover : nil
        let hidden = cardCover.map(MediaTileView.hides) ?? false
        show(provider, shown.provider, isCard && !hidden)
        show(author, shown.author, isCard && !hidden)
        show(title, shown.title, isCard && !hidden)
        show(descriptionView, shown.description, isCard && !hidden)
        show(footer, shown.footer, isCard && !hidden)
        while fieldViews.count < shown.fields.count {
            let pair = (name: MessageTextView(), value: MessageTextView())
            configureText(pair.name)
            configureText(pair.value)
            pair.name.images = images
            pair.value.images = images
            pair.name.messageMenu = { [weak self] in self?.messageMenu() ?? [] }
            fieldViews.append(pair)
        }
        for (index, pair) in fieldViews.enumerated() {
            let field = index < shown.fields.count ? shown.fields[index] : nil
            show(pair.name, field?.name, isCard && !hidden)
            show(pair.value, field?.value, isCard && !hidden)
        }
        authorIcon.show(isCard && !hidden ? embed.author?.icon.map { ImageLayerView.Content(key: $0) } : nil)
        authorIcon.isHidden = authorIcon.content == nil
        footerIcon.show(isCard && !hidden ? embed.footer?.icon.map { ImageLayerView.Content(key: $0) } : nil)
        footerIcon.isHidden = footerIcon.content == nil
        configureMedia(shown, hidden: hidden)
        if let thumb = embed.thumbnail, isCard, !hidden {
            thumbnail.isHidden = false
            thumbnail.configure(MediaTileView.Content(media: MediaTileView.Media(thumb)))
        } else {
            thumbnail.isHidden = true
            thumbnail.configure(nil)
        }
        if let cardCover, MediaTileView.hides(cardCover) {
            cover.isHidden = false
            cover.configure(
                MediaTileView.Content(
                    media: MediaTileView.Media(key: embed.media.first?.key ?? coverKey(embed), noun: "Embed"),
                    cover: cardCover, single: true))
        } else {
            cover.isHidden = true
            cover.configure(nil)
        }
        eye.isHidden = !(isCard && cardCover == .sensitive(shown: true))
        card.isHidden = !isCard
        updateColors()
        // A card is a group; media alone is its tile.
        setAccessibilityElement(isCard)
        setAccessibilityRole(.group)
        setAccessibilityLabel(isCard ? "Embed" : nil)
        needsLayout = true
    }

    private var keepsSelection = false

    private func coverKey(_ embed: Embed) -> ImageKey {
        embed.thumbnail?.key ?? embed.video?.key ?? .defaultAvatar(index: 0)
    }

    private func show(_ view: MessageTextView, _ text: NSAttributedString?, _ visible: Bool) {
        guard let text, visible else {
            view.isHidden = true
            return
        }
        view.isHidden = false
        view.show(text, keepingSelection: keepsSelection)
    }

    private func configureMedia(_ shown: Shown, hidden: Bool) {
        let embed = shown.embed
        let inlineCover = embed.inline ? shown.cover : nil
        var items: [MediaTileView.Media] = []
        if let video = embed.video {
            var poster = MediaTileView.Media(video.poster, video: embed.kind != .gifv)
            poster.loops = embed.kind == .gifv
            poster.ownPlay = false
            items = [poster]
        } else {
            items = embed.media.map { MediaTileView.Media($0) }
        }
        if hidden {
            items = []
        }
        while tiles.count < items.count {
            let tile = MediaTileView()
            tile.images = images
            tile.placeholders = placeholders
            tile.onAction = { [weak self, weak tile] action in
                guard let self, let tile else {
                    return
                }
                self.tileAction(action, tile)
            }
            tile.loopSource = { [weak self, weak tile] in
                // Only Discord's copy: an unproxied video would come from a third party's host.
                guard let self, let key = tile?.content?.media.key, let video = self.shown?.embed.video,
                    video.proxied, video.poster.key == key
                else {
                    return nil
                }
                return await self.loopSource(video.key)
            }
            tiles.append(tile)
            mediaView.addSubview(tile)
        }
        for (index, tile) in tiles.enumerated() {
            guard index < items.count else {
                tile.isHidden = true
                tile.configure(nil)
                continue
            }
            tile.isHidden = false
            tile.cornerRadius = items.count > 1 ? 0 : Self.radius
            tile.spoilerRadius = Self.inlineSpoilerRadius
            tile.configure(MediaTileView.Content(media: items[index], cover: inlineCover, single: items.count == 1))
        }
        mediaView.isHidden = items.isEmpty
        // Media alone under a spoiler takes the attachments' 8 pt corners.
        mediaView.layer?.cornerRadius = inlineCover == .spoiler ? Self.inlineSpoilerRadius : Self.radius
        let playable = embed.video != nil && embed.kind != .gifv
        pill.isHidden = !playable || inlineCover.map(MediaTileView.hides) == true || hidden
        pill.showsPlay = embed.video?.proxied == true || embed.url != nil
    }

    private func tileAction(_ action: MediaTileView.Action, _ tile: MediaTileView) {
        guard let embed = shown?.embed else {
            return
        }
        switch action {
        case .open:
            // A GIFV opens in the viewer as Discord's does; other videos play at their link.
            if embed.video != nil, embed.kind != .gifv {
                openVideo()
            } else if let key = tile.content?.media.key {
                act(.open(key))
            }
        case .reveal:
            act(.reveal)
        case .toggleSensitive:
            act(.toggleSensitive)
        case .altText:
            break
        }
    }

    private func openVideo() {
        guard let url = shown?.embed.url.flatMap(URL.init(string:)) else {
            return
        }
        act(.openLink(url))
    }

    private func act(_ action: Action) {
        if let embed = shown?.embed {
            onAction(embed, action)
        }
    }

    @objc private func eyeClicked() {
        act(.toggleSensitive)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors()
    }

    private func updateColors() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            card.backgroundColor = Palette.surfaceHigh.cgColor
            card.borderColor = Palette.borderSubtle.cgColor
            let color = shown?.embed.color.map { NSColor(rgb: $0) } ?? Palette.borderNormal
            barLayer.backgroundColor = color.cgColor
        }
    }

    override func layout() {
        super.layout()
        guard let layout = cachedLayout(width: bounds.width) else {
            return
        }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        card.frame = NSRect(origin: .zero, size: layout.size)
        barLayer.frame = NSRect(x: 0, y: 0, width: Self.bar, height: layout.size.height)
        CATransaction.commit()
        provider.frame = layout.provider ?? .zero
        author.frame = layout.author ?? .zero
        authorIcon.frame = layout.authorIcon ?? .zero
        title.frame = layout.title ?? .zero
        descriptionView.frame = layout.description ?? .zero
        for (pair, frames) in zip(fieldViews, layout.fields) {
            pair.name.frame = frames.name
            pair.value.frame = frames.value
        }
        mediaView.frame = layout.media ?? .zero
        for (tile, cell) in zip(tiles, layout.cells) {
            tile.frame = cell
        }
        thumbnail.frame = layout.thumbnail ?? .zero
        footer.frame = layout.footer ?? .zero
        footerIcon.frame = layout.footerIcon ?? .zero
        let media = layout.media ?? .zero
        let pillSize = pill.fittingSize
        pill.frame = NSRect(
            x: (media.midX - pillSize.width / 2).rounded(), y: (media.midY - Self.pillHeight / 2).rounded(),
            width: pillSize.width, height: Self.pillHeight)
        cover.frame = NSRect(origin: .zero, size: layout.size)
        eye.frame = NSRect(x: layout.size.width - 4 - 32, y: layout.size.height - 4 - 32, width: 32, height: 32)
    }

    private func cachedLayout(width: CGFloat) -> Layout? {
        if let layoutCache, layoutCache.width == width {
            return layoutCache.layout
        }
        guard let shown else {
            return nil
        }
        let layout = Self.layout(shown, width: width)
        layoutCache = (width, layout)
        return layout
    }

    /// The card or the media alone: what a click outside the text hits.
    var box: NSRect {
        cachedLayout(width: bounds.width).map { NSRect(origin: .zero, size: $0.size) } ?? .zero
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        let local = convert(point, from: superview)
        guard box.contains(local) else {
            return nil
        }
        return super.hitTest(point) ?? self
    }
}

protocol InlineField {
    var isInline: Bool { get }
}

private struct ShownField: InlineField {
    let name: NSAttributedString
    let value: NSAttributedString
    let isInline: Bool
}

/// Discord's video pill: a black capsule at 60 %, 48 tall, with Play and Open Link.
final class VideoPill: NSView {
    private let play = NSButton()
    private let open = NSButton()
    var onPlay: () -> Void = {}
    var onOpen: () -> Void = {}
    var showsPlay = true {
        didSet {
            play.isHidden = !showsPlay
            invalidateIntrinsicContentSize()
        }
    }

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = EmbedView.pillHeight / 2
        layer?.backgroundColor = NSColor.black.withAlphaComponent(0.6).cgColor
        for (button, symbol, label) in [
            (play, "play.fill", "Play"), (open, "arrow.up.right.square", "Open Link"),
        ] {
            button.isBordered = false
            button.title = ""
            button.image = NSImage(systemSymbolName: symbol, accessibilityDescription: label)?
                .withSymbolConfiguration(.init(pointSize: 18, weight: .regular))
            button.contentTintColor = .white
            button.alphaValue = 0.6
            button.target = self
            button.action = #selector(clicked(_:))
            button.setAccessibilityLabel(label)
            button.refusesFirstResponder = true
            addSubview(button)
        }
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    // 12 around two 24 pt icons; Play nudged 1 right, Open Link 2 and 4 aside.
    override var fittingSize: NSSize {
        NSSize(width: showsPlay ? 12 + 24 + 2 + 24 + 4 + 12 - 2 : 12 + 24 + 12, height: EmbedView.pillHeight)
    }

    override func layout() {
        super.layout()
        if showsPlay {
            play.frame = NSRect(x: 13, y: 12, width: 24, height: 24)
            open.frame = NSRect(x: 13 + 24 + 2, y: 12, width: 24, height: 24)
        } else {
            open.frame = NSRect(x: 12, y: 12, width: 24, height: 24)
        }
    }

    @objc private func clicked(_ sender: NSButton) {
        sender === play ? onPlay() : onOpen()
    }

    var buttons: [NSButton] { [play, open] }
}

extension EmbedView {
    /// `embed` with its texts built as the list shows them.
    static func shown(
        _ embed: Embed, cover: MediaTileView.Cover?, spoilersShown: Bool, revealed: Set<UInt16>, now: Date,
        times: TimestampText, footerTime: String?
    ) -> Shown {
        var nextChange: Date?
        func rich(_ text: RichText, _ base: MessageText.Source.Base, _ color: NSColor) -> NSAttributedString {
            let built = MessageText.build(
                MessageText.Source(text, base: base, color: color, spoilersShown: spoilersShown),
                revealed: revealed, now: now, times: times)
            if let next = built.nextChange {
                nextChange = min(nextChange ?? next, next)
            }
            return built.text
        }
        var shown = Shown(embed: embed, cover: cover)
        if let provider = embed.provider {
            shown.provider = line(
                provider.name, TypeScale.embedProvider,
                provider.url == nil ? Palette.textDefault : Palette.interactiveText,
                link: provider.url)
        }
        if let author = embed.author {
            shown.author = line(author.name, TypeScale.embedAuthor, Palette.textStrong, link: author.url)
        }
        if let title = embed.title {
            let link = embed.url.flatMap(URL.init(string:))
            let text = NSMutableAttributedString(
                attributedString: rich(title, .embedTitle, link == nil ? Palette.textStrong : Palette.textLink))
            // The title is a link; its spoilers and links of its own keep their clicks.
            if let link {
                text.enumerateAttribute(.link, in: NSRange(location: 0, length: text.length)) { value, range, _ in
                    if value == nil {
                        text.addAttribute(.link, value: link, range: range)
                    }
                }
            }
            shown.title = text
        }
        shown.description = embed.description.map { rich($0, .embedText, Palette.textDefault) }
        shown.fields = embed.fields.map {
            (rich($0.name, .embedFieldName, Palette.textStrong), rich($0.value, .embedText, Palette.textDefault), $0.inline)
        }
        if let footer = embed.footer {
            let parts = [footer.text, footerTime].compactMap { $0 }
            shown.footer = line(parts.joined(separator: " • "), TypeScale.embedFooter, Palette.textDefault, link: nil)
        }
        shown.nextChange = nextChange
        return shown
    }

    private static func line(_ text: String, _ style: TextStyle, _ color: NSColor, link: String?)
        -> NSAttributedString
    {
        let paragraph = NSMutableParagraphStyle()
        paragraph.minimumLineHeight = style.line
        paragraph.maximumLineHeight = style.line
        var attributes: [NSAttributedString.Key: Any] = [
            .font: style.nsFont, .foregroundColor: color, .paragraphStyle: paragraph,
            // As the message text's faces: 3 on a 16/22 line, 2 on 13/18.
            .baselineOffset: ((style.line - style.size) / 2).rounded(.down),
        ]
        if let link = link.flatMap(URL.init(string:)) {
            attributes[.link] = link
        }
        return NSAttributedString(string: text, attributes: attributes)
    }
}
