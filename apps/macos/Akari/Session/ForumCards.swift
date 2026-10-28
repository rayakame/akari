import AkariKit
import AppKit
import SwiftUI

/// The forum page's measures, from Discord's CSS (canary 653948).
enum ForumMeasures {
    static let inset: CGFloat = 16
    static let galleryInset: CGFloat = 24
    static let top: CGFloat = 20
    static let radius: CGFloat = 12
    static let control: CGFloat = 32
    static let cardGap: CGFloat = 8
    static let bottom: CGFloat = 24
    static let thumbnail: CGFloat = 72
    static let placeholderHeight: CGFloat = 102
    static let olderHeight: CGFloat = 40
    static let galleryGap: CGFloat = 16
    static let columnMin: CGFloat = 320
    static let columnMax: CGFloat = 450
    // Discord loads the next page within 200 of the list's bottom.
    static let loadDistance: CGFloat = 200

    /// Discord's masonry columns for the gallery's width: one of 320 up to 320, else as many as
    /// lie between 450- and 320-wide ones, sharing the width with 16 between them.
    static func columns(width: CGFloat) -> (count: Int, width: CGFloat) {
        guard width > columnMin else {
            return (1, columnMin)
        }
        let wide = width / columnMax
        let count = max(Int((wide + (width / columnMin - wide) / 2).rounded(.down)), 1)
        return (count, (width - galleryGap * CGFloat(count - 1)) / CGFloat(count))
    }

    /// A gallery card: its 60 header, the media at 2:3 inside its 12 margins and border, 24, and
    /// the 36 footer.
    static func galleryCardHeight(width: CGFloat) -> CGFloat {
        60 + ((width - 24 - 2) * 2 / 3).rounded(.up) + 24 + 36
    }

    /// As many placeholder cards as fill the window below Discord's 200 of header, 118 each.
    static func placeholderCards(height: CGFloat) -> Int {
        max(Int(((height - 200) / 118).rounded()), 1)
    }
}

/// A post's card in the list: its tags, title, first message and count, the first media at the
/// right. A button, opened with the hand, as Discord's cards.
struct ForumListCard: View {
    let card: ForumModel.Card
    let filter: Set<TagId>
    var now: Date?
    let open: () -> Void
    @State private var hovered = false
    @Environment(\.placeholders) private var placeholders

    var body: some View {
        Button(action: open) {
            HStack(alignment: .top, spacing: 0) {
                VStack(alignment: .leading, spacing: 0) {
                    if card.pinned || !card.tags.isEmpty {
                        ForumCardTags(card: card, filter: filter)
                            .padding(.bottom, 8)
                    }
                    Text(card.title)
                        .font(TypeScale.forumTitle.font)
                        .foregroundStyle(Color(nsColor: Palette.textMuted))
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                        .padding(.bottom, 6)
                    ForumCardMessage(preview: card.preview)
                        .frame(height: TypeScale.forumMessage.line)
                    ForumCardFooter(card: card, now: now)
                        .padding(.top, 8)
                }
                .padding(.bottom, 8)
                .frame(maxWidth: .infinity, alignment: .leading)
                if let media = card.media.first {
                    ForumThumbnail(media: media, placeholders: placeholders)
                        .frame(width: ForumMeasures.thumbnail, height: ForumMeasures.thumbnail)
                        .padding(.leading, 12)
                }
            }
            .padding(.vertical, 12)
            .padding(.horizontal, 10)
        }
        .buttonStyle(ForumCardStyle(hovered: hovered))
        .onHover { hovered = $0 }
        .background(CursorArea(.pointingHand))
        .accessibilityLabel(ForumListCard.label(card))
    }

    /// "Post {title}, {count} message(s)", as Discord's.
    static func label(_ card: ForumModel.Card) -> String {
        "Post \(card.title), \(card.count) \(card.count == "1" ? "message" : "messages")"
    }
}

/// The card's look: base-low under a subtle border, raised with the stronger border and a
/// shadow while hovered, darker while pressed.
struct ForumCardStyle: ButtonStyle {
    let hovered: Bool
    var radius = ForumMeasures.radius

    func makeBody(configuration: Configuration) -> some View {
        let shape = RoundedRectangle(cornerRadius: radius)
        let fill =
            configuration.isPressed
            ? Palette.surfaceHigher : hovered ? Palette.surfaceHigh : Palette.panel
        return configuration.label
            .background(Color(nsColor: fill), in: shape)
            .overlay {
                shape.strokeBorder(
                    Color(nsColor: hovered ? Palette.borderNormal : Palette.borderSubtle),
                    lineWidth: 1)
            }
            .shadow(color: .black.opacity(hovered ? 0.16 : 0), radius: 2, y: 1)
            .contentShape(shape)
    }
}

