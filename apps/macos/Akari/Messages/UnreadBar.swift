import AkariKit
import SwiftUI

struct UnreadBar: View {
    static let height: CGFloat = 32
    static let margin: CGFloat = 4

    let text: String
    let jump: () -> Void
    let markRead: () -> Void

    var body: some View {
        // All of the bar jumps but its button.
        HStack(spacing: 0) {
            Button(action: jump) {
                Text(text)
                    .lineLimit(1)
                    .padding(.leading, 12)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
            }
            Button(action: markRead) {
                Text("Mark As Read")
                    .padding(.horizontal, 12)
                    .frame(maxHeight: .infinity)
                    .contentShape(Rectangle())
            }
        }
        .buttonStyle(.plain)
        .font(TypeScale.jumpBar.font)
        .foregroundStyle(.white)
        .frame(height: Self.height)
        .background(Color(nsColor: Palette.brand), in: RoundedRectangle(cornerRadius: 8))
        .padding(.horizontal, 16)
        .padding(.top, Self.margin)
    }

    static func text(
        _ bar: MessageListModel.UnreadBar, calendar: Calendar = .autoupdatingCurrent,
        locale: Locale = .autoupdatingCurrent
    ) -> String {
        bar.text(time: MessageFormat.shortTime(bar.since, calendar: calendar, locale: locale))
    }
}
