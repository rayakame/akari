import AkariKit
import SwiftUI

struct ChannelHeader: View {
    static let height: CGFloat = 48

    /// A thread's Join Thread or Leave Thread, in the "…" menu.
    struct Membership {
        let item: SessionModel.MembershipItem
        let act: () -> Void
    }

    let channel: Channel?
    let name: String
    var parent: Channel?
    var openParent: () -> Void = {}
    var membership: Membership?

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
            if let membership {
                Menu {
                    Button(membership.item.title, action: membership.act)
                        .disabled(!membership.item.enabled)
                } label: {
                    Image(systemName: "ellipsis")
                        .font(.system(size: 16, weight: .semibold))
                        .foregroundStyle(Color(nsColor: Palette.interactiveText))
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
                .padding(.trailing, 8)
                .help("More")
                .accessibilityLabel("More")
            }
        }
        .padding(.leading, 16)
        .padding(.trailing, 8)
        .frame(height: Self.height)
        // Room for the line SessionView draws under both headers.
        .padding(.bottom, 1)
    }
}
