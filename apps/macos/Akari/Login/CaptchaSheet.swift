import SwiftUI

/// Captchas need a web view, which comes later; QR login doesn't ask for one.
struct CaptchaSheet: View {
    let dismiss: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Captcha required")
                .font(.title2.weight(.semibold))
            Text(
                "Discord wants a captcha for this login, and Akari can't show captchas yet. "
                    + "Log in with the QR code instead: scan it with the Discord app on your phone."
            )
            .fixedSize(horizontal: false, vertical: true)
            HStack {
                Spacer()
                Button("OK", action: dismiss)
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(24)
        .frame(width: 420)
    }
}
