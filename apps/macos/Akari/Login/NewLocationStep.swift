import AkariKit
import SwiftUI

/// Discord's confirmation link opens the official client, so Akari asks for the address it
/// opened instead (docs/ui/login.md).
struct NewLocationStep: View {
    @Bindable var model: LoginModel
    let via: NewLocation

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            StepHeading(title: "Confirm this login", subtitle: hint)
            LabeledField(label: via == .email ? "Link address" : "Code", error: model.fieldError) {
                TextField("", text: $model.code)
                    .textContentType(via == .email ? .URL : .oneTimeCode)
                    .onSubmit { Task { await model.submit() } }
            }
            SubmitArea(model: model, title: "Confirm")
            LinkButton("Go back") { model.backToForm() }
        }
        .onExitCommand { model.backToForm() }
    }

    private var hint: String {
        switch via {
        case .email:
            "Discord sent you an email to confirm a login from a new place. Open its link, "
                + "then paste the address the link opened here."
        case .phone: "Discord texted you a code to confirm a login from a new place."
        }
    }
}
