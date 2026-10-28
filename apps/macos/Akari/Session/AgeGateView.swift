import AkariKit
import SwiftUI

/// The age gate in place of an age-restricted channel's messages and composer (D-10): Discord's
/// words, Go Back, and Continue unless Discord knows the user is under 18.
struct AgeGateView: View {
    let gate: AgeGate
    let agree: () -> Void
    let back: () -> Void

    static let goBack = "Go Back"
    static let proceed = "Continue"

    /// Title, text and whether the user can continue, as the official client words them
    /// (canary 649848, module 323073).
    static func words(_ gate: AgeGate) -> (title: String, text: String, continues: Bool) {
        switch gate {
        case .ask(server: false):
            (
                "Age-Restricted Channel",
                "This channel contains adult content marked as age-restricted. Do you wish to proceed?",
                true
            )
        case .ask(server: true):
            (
                "Age-Restricted Server",
                "This server contains sensitive content marked as age-restricted. Do you wish to proceed?",
                true
            )
        case .tooYoung(server: false):
            (
                "Sorry, you're not old enough to view this age-restricted channel",
                "You need to be 18 or older in order to view age-restricted channels on Discord.",
                false
            )
        case .tooYoung(server: true):
            (
                "You do not have access to this age-restricted server",
                "You must be over legal age in order to view age-restricted servers.",
                false
            )
        case .open:
            ("", "", false)
        }
    }

    var body: some View {
        let words = Self.words(gate)
        VStack(spacing: 16) {
            VStack(spacing: 8) {
                Text(words.title)
                    .font(TypeScale.gateTitle.font)
                    .foregroundStyle(Color(nsColor: Palette.textStrong))
                    .frame(maxWidth: 640)
                Text(words.text)
                    .font(TypeScale.gateText.font)
                    .foregroundStyle(Color(nsColor: Palette.textMuted))
                    .frame(maxWidth: 440)
            }
            .multilineTextAlignment(.center)
            .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 16) {
                BrandButton(title: Self.goBack, secondary: true, action: back)
                if words.continues {
                    BrandButton(title: Self.proceed, action: agree)
                }
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: Palette.chat))
    }
}
