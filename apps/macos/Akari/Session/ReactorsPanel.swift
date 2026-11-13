import AkariKit
import SwiftUI

/// The official client's reactions modal by its sheet: the emoji in a column 24 in, 98 wide
/// with 16 before its divider, tabs 32 high and 8 apart, users in rows of 52.
enum ReactorsLayout {
    static let width: CGFloat = 600
    static let height: CGFloat = 480
    static let margin: CGFloat = 40
    static let headerHeight: CGFloat = 64
    static let inset: CGFloat = 24
    static let tabsWidth: CGFloat = 98
    static let tabsTrailing: CGFloat = 16
    static let tabHeight: CGFloat = 32
    static let tabGap: CGFloat = 8
    static let emojiSide: CGFloat = 24
    static let rowHeight: CGFloat = 52
    static let rowLeading: CGFloat = 16
    static let avatar: CGFloat = 32
    static let nameGap: CGFloat = 10
    static let radius: CGFloat = 12

    static func frame(in area: CGSize) -> CGRect {
        let size = CGSize(
            width: min(width, max(0, area.width - 2 * margin)),
            height: min(height, max(0, area.height - 2 * margin)))
        return CGRect(
            x: ((area.width - size.width) / 2).rounded(), y: ((area.height - size.height) / 2).rounded(),
            width: size.width, height: size.height)
    }
}

/// "View Reactions": the message's emoji by count on the left, the users who reacted with the
/// chosen one on the right. The X, Escape or a press outside closes it.
struct ReactorsPanel: View {
    let reactors: ReactorsModel
    let size: CGSize
    let close: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            header
            HStack(spacing: 0) {
                tabs
                Rectangle()
                    .fill(Color(nsColor: Palette.borderSubtle))
                    .frame(width: 1)
                users
            }
            .padding(.leading, ReactorsLayout.inset)
        }
        .frame(width: size.width - 2, height: size.height - 2)
        .background(Color(nsColor: Palette.surfaceHigh))
        .clipShape(RoundedRectangle(cornerRadius: ReactorsLayout.radius))
        .padding(1)
        .overlay {
            RoundedRectangle(cornerRadius: ReactorsLayout.radius + 1)
                .strokeBorder(Color(nsColor: Palette.borderSubtle), lineWidth: 1)
        }
        .accessibilityElement(children: .contain)
        .firstClicks()
        .task(id: ObjectIdentifier(reactors)) { await reactors.open() }
    }

    private var header: some View {
        HStack(spacing: 0) {
            Text(ReactionWords.window)
                .font(TypeScale.emptyTitle.font)
                .foregroundStyle(Color(nsColor: Palette.textStrong))
                .accessibilityAddTraits(.isHeader)
            Spacer(minLength: 0)
            ModalCloseButton(action: close)
        }
        .padding(.leading, ReactorsLayout.inset)
        .padding(.trailing, 16)
        .frame(height: ReactorsLayout.headerHeight)
    }

    private var tabs: some View {
        ScrollView {
            VStack(spacing: ReactorsLayout.tabGap) {
                ForEach(reactors.tabs) { pill in
                    ReactorsTab(pill: pill, selected: pill.id == reactors.selected) {
                        Task { await reactors.select(pill.id) }
                    }
                }
            }
            .padding(.trailing, ReactorsLayout.tabsTrailing)
            .padding(.bottom, 16)
        }
        .scrollIndicators(.never)
        .frame(width: ReactorsLayout.tabsWidth + ReactorsLayout.tabsTrailing)
    }

    private var users: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                ForEach(reactors.reactors) { reactor in
                    ReactorRow(reactor: reactor) {
                        Task { await reactors.remove(reactor) }
                    }
                }
                if reactors.failed {
                    ReactorsRetry { Task { await reactors.loadMore() } }
                } else if reactors.hasMore || reactors.loading {
                    ProgressView()
                        .controlSize(.small)
                        .frame(maxWidth: .infinity)
                        .frame(height: ReactorsLayout.rowHeight)
                        // The spinner's row comes on screen near the list's end: the next page.
                        .onAppear { Task { await reactors.loadMore() } }
                        .id(reactors.reactors.count)
                }
            }
            .padding(.bottom, 8)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// One emoji of the window's left column: the emoji at 24 and its count.