/// The pinned badge, then the first three tags and "+N" for the rest (named in its tooltip).
struct ForumCardTags: View {
    let card: ForumModel.Card
    let filter: Set<TagId>

    static let shown = 3

    var body: some View {
        HStack(spacing: 8) {
            if card.pinned {
                Circle()
                    .fill(Color(nsColor: Palette.brand))
                    .frame(width: 22, height: 22)
                    .overlay {
                        Image(systemName: "pin.fill")
                            .font(.system(size: 11, weight: .semibold))
                            .foregroundStyle(.white)
                    }
                    .help("Pinned")
                    .accessibilityLabel("Pinned")
            }
            ForEach(card.tags.prefix(Self.shown), id: \.id) { tag in
                ForumTagPill(
                    tag: tag, size: .small, look: filter.contains(tag.id) ? .inFilter : .onCard)
            }
            if card.tags.count > Self.shown {
                let rest = card.tags.dropFirst(Self.shown)
                Text("+\(rest.count)")
                    .font(TypeScale.forumTagSmall.font)
                    .foregroundStyle(Color(nsColor: Palette.interactiveText))
                    .padding(.horizontal, 8)
                    .frame(height: 24)
                    .background(Color(nsColor: Palette.backgroundModNormal), in: Capsule())
                    .help(rest.map(\.name).joined(separator: ", "))
            }
        }
        .frame(height: 24)
    }
}

/// A tag's pill: 32 in the bar (12 inside), 24 on a card (8 inside), its emoji before its name.
struct ForumTagPill: View {
    enum Size {
        case medium, small

        var height: CGFloat { self == .medium ? 32 : 24 }
        var padding: CGFloat { self == .medium ? 12 : 8 }
        var emoji: CGFloat { self == .medium ? 14 : 12 }
        var style: TextStyle { self == .medium ? TypeScale.forumTag : TypeScale.forumTagSmall }
    }

    enum Look {
        /// In the bar, out of the filter.
        case idle
        /// In the bar and the filter: the brand's tint and border.
        case selected
        /// On a card.
        case onCard
        /// On a card, in the filter: the lowest background.
        case inFilter
    }

    let tag: ForumTag
    var size = Size.medium
    var look = Look.idle
    var hovered = false

    var body: some View {
        HStack(spacing: 0) {
            if let emoji = tag.emoji {
                TagEmojiView(emoji: emoji, size: size.emoji)
                    .padding(.trailing, 6)
            }
            Text(tag.name)
                .font(size.style.font)
                .lineLimit(1)
        }
        .foregroundStyle(Color(nsColor: look == .selected ? Palette.mentionText : text))
        .padding(.horizontal, size.padding)
        .frame(height: size.height)
        .background(fill, in: Capsule())
        .overlay {
            Capsule().strokeBorder(
                Color(nsColor: look == .selected ? Palette.brand : Palette.borderSubtle),
                lineWidth: 1)
        }
    }

    private var text: NSColor {
        hovered ? Palette.interactiveTextActive : Palette.interactiveText
    }

    private var fill: Color {
        switch look {
        case .selected: Color(nsColor: Palette.brand).opacity(0.15)
        case .inFilter: Color(nsColor: Palette.baseLowest)
        case .idle, .onCard:
            Color(nsColor: hovered ? Palette.backgroundModStrong : Palette.backgroundModNormal)
        }
    }
}

/// A tag's emoji: Unicode as text, a custom one from the image tier.
struct TagEmojiView: View {
    let emoji: TagEmoji
    let size: CGFloat

    var body: some View {
        switch emoji {
        case .unicode(let text):
            Text(text)
                .font(.system(size: size))
        case .custom(_, let key):
            RemoteImage(key: key, place: .emoji, fit: .contain)
                .frame(width: size, height: size)
        }
    }
}

/// "Ray: hello", the first message on one line; Discord's words without one.
struct ForumCardMessage: View {
    let preview: ForumModel.Card.Preview

    static let deleted = "Original message was deleted"

    var body: some View {
        HStack(spacing: 0) {
            switch preview {
            case .message(let message, let author):
                Text(author + ":")
                    .font(TypeScale.forumAuthor.font)
                    .fixedSize()
                Self.content(of: message)
                    .font(TypeScale.forumMessage.font)
                    .padding(.leading, 4)
            case .deleted:
                Text(Self.deleted)
                    .font(TypeScale.forumMessage.font)
            case .loading:
                Color.clear
            }
        }
        .foregroundStyle(Color(nsColor: Palette.textMuted))
        .lineLimit(1)
        .truncationMode(.tail)
    }

