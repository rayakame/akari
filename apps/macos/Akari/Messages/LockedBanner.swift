import SwiftUI

struct LockedBanner: View {
    static let height: CGFloat = 32
    static let margin: CGFloat = 4
    static let text = "This thread has been locked. Only moderators can send messages."

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "lock.fill")
                .font(.system(size: 12, weight: .semibold))
                .accessibilityHidden(true)
            Text(Self.text)
                .lineLimit(1)
            Spacer(minLength: 0)
        }
        .font(TypeScale.jumpBar.font)
        .foregroundStyle(Color(nsColor: Palette.textDefault))
        .padding(.horizontal, 12)
        .frame(height: Self.height)
        .background(Color(nsColor: Palette.card), in: RoundedRectangle(cornerRadius: 8))
        .padding(.horizontal, 16)
        .padding(.top, Self.margin)
    }
}
