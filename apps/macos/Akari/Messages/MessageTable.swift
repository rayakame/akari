import AkariKit
import AppKit
import SwiftUI

// Fed with values read in a SwiftUI body, so Observation drives the updates.
struct MessageTable: NSViewRepresentable {
    let rows: [MessageListModel.Row]
    let atPresent: Bool
    let actions: MessageListActions

    func makeCoordinator() -> MessageTableController {
        MessageTableController()
    }

    func makeNSView(context: Context) -> NSScrollView {
        context.coordinator.scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.actions = actions
        context.coordinator.show(rows, atPresent: atPresent)
    }
}

struct MessageArea: View {
    let messages: MessageListModel
    let placeholder: String

    var body: some View {
        GeometryReader { geometry in
            VStack(spacing: 0) {
                MessageTable(
                    rows: messages.rows, atPresent: messages.atPresent,
                    actions: MessageListBridge(messages: messages)
                )
                .id(messages.channelId)
                .overlay { status }
                ComposerView(
                    composer: messages.composer, placeholder: placeholder,
                    areaHeight: geometry.size.height
                )
                .id(messages.channelId)
            }
        }
        .task(id: messages.channelId) { await messages.open() }
    }

    @ViewBuilder private var status: some View {
        if messages.rows.isEmpty {
            if let error = messages.loadFailure?.error {
                VStack(spacing: 12) {
                    Text(error.localizedDescription)
                        .foregroundStyle(Color(nsColor: Palette.textMuted))
                    Button("Try again") {
                        Task { await messages.jumpToPresent() }
                    }
                }
            } else if messages.loading != nil {
                ProgressView()
            }
        }
    }
}

// The table's actions on the channel's models.
final class MessageListBridge: MessageListActions {
    let messages: MessageListModel

    init(messages: MessageListModel) {
        self.messages = messages
    }

    func retry(_ message: MessageId) {
        Task { await messages.composer.retry(message) }
    }

    func delete(_ message: MessageId) {
        messages.composer.discard(message)
    }
}
