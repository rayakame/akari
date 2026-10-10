import AkariKit
import SwiftUI

// Discord's status dot and voice and settings buttons wait for presence, voice and settings.
struct UserPanel: View {
    let user: User?

    var body: some View {
        HStack(spacing: 8) {
            if let user {
                InitialsAvatar(user, size: 32)
                VStack(alignment: .leading, spacing: 0) {
                    Text(user.displayName)
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(Color(nsColor: Palette.textStrong))
                    Text(user.username)
                        .font(.system(size: 12))
                        .foregroundStyle(Color(nsColor: Palette.textMuted))
                }
                .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .padding(8)
        .frame(height: 56)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color(nsColor: Palette.panel)))
        .overlay(
            RoundedRectangle(cornerRadius: 8).strokeBorder(
                Color(nsColor: Palette.borderMuted), lineWidth: 1))
    }
}
