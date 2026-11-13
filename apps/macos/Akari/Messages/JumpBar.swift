import SwiftUI

struct JumpBar: View {
    static let height: CGFloat = 32
    static let margin: CGFloat = 4

    /// The official client's words while the list shows the message a reply answers.
    static let viewingReply = "You Are Viewing a Reply."
    static let toOriginal = "Jump To Original Message"

    let text: String
    let button: String
    let symbol: String
    let jump: () -> Void

    var body: some View {
        HStack {
            Text(text)
            Spacer(minLength: 8)
            Button(action: jump) {
                Label(button, systemImage: symbol)
            }
            .buttonStyle(.plain)
            .background(CursorArea(.pointingHand))
        }
        .font(TypeScale.jumpBar.font)
        .foregroundStyle(.white)
        .padding(.horizontal, 12)
        .frame(height: Self.height)
        .background(Color(nsColor: Palette.brand), in: RoundedRectangle(cornerRadius: 8))
        .padding(.horizontal, 16)
        .padding(.bottom, Self.margin)
    }

    /// What the bar says and what its button does.
    enum Kind: Equatable {
        case present(String)
        /// The list jumped from a reply's line: the button goes back to the reply.
        case reply

        var text: String {
            switch self {
            case .present(let text): text
            case .reply: JumpBar.viewingReply
            }
        }

        var button: String {
            self == .reply ? JumpBar.toOriginal : "Jump to present"
        }

        var symbol: String {
            self == .reply ? "arrow.uturn.backward" : "arrow.down"
        }
    }

    init(_ kind: Kind, jump: @escaping () -> Void) {
        self.init(text: kind.text, button: kind.button, symbol: kind.symbol, jump: jump)
    }

    init(
        text: String, button: String = "Jump to present", symbol: String = "arrow.down",
        jump: @escaping () -> Void
    ) {
        self.text = text
        self.button = button
        self.symbol = symbol
        self.jump = jump
    }

    static func kind(atPresent: Bool, isStale: Bool, hasRows: Bool, fromReply: Bool) -> Kind? {
        guard let text = text(atPresent: atPresent, isStale: isStale, hasRows: hasRows) else {
            return nil
        }
        return fromReply ? .reply : .present(text)
    }

    static func text(atPresent: Bool, isStale: Bool, hasRows: Bool) -> String? {
        guard !atPresent, hasRows else {
            return nil
        }
        return isStale ? "Some messages may be missing" : "You're reading older messages"
    }
}
