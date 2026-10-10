import AkariKit
import SwiftUI

struct ChannelHeader: View {
    static let height: CGFloat = 48

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
        .frame(height: Self.height)
        // Room for the line SessionView draws under both headers.
        .padding(.bottom, 1)
    }
}
