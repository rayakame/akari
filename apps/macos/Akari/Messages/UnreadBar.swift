import AkariKit
import AppKit
import SwiftUI

extension EnvironmentValues {
    @Entry var announce: @MainActor (String) -> Void = UnreadBar.announce
}

struct UnreadBar: View {
    static let height: CGFloat = 32
    static let margin: CGFloat = 4

    let text: String
    let jump: () -> Void
    let markRead: () -> Void
    @Environment(\.announce) private var announce

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
        // Not on a recount: VoiceOver would repeat the bar with every new message.
        .onAppear { announce(text) }
    }

    static func announce(_ text: String) {
        let element: NSResponder = NSApp.mainWindow ?? NSApp
        NSAccessibility.post(
            element: element, notification: .announcementRequested,
            userInfo: [
                .announcement: text, .priority: NSAccessibilityPriorityLevel.medium.rawValue,
            ])
    }

    static func text(
        _ bar: MessageListModel.UnreadBar, calendar: Calendar = .autoupdatingCurrent,
        locale: Locale = .autoupdatingCurrent
    ) -> String {
        bar.text(time: MessageFormat.shortTime(bar.since, calendar: calendar, locale: locale))
    }
}
