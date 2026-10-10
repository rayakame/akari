import AkariKit
import SwiftUI

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
            .fill(Color(nsColor: Self.color(id)))
            .frame(width: size, height: size)
            .overlay {
                Text(Self.letters(name))
                    .font(.system(size: size * 0.4, weight: .semibold))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .minimumScaleFactor(0.5)
            }
            .accessibilityHidden(true)
    }

    // Server initials keep punctuation; an avatar of "Ren, Bo" should read "RB".
    static func letters(_ name: String) -> String {
        Initials.of(name.filter { !$0.isPunctuation }, limit: 2)
    }

    static func color(_ id: UInt64) -> NSColor {
        Palette.avatars[Int(id % UInt64(Palette.avatars.count))]
    }
}
