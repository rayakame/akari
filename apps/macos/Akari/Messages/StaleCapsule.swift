import SwiftUI

// While a new session checks the open channel for messages it missed.
struct StaleCapsule: View {
    var body: some View {
        HStack(spacing: 6) {
            ProgressView().controlSize(.mini)
            Text("Catching up…")
        }
        .font(.system(size: 12))
        .foregroundStyle(Color(nsColor: Palette.textMuted))
        .padding(.horizontal, 10)
        .frame(height: 24)
        .background(Color(nsColor: Palette.panel), in: Capsule())
        .padding(.top, 8)
    }

    static func isShown(atPresent: Bool, isStale: Bool, hasRows: Bool) -> Bool {
        isStale && atPresent && hasRows
    }
}
