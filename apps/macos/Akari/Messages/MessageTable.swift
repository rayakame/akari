import AkariKit
import AppKit
import SwiftUI

// Fed with values read in a SwiftUI body, so Observation drives the updates.
struct MessageTable: NSViewRepresentable {
    let state: MessageTableState
    let actions: MessageListActions
    let link: MessageTableLink

    func makeCoordinator() -> MessageTableController {
        MessageTableController()
    }

    func makeNSView(context: Context) -> NSScrollView {
        context.coordinator.scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.actions = actions
        link.controller = context.coordinator
        context.coordinator.show(state)
    }
}

// Lets the SwiftUI parts around the table (jump bar, composer) reach its controller.
final class MessageTableLink {
    weak var controller: MessageTableController?
}

struct MessageArea: View {
    let messages: MessageListModel
    let name: ChannelName
    @State private var link = MessageTableLink()

    var body: some View {
        let bridge = MessageListBridge(messages: messages, link: link)
        GeometryReader { geometry in
            VStack(spacing: 0) {
                MessageTable(state: state, actions: bridge, link: link)
                    .id(messages.channelId)
                    .overlay { status }
                    .overlay(alignment: .top) {
                        if StaleCapsule.isShown(
                            atPresent: messages.atPresent, isStale: messages.isStale,
                            hasRows: !messages.rows.isEmpty)
                        {
                            StaleCapsule()
                        }
                    }
                    .overlay(alignment: .bottom) {
                        if let text = JumpBar.text(
                            atPresent: messages.atPresent, isStale: messages.isStale,
                            hasRows: !messages.rows.isEmpty)
                        {
                            JumpBar(text: text) { bridge.escape() }
                        }
                    }
                // Above the composer, as in the official client: send errors on the left,
                // slowmode on the right.
                ComposerStatus(composer: messages.composer)
                ComposerView(
                    composer: messages.composer, placeholder: name.placeholder,
                    areaHeight: geometry.size.height,
                    onSend: { link.controller?.jumpToPresent(load: false) },
                    onEscape: { bridge.escape() }
                )
                .id(messages.channelId)
            }
        }
        .task(id: messages.channelId) { await messages.open() }
    }

    private var state: MessageTableState {
        MessageTableState(
            rows: messages.rows, atPresent: messages.atPresent,
            reachedOldest: messages.reachedOldest, loading: messages.loading,
            failedLoads: messages.failedLoads, beginning: name.beginning)
    }

    @ViewBuilder private var status: some View {
        if messages.rows.isEmpty {
            if let error = messages.failedLoads[.latest] ?? messages.failedLoads.values.first {
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

final class MessageListBridge: MessageListActions {
    let messages: MessageListModel
    let link: MessageTableLink

    init(messages: MessageListModel, link: MessageTableLink) {
        self.messages = messages
        self.link = link
    }

    func retry(_ message: MessageId) {
        Task { await messages.composer.retry(message) }
    }

    func delete(_ message: MessageId) {
        messages.composer.discard(message)
    }

    func loadMore(_ edge: MessageTimeline.Edge) {
        Task {
            switch edge {
            case .older: await messages.loadOlder()
            case .newer: await messages.loadNewer()
            case .beginning: break
            }
        }
    }

    func jumpToLatest() {
        Task { await messages.jumpToPresent() }
    }

    func escape() {
        link.controller?.jumpToPresent(load: true)
    }
}
