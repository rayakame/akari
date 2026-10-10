import AkariKit
import SwiftUI

struct CredentialsForm: View {
    @Bindable var model: LoginModel
    @State private var revealed = false

    private static let register = URL(string: "https://discord.com/register")!

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            StepHeading(title: "Welcome back", subtitle: "Log in with your Discord account.")
            LabeledField(label: "Email or phone number", error: model.fieldError) {
                TextField("", text: $model.login)
                    .textContentType(.username)
                    .onSubmit(submit)
            }
            LabeledField(label: "Password", error: model.fieldError) {
                HStack(spacing: 8) {
                    Group {
                        if revealed {
                            TextField("", text: $model.password)
                        } else {
                            SecureField("", text: $model.password)
                        }
                    }
                    .textContentType(.password)
                    .onSubmit(submit)
                    Button {
                        revealed.toggle()
                    } label: {
                        Image(systemName: revealed ? "eye.slash" : "eye")
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(Color(nsColor: Palette.interactiveText))
                    .help(revealed ? "Hide password" : "Show password")
                }
            }
            SubmitArea(model: model, title: "Log In")
            HStack(spacing: 4) {
                Text("Need an account?")
                    .foregroundStyle(Color(nsColor: Palette.textMuted))
                Link("Register", destination: Self.register)
                    .foregroundStyle(Color(nsColor: Palette.textLink))
            }
            .font(.system(size: 14))
        }
    }

    private func submit() {
        Task { await model.submit() }
    }
}
