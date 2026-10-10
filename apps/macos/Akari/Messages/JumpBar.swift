import SwiftUI

struct JumpBar: View {
    static let height: CGFloat = 32
    static let margin: CGFloat = 4

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
        .frame(height: Self.height)
        .background(Color(nsColor: Palette.brand), in: RoundedRectangle(cornerRadius: 8))
        .padding(.horizontal, 16)
        .padding(.bottom, Self.margin)
    }

    static func text(atPresent: Bool, isStale: Bool, hasRows: Bool) -> String? {
        guard !atPresent, hasRows else {
            return nil
        }
        return isStale ? "Some messages may be missing" : "You're reading older messages"
    }
}
