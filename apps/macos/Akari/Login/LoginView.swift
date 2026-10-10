import AkariKit
import SwiftUI

struct LoginView: View {
    @Bindable var model: LoginModel

    var body: some View {
        ViewThatFits(in: .horizontal) {
            HStack(alignment: .center, spacing: 64) {
                form.frame(width: 416)
                QrPanel(model: model).frame(width: 240)
            }
            form.frame(maxWidth: 416)
        }
        .padding(32)
        .background(RoundedRectangle(cornerRadius: 8).fill(Color(nsColor: Palette.card)))
        .padding(24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: Palette.frame))
        .onAppear { model.appear() }
        .onDisappear { model.disappear() }
        .sheet(isPresented: captchaShown) {
            CaptchaSheet { model.dismissCaptcha() }
        }
    }

    private var form: some View {
        VStack(alignment: .leading, spacing: 20) {
            if let notice = model.notice {
                NoticeBanner(notice: notice)
            }
            switch model.step {
            case .credentials, .captchaUnsupported:
                CredentialsForm(model: model)
            case .mfa(let challenge, let method):
                MfaStep(model: model, challenge: challenge, method: method)
            case .newLocation(let via):
                NewLocationStep(model: model, via: via)
            }
        }
    }

    private var captchaShown: Binding<Bool> {
        Binding {
            model.step == .captchaUnsupported
        } set: { shown in
            if !shown {
                model.dismissCaptcha()
            }
        }
    }
}

struct StepHeading: View {
    let title: String
    let subtitle: String

    var body: some View {
        VStack(spacing: 8) {
            Text(title)
                .font(.system(size: 24, weight: .semibold))
                .foregroundStyle(Color(nsColor: Palette.textStrong))
            Text(subtitle)
                .font(.system(size: 16))
                .foregroundStyle(Color(nsColor: Palette.textMuted))
                .fixedSize(horizontal: false, vertical: true)
        }
        .multilineTextAlignment(.center)
        .frame(maxWidth: .infinity)
    }
}

struct SubmitArea: View {
    let model: LoginModel
    let title: String

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            VStack(alignment: .leading, spacing: 8) {
                if let until = model.rateLimitedUntil, context.date < until {
                    let seconds = Int(until.timeIntervalSince(context.date).rounded(.up))
                    problem("Too many attempts. Try again in \(seconds) s.")
                } else if let message = model.message {
                    problem(message)
                }
                PrimaryButton(
                    title: title, busy: model.isSubmitting,
                    enabled: model.canSubmit(at: context.date)
                ) {
                    Task { await model.submit() }
                }
            }
        }
    }

    private func problem(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 14))
            .foregroundStyle(Color(nsColor: Palette.danger))
            .fixedSize(horizontal: false, vertical: true)
    }
}

private struct NoticeBanner: View {
    let notice: LoginNotice

    var body: some View {
        Label(text, systemImage: "info.circle")
            .font(.system(size: 14))
            .foregroundStyle(Color(nsColor: Palette.textDefault))
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 4).fill(Color(nsColor: Palette.inputBackground)))
    }

    private var text: String {
        switch notice {
        case .sessionExpired: "Your session ended. Log in again."
        case .reconnectFailed: "Akari couldn't reconnect. Log in again."
        case .tokenUnreadable:
            "Akari couldn't read your saved login from the Keychain. Log in again."
        }
    }
}
