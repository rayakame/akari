import Foundation
import Testing

@testable import AkariKit

struct FormattedTextTests {
    @Test
    func offsetsCutTheContentAtTheRightPlaces() {
        let content = "😀 **日本** a\u{301} *🎉x* `👍` 𝒜 e\u{301}e"
        let text = formattedText(content: content)
        let shown = text.blocks.flatMap(\.spans).compactMap { span -> String? in
            switch span.kind {
            case .text(let start, let end), .code(let start, let end):
                (content as NSString).substring(
                    with: NSRange(location: Int(start), length: Int(end - start)))
            default:
                nil
            }
        }

        #expect(shown.joined() == "😀 日本 a\u{301} 🎉x 👍 𝒜 e\u{301}e")
        #expect(shown.contains("日本") && shown.contains("🎉x") && shown.contains("👍"))
    }

    @Test
    func roleIdsAreTyped() {
        let text = formattedText(content: "<@&5>")

        guard case .role(let id, _, _) = text.blocks.first?.spans.first?.kind else {
            Issue.record("no role mention in \(text)")
            return
        }
        let typed: RoleId = id
        #expect(typed == RoleId(rawValue: 5))
    }
}
