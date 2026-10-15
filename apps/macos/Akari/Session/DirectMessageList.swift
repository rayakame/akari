import AkariKit
import SwiftUI

struct DirectMessageList: View {
    let session: SessionModel

    var body: some View {
        VStack(spacing: 0) {
            ListHeader(title: "Direct Messages")
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    ForEach(session.directMessages.conversations) { conversation in
                        ConversationRow(
                            conversation: conversation,
                            selected: session.messages?.channelId == conversation.id,
                            badge: session.directMessages.badge(of: conversation.id)
                        ) {
                            session.open(channel: conversation.id)
                        }
                    }
                }
                .padding(.top, 8)
                .padding(.bottom, 72)
            }
        }
    }
}

extension DirectMessageList {
    static func memberLine(_ count: Int) -> String {
        count == 1 ? "1 Member" : "\(count) Members"
    }
}

private struct ConversationRow: View {
    let conversation: DirectMessageListModel.Conversation
    let selected: Bool
    var badge = Badge(unread: false, mentions: 0)
    let open: () -> Void
    @State private var hovered = false
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        Button(action: open) {
            HStack(spacing: 12) {
                avatar
                VStack(alignment: .leading, spacing: 0) {
                    Text(conversation.name)
                        .font(TypeScale.channelName.weight(weight).font)
                        .foregroundStyle(
                            Color(
                                nsColor: selected || hovered || badge.unread
                                    ? Palette.interactiveTextActive : Palette.channelDefault)
                        )
                        .lineLimit(1)
                    if conversation.channel.kind == .groupDm {
                        Text(DirectMessageList.memberLine(conversation.memberCount))
                            .font(TypeScale.small.font)
                            .foregroundStyle(Color(nsColor: Palette.textMuted))
                    }
                }
                Spacer(minLength: 0)
                if badge.mentions > 0 {
                    MentionBadge(count: badge.mentions)
                }
            }
            .padding(.horizontal, 8)
            .frame(height: 42)
            .background(
                RoundedRectangle(cornerRadius: 8).fill(
                    selected
                        ? Color(nsColor: Palette.selectedBackground)
                        : hovered ? Color(nsColor: Palette.hoverBackground) : .clear)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .padding(.horizontal, 8)
    }

    // As a channel row's name.
    private var weight: TextStyle.Weight {
        selected && colorScheme == .light ? .semibold : .medium
    }

    @ViewBuilder private var avatar: some View {
        if conversation.channel.kind == .dm, let user = conversation.recipients.first {
            InitialsAvatar(user, size: 32)
        } else {
            InitialsAvatar(
                name: conversation.name, id: conversation.id.rawValue, size: 32)
        }
    }
}
