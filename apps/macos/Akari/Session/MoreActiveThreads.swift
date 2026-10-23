import AkariKit
import SwiftUI

/// When the channel list's "More Active Threads" shows, as Discord's: 200 ms after the pointer
/// rests on a text or announcement channel with active threads the list doesn't show, until
/// 250 ms after it leaves both the row and the popout. A click on the row closes it until the
/// pointer comes back.
@MainActor @Observable
final class ThreadsHover {
    static let showDelay: Duration = .milliseconds(200)
    static let hideDelay: Duration = .milliseconds(250)

    private(set) var channel: ChannelId?
    // Each row has its own: leaving one row for another closes the first row's popout unless
    // the second shows its own.
    @ObservationIgnored private var showing: Task<Void, Never>?
    @ObservationIgnored private var hiding: Task<Void, Never>?
    @ObservationIgnored private var row: ChannelId?
    @ObservationIgnored private var overPopout = false
    @ObservationIgnored private let wait: (Duration) async -> Void

    init(wait: @escaping (Duration) async -> Void = { try? await Task.sleep(for: $0) }) {
        self.wait = wait
    }

    /// The pointer entered or left a channel's row; `available` says whether it has threads to
    /// show when the delay ends.
    func hover(_ channel: ChannelId, _ inside: Bool, available: @escaping () -> Bool) {
        if inside {
            row = channel
            if channel == self.channel {
                hiding?.cancel()
            }
            showing?.cancel()
            showing = after(Self.showDelay) { [weak self] in
                if available() {
                    self?.channel = channel
                }
            }
        } else if row == channel {
            row = nil
            showing?.cancel()
            scheduleHide()
        }
    }

    func hoverPopout(_ inside: Bool) {
        overPopout = inside
        if inside {
            hiding?.cancel()
        } else {
            scheduleHide()
        }
    }

    /// The row left the list (another server, a collapsed category, a channel no longer listed):
    /// no pointer will leave it.
    func rowGone(_ channel: ChannelId) {
        if row == channel || self.channel == channel {
            dismiss()
        }
    }

    /// A click on the row, a row of the popout or "See All".
    func dismiss() {
        showing?.cancel()
        hiding?.cancel()
        row = nil
        overPopout = false
        channel = nil
    }

    private func scheduleHide() {
        hiding?.cancel()
        hiding = after(Self.hideDelay) { [weak self] in
            guard let self, row != channel, !overPopout else {
                return
            }
            channel = nil
        }
    }

    private func after(_ delay: Duration, _ action: @escaping @MainActor () -> Void) -> Task<
        Void, Never
    > {
        let wait = self.wait
        return Task { @MainActor in
            await wait(delay)
            if !Task.isCancelled {
                action()
            }
        }
    }
}

/// Where the hovered channel's row is, for the popout beside it.
struct PeekRowBounds: PreferenceKey {
    static let defaultValue: Anchor<CGRect>? = nil

    static func reduce(value: inout Anchor<CGRect>?, nextValue: () -> Anchor<CGRect>?) {
        value = value ?? nextValue()
    }
}

/// "More Active Threads": up to three threads, each with its owner's face, name and time, and
/// "See All", which opens the channel's Threads modal. 17 to the right of the row, its top level
/// with the row's while the window has room.
struct MoreActiveThreadsPopout: View {
    static let spacing: CGFloat = 17
    static let minWidth: CGFloat = 272
    static let title = "More Active Threads"
    static let seeAll = "See All"

    /// 8 around, the title's 8 + 18 + 2, then the threads and "See All", 34 each and 2 apart.
    static func height(threads: Int) -> CGFloat {
        8 + 28 + CGFloat(threads + 1) * 34 + CGFloat(threads) * 2 + 8
    }

    /// Discord's position layer aligns it with the row's top and moves it up to stay 12 inside
    /// the window's bottom, though never above 12.
    static func top(row: CGRect, height: CGFloat, area: CGFloat) -> CGFloat {
        guard row.minY >= 0 else {
            return 0
        }
        return row.minY + height + 12 > area ? max(12, area - 12 - height) : row.minY
    }

    let peeks: [ThreadPeek]
    var now: Date?
    let open: (ChannelId, _ beside: Bool) -> Void
    let seeAllThreads: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text(Self.title)
                .font(TypeScale.browserLabel.font)
                .foregroundStyle(Color(nsColor: Palette.textMuted))
                .lineLimit(1)
                .frame(height: TypeScale.browserText.line)
                .padding(.top, 8)
                .padding(.horizontal, 8)
                .padding(.bottom, 2)
            VStack(alignment: .leading, spacing: 2) {
                ForEach(peeks) { peek in
                    PeekRow(peek: peek, now: now) { beside in open(peek.id, beside) }
                }
                PeekRow(peek: nil, now: now) { _ in seeAllThreads() }
            }
        }
        .padding(8)
        .frame(minWidth: Self.minWidth, maxWidth: 400, alignment: .leading)
        .fixedSize()
        .background(Color(nsColor: Palette.surfaceHigh), in: RoundedRectangle(cornerRadius: 12))
        .overlay {
            RoundedRectangle(cornerRadius: 12)
                .strokeBorder(Color(nsColor: Palette.borderSubtle), lineWidth: 1)
        }
    }
}

/// A thread, or "See All" for `nil`.
struct PeekRow: View {
    let peek: ThreadPeek?
    var now: Date?
    let click: (_ beside: Bool) -> Void
    @State private var hovered = false

    var body: some View {
        HStack(spacing: 8) {
            leading
                .frame(width: 16, height: 16)
            if let peek {
                HStack(spacing: 4) {
                    Text(peek.name)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Text("\u{2022}")
                        .foregroundStyle(Color(nsColor: Palette.textMuted))
                    Text(time(peek.time))
                        .foregroundStyle(
                            Color(nsColor: hovered ? Palette.textStrong : Palette.textMuted))
                        .fixedSize()
                }
            } else {
                Text(MoreActiveThreadsPopout.seeAll)
            }
            Spacer(minLength: 0)
        }
        .frame(height: TypeScale.browserText.line)
        .font(TypeScale.browserText.font)
        .foregroundStyle(Color(nsColor: hovered ? Palette.textStrong : Palette.textDefault))
        .padding(8)
        .background(
            Color(nsColor: hovered ? Palette.backgroundModNormal : .clear),
            in: RoundedRectangle(cornerRadius: 4)
        )
        .contentShape(Rectangle())
        .onHover { hovered = $0 }
        .onTapGesture { click(NSEvent.modifierFlags.contains(.shift)) }
        .background(CursorArea(.pointingHand))
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
    }

    @ViewBuilder private var leading: some View {
        if let peek {
            if let owner = peek.owner {
                InitialsAvatar(owner, size: 16)
            } else {
                Circle()
                    .fill(Color(nsColor: InitialsAvatar.color(peek.ownerId?.rawValue ?? 0)))
            }
        } else {
            Image(systemName: ChannelSymbols.name(for: .publicThread))
                .font(.system(size: 13))
        }
    }

    private func time(_ date: Date) -> String {
        ThreadBox.relativeTime(since: date, now: now ?? Date(), locale: .current).text
    }
}
