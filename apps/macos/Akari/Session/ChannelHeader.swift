import AkariKit
import SwiftUI

struct ChannelHeader: View {
    static let height: CGFloat = 48

    let channel: Channel?
    let name: String
    var parent: Channel?
    var openParent: () -> Void = {}
    /// The Threads button, where the channel has one.
    var threads: ThreadsButton?
    /// A thread's "…".
    var menu: ThreadMenu?
    var act: (ThreadMenu.Item) -> Void = { _ in }

    var body: some View {
        HStack(spacing: 8) {
            if let channel {
                if let parent, channel.isThread {
                    ChannelSymbol(channel: parent)
                    Button(action: openParent) {
                        Text(parent.name ?? "")
                            .font(TypeScale.channelHeader.font)
                            .foregroundStyle(Color(nsColor: Palette.interactiveText))
                            .lineLimit(1)
                    }
                    .buttonStyle(.plain)
                    .background(PointingHandArea())
                    .accessibilityLabel(parent.name ?? "")
                    Image(systemName: "chevron.right")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(Color(nsColor: Palette.textMuted))
                        .accessibilityHidden(true)
                }
                ChannelSymbol(channel: channel)
                Text(name)
                    .font(TypeScale.channelHeader.font)
                    .foregroundStyle(Color(nsColor: Palette.textStrong))
                    .lineLimit(1)
                if let topic = channel.topic, !topic.isEmpty {
                    Rectangle()
                        .fill(Color(nsColor: Palette.borderSubtle))
                        .frame(width: 1, height: 24)
                        .padding(.horizontal, 4)
                    Text(topic)
                        .font(TypeScale.topic.font)
                        .foregroundStyle(Color(nsColor: Palette.textMuted))
                        .lineLimit(1)
                        .help(topic)
                }
            }
            Spacer(minLength: 0)
            if let threads {
                threads
                    .padding(.trailing, menu == nil ? 8 : 0)
            }
            if let menu {
                ThreadMenuButton(menu: menu, act: act)
                    .padding(.trailing, 8)
            }
        }
        .padding(.leading, 16)
        .padding(.trailing, 8)
        .frame(height: Self.height)
        // Room for the line SessionView draws under both headers.
        .padding(.bottom, 1)
    }
}
