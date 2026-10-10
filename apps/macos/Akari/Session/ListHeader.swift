import SwiftUI

// Above the channel or DM list, as tall as the channel header so one line runs under both.
struct ListHeader: View {
    let title: String

    var body: some View {
        Text(title)
            .font(.system(size: 16, weight: .semibold))
            .foregroundStyle(Color(nsColor: Palette.textStrong))
            .lineLimit(1)
            .padding(.horizontal, 16)
            .frame(
                maxWidth: .infinity, minHeight: ChannelHeader.height,
                maxHeight: ChannelHeader.height, alignment: .leading
            )
            .padding(.bottom, 1)
    }
}