    static func content(of message: Message) -> Text {
        if let placeholder = ThreadBox.placeholder(for: message) {
            return Text(placeholder).italic()
        }
        let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)
        let built = NSMutableAttributedString(
            attributedString: ThreadBoxView.content(
                MessageText.build(message, revealed: [], now: Date(), times: times).text))
        // A read post's message is muted, its mentions and links keep their colors.
        let whole = NSRange(location: 0, length: built.length)
        built.enumerateAttribute(.foregroundColor, in: whole) { value, range, _ in
            if let color = value as? NSColor,
                color == Palette.textDefault || color == Palette.textStrong
            {
                built.addAttribute(.foregroundColor, value: Palette.textMuted, range: range)
            }
        }
        let attributed =
            (try? AttributedString(built, including: \.appKit)) ?? AttributedString(built.string)
        return Text(attributed)
    }
}

/// The message count, "•" and the time; its tooltip says when the post was made.
struct ForumCardFooter: View {
    let card: ForumModel.Card
    var now: Date?

    var body: some View {
        HStack(spacing: 8) {
            ForumCardCount(count: card.count, size: 14)
            Text("\u{2022}")
                .foregroundStyle(Color(nsColor: Palette.borderSubtle))
            ForumTime(date: card.time, now: now)
                .font(TypeScale.forumTime.font)
                .foregroundStyle(Color(nsColor: Palette.textDefault))
                .help("Posted \(card.created.formatted(date: .complete, time: .shortened))")
        }
        .frame(height: 20)
    }
}

struct ForumCardCount: View {
    let count: String
    let size: CGFloat
    var muted = false

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: "bubble.left")
                .font(.system(size: size - 2))
                .foregroundStyle(Color(nsColor: muted ? Palette.textMuted : Palette.textDefault))
            Text(count)
                .font(TypeScale.forumFooter.font)
                .foregroundStyle(Color(nsColor: muted ? Palette.interactiveText : Palette.textDefault))
        }
        .accessibilityElement(children: .combine)
    }
}

/// "5m ago", "3h ago", "2d ago", ">30d ago", then the date; `prefix` for the gallery's "Posted".
struct ForumTime: View {
    let date: Date
    var now: Date?
    var prefix = ""

    var body: some View {
        if let now {
            Text(prefix + ThreadBox.relativeTime(since: date, now: now, locale: .current).text)
        } else {
            TimelineView(.periodic(from: .now, by: 60)) { context in
                Text(
                    prefix
                        + ThreadBox.relativeTime(since: date, now: context.date, locale: .current)
                        .text)
            }
        }
    }
}

/// The first media at a card's right, 72 square: covered media shows only its placeholder and
/// the eye, as Discord blurs it.
struct ForumThumbnail: View {
    let media: Attachment
    let placeholders: PlaceholderCache?
    var radius: CGFloat = 8

    var body: some View {
        ZStack {
            Color(nsColor: Palette.backgroundModMuted)
            if let blurred = media.placeholder.flatMap({ placeholders?.image($0) }) {
                Image(decorative: blurred, scale: 1)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
            }
            if media.cover == nil {
                RemoteImage(key: media.key, place: .gif, fit: .cover)
            } else {
                Image(systemName: "eye.slash.fill")
                    .font(.system(size: 18))
                    .foregroundStyle(.white)
                    .shadow(radius: 2)
            }
            if media.kind == .video {
                Image(systemName: "play.fill")
                    .font(.system(size: 14))
                    .foregroundStyle(.white)
                    .padding(6)
                    .background(.black.opacity(0.6), in: Circle())
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: radius))
        .accessibilityHidden(true)
    }
}

/// A post's card in the gallery: author and age, the title, the media (at most 4, "+N") or the
/// message's start, the tags over the media's bottom, and the count.
struct ForumGalleryCard: View {
    let card: ForumModel.Card
    let filter: Set<TagId>
    let size: CGSize
    var now: Date?
    let open: () -> Void
    @State private var hovered = false
    @Environment(\.placeholders) private var placeholders

