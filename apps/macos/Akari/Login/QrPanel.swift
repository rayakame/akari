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
            QrTile { QrCodeView(url: url) }
        case .scanned(let user):
            InitialsAvatar(name: user.username, id: user.id.rawValue, size: 96)
                .frame(width: 176, height: 176)
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

/// The login code with Akari's logo over its middle, where the official client puts its own: a
/// brand tile with the white logo in a white ring. It covers few enough modules for the code's
/// error correction.
struct QrCodeView: View {
    let url: String
    static let side: CGFloat = 160
    static let logo: CGFloat = 26
    static let ring: CGFloat = 4

    var body: some View {
        if let image = QrCode.image(for: url, side: Self.side) {
            Image(nsImage: image)
                .interpolation(.none)
                .overlay {
                    RoundedRectangle(cornerRadius: 7 + Self.ring, style: .continuous)
                        .fill(.white)
                        .frame(width: Self.logo + 2 * Self.ring, height: Self.logo + 2 * Self.ring)
                        .overlay {
                            RoundedRectangle(cornerRadius: 7, style: .continuous)
                                .fill(Color(nsColor: Palette.brand))
                                .frame(width: Self.logo, height: Self.logo)
                                .overlay { AkariLogo().foregroundStyle(.white) }
                        }
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel("QR code to log in")
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
