import SwiftUI

struct EmptyState: View {
    let text: String?

    var body: some View {
        Group {
            if let text {
                Text(text)
                    .font(.system(size: 16))
                    .foregroundStyle(Color(nsColor: Palette.textMuted))
            } else {
                ProgressView()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}
