import AkariKit
import SwiftUI

struct DirectMessageList: View {
    let session: SessionModel

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 2) {
                Text("Direct Messages")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Color(nsColor: Palette.textMuted))
                    .padding(.horizontal, 16)
                    .frame(height: 24)
                    .padding(.top, 12)
                ForEach(session.directMessages.conversations) { conversation in
                    ConversationRow(
                        conversation: conversation,
                        selected: session.messages?.channelId == conversation.id
                    ) {
                        session.open(channel: conversation.id)
                    }
                }
            }
            .padding(.bottom, 72)
        }
    }
}

private struct ConversationRow: View {
    let conversation: DirectMessageListModel.Conversation
    let selected: Bool
    let open: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: open) {
            HStack(spacing: 12) {
                avatar
                VStack(alignment: .leading, spacing: 0) {
                    Text(conversation.name)
                        .font(.system(size: 16, weight: selected ? .medium : .regular))
                        .foregroundStyle(
                            Color(
                                nsColor: selected || hovered
                                    ? Palette.interactiveTextActive : Palette.channelDefault)
                        )
                        .lineLimit(1)
                    if conversation.channel.kind == .groupDm {
                        Text("\(conversation.recipients.count + 1) Members")
                            .font(.system(size: 12))
                            .foregroundStyle(Color(nsColor: Palette.textMuted))
                    }
                }
                Spacer(minLength: 0)
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

    @ViewBuilder private var avatar: some View {
        if conversation.channel.kind == .dm, let user = conversation.recipients.first {
            InitialsAvatar(user, size: 32)
        } else {
            InitialsAvatar(
                name: conversation.name, id: conversation.id.rawValue, size: 32)
        }
    }
}