    var body: some View {
        Button(action: open) {
            VStack(alignment: .leading, spacing: 0) {
                header
                    .frame(height: 60 - 22, alignment: .topLeading)
                    .padding(EdgeInsets(top: 12, leading: 12, bottom: 10, trailing: 12))
                content(height: size.height - 60 - 24 - 36)
                    .padding(.horizontal, 12)
                    .overlay(alignment: .bottomLeading) {
                        if !card.tags.isEmpty {
                            ForumCardTags(card: card, filter: filter)
                                .padding(.leading, 18)
                                .offset(y: 12 - 2)
                        }
                    }
                Spacer(minLength: 24)
                ForumCardCount(count: card.count, size: 16, muted: true)
                    .padding(12)
                    .frame(height: 36 + 12, alignment: .leading)
            }
            .frame(width: size.width, height: size.height, alignment: .topLeading)
        }
        .buttonStyle(ForumCardStyle(hovered: hovered))
        .onHover { hovered = $0 }
        .background(CursorArea(.pointingHand))
        .accessibilityLabel(ForumListCard.label(card))
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 6) {
                if case .message(_, let author) = card.preview {
                    Text(author)
                        .font(TypeScale.forumAuthor.font)
                        .foregroundStyle(Color(nsColor: Palette.textMuted))
                        .lineLimit(1)
                }
                ForumTime(date: card.created, now: now, prefix: "Posted ")
                    .font(TypeScale.forumGalleryTime.font)
                    .foregroundStyle(Color(nsColor: Palette.textDefault))
                    .lineLimit(1)
            }
            Text(card.title)
                .font(TypeScale.forumGalleryTitle.font)
                .foregroundStyle(Color(nsColor: Palette.textMuted))
                .lineLimit(1)
        }
    }

    @ViewBuilder private func content(height: CGFloat) -> some View {
        if card.media.isEmpty {
            preview
                .frame(maxWidth: .infinity, maxHeight: height, alignment: .topLeading)
        } else {
            ForumMosaic(media: card.media, placeholders: placeholders)
                .frame(height: height)
        }
    }

    // The message's start, fading out at the bottom.
    private var preview: some View {
        Group {
            switch card.preview {
            case .message(let message, _):
                ForumCardMessage.content(of: message)
                    .font(TypeScale.forumMessage.font)
                    .foregroundStyle(Color(nsColor: Palette.textMuted))
            case .deleted:
                Text(ForumCardMessage.deleted)
                    .font(TypeScale.forumMessage.font)
                    .foregroundStyle(Color(nsColor: Palette.textMuted))
            case .loading:
                Color.clear
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .padding(EdgeInsets(top: 12, leading: 12, bottom: 0, trailing: 12))
        .mask {
            LinearGradient(
                stops: [.init(color: .black, location: 0.6), .init(color: .clear, location: 1)],
                startPoint: .top, endPoint: .bottom)
        }
        .background(Color(nsColor: Palette.panel), in: RoundedRectangle(cornerRadius: 8))
        .overlay {
            RoundedRectangle(cornerRadius: 8)
                .strokeBorder(Color(nsColor: Palette.borderSubtle), lineWidth: 1)
        }
    }
}

/// Up to four media: one fills, two side by side, three as one beside two, four in a square;
/// "+N" for the rest.
struct ForumMosaic: View {
    let media: [Attachment]
    let placeholders: PlaceholderCache?

    private static let gap: CGFloat = 4

    var body: some View {
        let shown = Array(media.prefix(4))
        GeometryReader { proxy in
            let size = proxy.size
            let half = (size.width - Self.gap) / 2
            let halfHeight = (size.height - Self.gap) / 2
            Group {
                switch shown.count {
                case 1:
                    tile(shown[0])
                case 2:
                    HStack(spacing: Self.gap) {
                        tile(shown[0])
                        tile(shown[1])
                    }
                case 3:
                    HStack(spacing: Self.gap) {
                        tile(shown[0]).frame(width: half)
                        VStack(spacing: Self.gap) {
                            tile(shown[1])
                            tile(shown[2])
                        }
                    }
                default:
                    VStack(spacing: Self.gap) {
                        HStack(spacing: Self.gap) {
                            tile(shown[0])
                            tile(shown[1])
                        }
                        .frame(height: halfHeight)
                        HStack(spacing: Self.gap) {
                            tile(shown[2])
                            tile(shown[3])
                        }
                    }
                }
            }
            .frame(width: size.width, height: size.height)
        }
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay(alignment: .bottomTrailing) {
            if media.count > 4 {
                Text("+\(media.count - 4)")
                    .font(TypeScale.forumTagSmall.font)
                    .foregroundStyle(Color(nsColor: Palette.textDefault))
                    .padding(.horizontal, 4)
                    .frame(height: 24)
                    .background(Color(nsColor: Palette.surfaceHigh), in: RoundedRectangle(cornerRadius: 4))
                    .padding(8)
            }
        }
    }

    private func tile(_ attachment: Attachment) -> some View {
        ForumThumbnail(media: attachment, placeholders: placeholders, radius: 0)
    }
}

extension EnvironmentValues {
    /// The session's placeholder images, for the forum's cards.
    @Entry var placeholders: PlaceholderCache?
}
