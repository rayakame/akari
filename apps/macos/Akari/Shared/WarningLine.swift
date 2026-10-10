import SwiftUI

struct WarningLine: View {
    let text: String
    let dismiss: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.triangle")
            Text(text)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
            Button(action: dismiss) {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .help("Dismiss")
        }
        .font(.system(size: 13))
        .foregroundStyle(Color(nsColor: Palette.textStrong))
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .background(Color(nsColor: Palette.panel))
    }
}
