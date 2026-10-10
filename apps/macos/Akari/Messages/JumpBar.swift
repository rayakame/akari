import SwiftUI

// Above the composer while older messages are shown.
struct JumpBar: View {
    let text: String
    let jump: () -> Void

    var body: some View {
        HStack {
            Text(text)
            Spacer(minLength: 8)
            Button(action: jump) {
                Label("Jump to present", systemImage: "arrow.down")
            }
            .buttonStyle(.plain)
        }
        .font(.system(size: 14, weight: .medium))
        .foregroundStyle(.white)
        .padding(.horizontal, 12)
        .frame(height: 32)
        .background(Color(nsColor: Palette.brand), in: RoundedRectangle(cornerRadius: 8))
        .padding(.horizontal, 16)
        .padding(.bottom, 4)
    }

    /// `nil` while the newest messages are loaded.
    static func text(atPresent: Bool, isStale: Bool, hasRows: Bool) -> String? {
        guard !atPresent, hasRows else {
            return nil
        }
        return isStale ? "Some messages may be missing" : "You're reading older messages"
    }
}
