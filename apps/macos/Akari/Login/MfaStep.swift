import AkariKit
import SwiftUI

struct MfaStep: View {
    @Bindable var model: LoginModel
    let challenge: MfaChallenge
    let method: MfaMethod

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            StepHeading(title: "Two-factor authentication", subtitle: hint)
            LabeledField(label: method == .backup ? "Backup code" : "Code", error: model.fieldError)
            {
                TextField("", text: $model.code)
                    .textContentType(.oneTimeCode)
                    .onSubmit { Task { await model.submit() } }
            }
            SubmitArea(model: model, title: "Log In")
            VStack(alignment: .leading, spacing: 8) {
                ForEach(otherMethods, id: \.self) { other in
                    LinkButton(Self.switchTitle(other)) {
                        Task { await model.choose(other) }
                    }
                }
                LinkButton("Go back") { model.backToForm() }
            }
        }
        .onExitCommand { model.backToForm() }
    }

    // Security keys need WebAuthn, which Akari doesn't support yet.
    private var otherMethods: [MfaMethod] {
        challenge.methods.filter { $0 != method && $0 != .webAuthn }
    }

    private var hint: String {
        switch method {
        case .totp: "Enter the 6-digit code from your authenticator app."
        case .sms:
            challenge.smsSentTo.map { "Enter the code sent to \($0)." }
                ?? "Enter the code from the text message."
        case .backup: "Enter one of your 8-digit backup codes."
        case .webAuthn: "Security keys aren't supported yet. Choose another way below."
        }
    }

    private static func switchTitle(_ method: MfaMethod) -> String {
        switch method {
        case .totp: "Use your authenticator app"
        case .sms: "Text me a code instead"
        case .backup: "Use a backup code"
        case .webAuthn: "Use a security key"
        }
    }
}
