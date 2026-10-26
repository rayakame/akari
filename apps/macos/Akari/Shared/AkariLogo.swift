import SwiftUI

/// Akari's logo in its square, placed as the app icon places it, in the foreground style.
/// Without a label it's decoration that assistive apps skip.
struct AkariLogo: View {
    var label: String?

    var body: some View {
        let image = Image("AkariLogo")
            .renderingMode(.template)
            .resizable()
            .aspectRatio(1, contentMode: .fit)
        if let label {
            image.accessibilityLabel(label)
        } else {
            image.accessibilityHidden(true)
        }
    }
}