struct ReactorsTab: View {
    let pill: ReactionPill
    let selected: Bool
    let select: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: select) {
            HStack(spacing: 0) {
                ReactionEmojiView(emoji: pill.emoji, side: ReactorsLayout.emojiSide)
                    .padding(.vertical, 4)
                    .padding(.leading, 4)
                    .padding(.trailing, 8)
                Text(pill.count.formatted())
                    .font(TypeScale.forumNewCount.font)
                    .monospacedDigit()
                    .foregroundStyle(
                        Color(nsColor: selected || hovered ? Palette.interactiveTextActive : Palette.interactiveText))
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 4)
            .frame(height: ReactorsLayout.tabHeight)
            .background(Color(nsColor: fill), in: RoundedRectangle(cornerRadius: 8))
            .contentShape(RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .background(CursorArea(.pointingHand))
        .help(pill.name)
        .accessibilityLabel(ReactionWords.tab(pill))
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private var fill: NSColor {
        if selected {
            return hovered ? Palette.selectedBackground : Palette.interactiveBackgroundActive
        }
        return hovered ? Palette.hoverBackground : .clear
    }
}

/// One user who reacted: the avatar, the display name over the username, and the ✕ that removes
/// the reaction while the row is hovered.
struct ReactorRow: View {
    let reactor: ReactorsModel.Reactor
    let remove: () -> Void
    /// Fixed for tests; otherwise the pointer's.
    var hoveredLook: Bool?
    @State private var hovered = false

    var body: some View {
        HStack(spacing: 0) {
            UserAvatar(reactor.user, size: ReactorsLayout.avatar, hovered: hovered)
            VStack(alignment: .leading, spacing: 0) {
                Text(reactor.user.displayName)
                    .font(TypeScale.formTitle.font)
                    .foregroundStyle(Color(nsColor: Palette.textDefault))
                    .lineLimit(1)
                Text(reactor.user.username)
                    .font(TypeScale.small.font)
                    .foregroundStyle(Color(nsColor: Palette.textMuted))
                    .lineLimit(1)
            }
            .padding(.horizontal, ReactorsLayout.nameGap)
            Spacer(minLength: 0)
            if reactor.removable {
                Button(action: remove) {
                    Image(systemName: "xmark")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(Color(nsColor: Palette.interactiveText))
                        .frame(width: 24, height: 24)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .background(CursorArea(.pointingHand))
                .opacity(hoveredLook ?? hovered ? 1 : 0)
                .help(ReactionWords.remove)
                .accessibilityLabel(ReactionWords.remove)
            }
        }
        .padding(.leading, ReactorsLayout.rowLeading)
        .padding(.trailing, 16)
        .frame(height: ReactorsLayout.rowHeight)
        .contentShape(Rectangle())
        .onHover { hovered = $0 }
        .accessibilityElement(children: .contain)
    }
}

/// In place of the spinner when a page didn't come.
struct ReactorsRetry: View {
    static let text = "Couldn't load who reacted."
    let retry: () -> Void

    var body: some View {
        HStack(spacing: 6) {
            Text(Self.text)
                .foregroundStyle(Color(nsColor: Palette.textMuted))
            Button("Retry", action: retry)
                .buttonStyle(.plain)
                .foregroundStyle(Color(nsColor: Palette.textLink))
                .background(CursorArea(.pointingHand))
        }
        .font(TypeScale.detail.font)
        .frame(maxWidth: .infinity)
        .frame(height: ReactorsLayout.rowHeight)
    }
}

/// A reaction's emoji in a SwiftUI place: a custom one from the image tier, a Unicode one in
/// the system's emoji font, both filling a box `side` wide.
struct ReactionEmojiView: View {
    let emoji: ReactionEmoji
    let side: CGFloat
    var hovered = false

    var body: some View {
        Group {
            if let key = emoji.key {
                RemoteImage(key: key, place: .emoji, hovered: hovered, fit: .contain)
            } else {
                UnicodeEmoji(emoji: emoji.name ?? "")
            }
        }
        .frame(width: side, height: side)
        .accessibilityHidden(true)
    }
}

private struct UnicodeEmoji: NSViewRepresentable {
    let emoji: String

    func makeNSView(context: Context) -> EmojiGlyphView {
        EmojiGlyphView()
    }

    func updateNSView(_ view: EmojiGlyphView, context: Context) {
        view.emoji = emoji
    }
}
