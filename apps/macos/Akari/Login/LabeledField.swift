import SwiftUI

/// A form field with its label above it; an error shows in the label line, in red.
struct LabeledField<Field: View>: View {
    let label: String
    let error: String?
    @ViewBuilder let field: Field

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 4) {
                Text(label.uppercased())
                    .foregroundStyle(
                        Color(nsColor: error == nil ? Palette.textMuted : Palette.textError))
                if let error {
                    Text("– \(error)")
                        .italic()
                        .foregroundStyle(Color(nsColor: Palette.textError))
                }
            }
            .font(.system(size: 12, weight: .bold))
            .lineLimit(2)
            field
                .textFieldStyle(.plain)
                .font(.system(size: 16))
                .foregroundStyle(Color(nsColor: Palette.textDefault))
                .padding(.horizontal, 10)
                .frame(height: 40)
                .background(
                    RoundedRectangle(cornerRadius: 4).fill(Color(nsColor: Palette.inputBackground)))
        }
    }
}

/// The full-width button at the bottom of each step.
struct PrimaryButton: View {
    let title: String
    let busy: Bool
    let enabled: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            ZStack {
                Text(title).opacity(busy ? 0 : 1)
                if busy {
                    ProgressView().controlSize(.small).tint(.white)
                }
            }
            .font(.system(size: 16, weight: .medium))
            .foregroundStyle(.white)
            .frame(maxWidth: .infinity, minHeight: 44)
            .background(RoundedRectangle(cornerRadius: 4).fill(Color(nsColor: Palette.brand)))
            .opacity(enabled || busy ? 1 : 0.5)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
    }
}

/// A text button in the link color.
struct LinkButton: View {
    let title: String
    let action: () -> Void

    init(_ title: String, action: @escaping () -> Void) {
        self.title = title
        self.action = action
    }

    var body: some View {
        Button(title, action: action)
            .buttonStyle(.plain)
            .font(.system(size: 14))
            .foregroundStyle(Color(nsColor: Palette.textLink))
    }
}
