import AkariKit
import SwiftUI

struct ComposerStatus: View {
    static let height: CGFloat = 24

    let composer: ComposerModel

    var body: some View {
        HStack(spacing: 8) {
            if let problem = composer.problem {
                Text(problem.text)
                    .foregroundStyle(Color(nsColor: Palette.danger))
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .help(problem.text)
                    .modifier(Backdrop())
            }
            Spacer(minLength: 0)
            if let slowmode = composer.slowmode {
                slowmodeView(slowmode)
                    .modifier(Backdrop())
            }
        }
        .font(.system(size: 12))
        .padding(.horizontal, 16 - Backdrop.padding)
        .frame(height: Self.height)
    }

    // Messages scroll on under the strip; its texts keep the chat's background behind them.
    private struct Backdrop: ViewModifier {
        static let padding: CGFloat = 4

        func body(content: Content) -> some View {
            content
                .padding(.horizontal, Self.padding)
                .padding(.vertical, 2)
                .background(Color(nsColor: Palette.chat), in: RoundedRectangle(cornerRadius: 4))
        }
    }

    @ViewBuilder private func slowmodeView(_ slowmode: Slowmode) -> some View {
        if let until = slowmode.until, until > Date() {
            // Ticks only while a cooldown runs, so an idle composer wakes nothing.
            TimelineView(.periodic(from: .now, by: 1)) { context in
                Label(
                    Self.countdown(max(0, until.timeIntervalSince(context.date))),
                    systemImage: "timer"
                )
                .monospacedDigit()
            }
            .foregroundStyle(Color(nsColor: Palette.textMuted))
        } else {
            Label(Self.note(slowmode), systemImage: "timer")
                .foregroundStyle(Color(nsColor: Palette.textMuted))
        }
    }

    // Rounded up, so it never shows 0:00 while the user still has to wait.
    static func countdown(_ seconds: TimeInterval) -> String {
        let total = Int(seconds.rounded(.up))
        let (hours, minutes, rest) = (total / 3600, total / 60 % 60, total % 60)
        let padded = { (value: Int) in value < 10 ? "0\(value)" : "\(value)" }
        if hours > 0 {
            return "\(hours):\(padded(minutes)):\(padded(rest))"
        }
        return "\(total / 60):\(padded(rest))"
    }

    static func note(_ slowmode: Slowmode) -> String {
        if slowmode.exempt {
            return "Slowmode is on, but it doesn't apply to you"
        }
        return "Slowmode: one message every \(every(slowmode.interval))"
    }

    private static func every(_ interval: TimeInterval) -> String {
        let seconds = Int(interval)
        let (value, unit) =
            seconds % 3600 == 0
            ? (seconds / 3600, "hour")
            : seconds % 60 == 0 ? (seconds / 60, "minute") : (seconds, "second")
        return value == 1 ? unit : "\(value) \(unit)s"
    }
}
