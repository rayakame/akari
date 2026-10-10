import AkariKit
import SwiftUI

// A notice shows once it has lasted its delay, so a quick resume doesn't flash it.
struct ConnectionBar: View {
    let notice: ConnectionNotice?
    let reconnect: () -> Void
    @State private var shown: ConnectionNotice?

    var body: some View {
        Group {
            if let shown {
                bar(shown)
            }
        }
        .task(id: notice) {
            guard let notice else {
                shown = nil
                return
            }
            if notice.delay > 0 {
                try? await Task.sleep(for: .seconds(notice.delay))
                guard !Task.isCancelled else {
                    return
                }
            }
            shown = notice
        }
    }

    private func bar(_ notice: ConnectionNotice) -> some View {
        let closed = notice.offersReconnect
        return HStack(spacing: 8) {
            if notice == .connecting || notice == .reconnecting {
                ProgressView().controlSize(.small)
            }
            Text(notice.text)
                .lineLimit(1)
                .truncationMode(.tail)
            if closed {
                Button("Reconnect", action: reconnect)
                    .buttonStyle(.plain)
                    .padding(.horizontal, 10)
                    .frame(height: 22)
                    .overlay(RoundedRectangle(cornerRadius: 4).stroke(.white, lineWidth: 1))
            }
        }
        .font(.system(size: 13, weight: .medium))
        .foregroundStyle(closed ? Color.white : Color(nsColor: Palette.textMuted))
        .frame(maxWidth: .infinity)
        .frame(height: 32)
        .background(Color(nsColor: closed ? Palette.danger : Palette.panel))
    }
}
