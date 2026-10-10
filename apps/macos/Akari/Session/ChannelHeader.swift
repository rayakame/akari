import AkariKit
import SwiftUI

/// The bar above the messages: the channel's symbol, its name and topic.
struct ChannelHeader: View {
    let channel: Channel?
    let name: String

    var body: some View {
        HStack(spacing: 8) {
            if let channel {
                ChannelSymbol(channel: channel)
                Text(name)
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(Color(nsColor: Palette.textStrong))
                    .lineLimit(1)
                if let topic = channel.topic, !topic.isEmpty {
                    Rectangle()
                        .fill(Color(nsColor: Palette.borderSubtle))
                        .frame(width: 1, height: 24)
                        .padding(.horizontal, 4)
                    Text(topic)
                        .font(.system(size: 14))
                        .foregroundStyle(Color(nsColor: Palette.textMuted))
                        .lineLimit(1)
                        .help(topic)
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.leading, 16)
        .padding(.trailing, 8)
        .frame(height: 48)
        .overlay(alignment: .bottom) {
            Rectangle().fill(Color(nsColor: Palette.borderSubtle)).frame(height: 1)
        }
    }
}
