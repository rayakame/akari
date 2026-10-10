import AkariKit
import SwiftUI

/// A colored circle with up to two initials, for users without a picture.
struct InitialsAvatar: View {
    let name: String
    let id: UInt64
    let size: CGFloat

    init(name: String, id: UInt64, size: CGFloat) {
        self.name = name
        self.id = id
        self.size = size
    }

    init(_ user: User, size: CGFloat) {
        self.init(name: user.displayName, id: user.id.rawValue, size: size)
    }

    var body: some View {
        Circle()
            .fill(Color(nsColor: Palette.avatars[Int(id % UInt64(Palette.avatars.count))]))
            .frame(width: size, height: size)
            .overlay {
                Text(Initials.of(name, limit: 2))
                    .font(.system(size: size * 0.4, weight: .semibold))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .minimumScaleFactor(0.5)
            }
            .accessibilityHidden(true)
    }
}
