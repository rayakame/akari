import AkariKit
import AppKit
import SwiftUI

// Fed with values read in a SwiftUI body, so Observation drives the updates.
struct MessageTable: NSViewRepresentable {
    let rows: [MessageListModel.Row]
    let atPresent: Bool

    func makeCoordinator() -> MessageTableController {
        MessageTableController()
    }

    func makeNSView(context: Context) -> NSScrollView {
        context.coordinator.scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.show(rows, atPresent: atPresent)
    }
}

struct MessageArea: View {
    let messages: MessageListModel

    var body: some View {
        MessageTable(rows: messages.rows, atPresent: messages.atPresent)
            .id(messages.channelId)
            .overlay { status }
            .task(id: messages.channelId) { await messages.open() }
    }

    @ViewBuilder private var status: some View {
        if messages.rows.isEmpty {
            if let error = messages.loadError {
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
