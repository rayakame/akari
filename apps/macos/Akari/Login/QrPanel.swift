import AkariKit
import SwiftUI

struct QrPanel: View {
    let model: LoginModel

    var body: some View {
        VStack(spacing: 16) {
            tile
            VStack(spacing: 8) {
                Text(heading)
                    .font(TypeScale.qrTitle.font)
                    .foregroundStyle(Color(nsColor: Palette.textStrong))
                Text(detail)
                    .font(TypeScale.detail.font)
                    .foregroundStyle(Color(nsColor: Palette.textMuted))
                    .fixedSize(horizontal: false, vertical: true)
                action
            }
            .multilineTextAlignment(.center)
        }
    }

    @ViewBuilder private var tile: some View {
        switch model.qr {
        case .connecting:
            QrTile { ProgressView().tint(.black) }
        case .code(let url):
            QrTile {
                if let image = QrCode.image(for: url, side: 160) {
                    Image(nsImage: image)
                        .interpolation(.none)
                        .accessibilityLabel("QR code to log in")
                }
            }
        case .scanned:
            // No account yet, so no image pipeline for the scanned user's avatar: the disc.
            Circle()
                .fill(Color(nsColor: Palette.avatarPlaceholder))
                .frame(width: 96, height: 96)
                .overlay {
                    Image(systemName: "person.fill")
                        .font(.system(size: 44))
                        .foregroundStyle(Color(nsColor: Palette.textMuted))
                }
                .frame(width: 176, height: 176)
                .accessibilityHidden(true)
        case .captchaUnsupported, .failed:
            Image(systemName: "exclamationmark.triangle")
                .font(.system(size: 48))
                .foregroundStyle(Color(nsColor: Palette.textMuted))
                .frame(width: 176, height: 176)
        }
    }

    private var heading: String {
        switch model.qr {
        case .connecting, .code: "Log in with your phone"
        case .scanned: "Confirm on your phone"
        case .captchaUnsupported, .failed: "QR login stopped"
        }
    }

    private var detail: String {
        switch model.qr {
        case .connecting, .code: "Scan it with the Discord app on your phone to log in."
        case .scanned(let user): "Logging in as \(user.username)."
        case .captchaUnsupported: "Discord asked for a captcha, which Akari can't show yet."
        case .failed(let error): error.localizedDescription
        }
    }

    @ViewBuilder private var action: some View {
        switch model.qr {
        case .connecting, .code:
            EmptyView()
        case .scanned:
            LinkButton("Wrong account? Show a new code") { model.restartQr() }
        case .captchaUnsupported, .failed:
            LinkButton("Try again") { model.restartQr() }
        }
    }
}

private struct QrTile<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        RoundedRectangle(cornerRadius: 8)
            .fill(.white)
            .frame(width: 176, height: 176)
            .overlay { content }
    }
}
