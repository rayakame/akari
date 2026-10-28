import AppKit
import Testing

@testable import Akari

@MainActor
struct MessageTextViewTests {
    let window = TestWindow.make(size: NSSize(width: 900, height: 400))

    func shown(_ text: NSAttributedString, width: CGFloat) -> MessageTextView {
        let view = MessageTextView()
        view.textStorage?.setAttributedString(text)
        let container = NSView(frame: NSRect(x: 0, y: 0, width: width, height: 400))
        window.contentView = container
        container.addSubview(view)
        NSLayoutConstraint.activate([
            view.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            view.topAnchor.constraint(equalTo: container.topAnchor),
            view.widthAnchor.constraint(equalToConstant: width),
        ])
        container.layoutSubtreeIfNeeded()
        return view
    }

    @Test
    func theTextViewIsTextKit1() {
        let view = shown(CellText.body("some text to select", color: .labelColor), width: 400)

        #expect(view.layoutManager is MessageLayoutManager)
        #expect(view.textLayoutManager == nil)
        window.makeFirstResponder(view)
        view.setSelectedRange(NSRange(location: 0, length: 4))
        view.selectAll(nil)
        #expect(view.textLayoutManager == nil)
        #expect(view.layoutManager is MessageLayoutManager)
    }

    // Plain words, and text whose line breaking and fallback fonts are harder to predict.
    static let samples: [String] =
        [1, 7, 30, 90].map { String(repeating: "words that wrap ", count: $0) } + [
            "https://example.com/" + String(repeating: "averyverylongpathsegment", count: 12),
            String(repeating: "😀🎉👍🏽 ", count: 40),
            String(repeating: "日本語のテキストは単語の間に空白がありません。", count: 8),
            String(repeating: "مرحبا بالعالم هذا نص طويل ", count: 12),
            String(repeating: "שלום עולם זה טקסט ארוך ", count: 12),
            String(repeating: "e\u{301}a\u{308}o\u{302} ", count: 60),
            "first line\nsecond line\n\nafter a blank line\n",
            "ends with a newline\n",
            String(repeating: "short\n", count: 12),
            "mixed English, 日本語, עברית and 😀 in one " + String(repeating: "long line ", count: 30),
        ]

    @Test(arguments: [512, 292, 812] as [CGFloat])
    func heightsMatchTheTextViewAtEveryWidth(_ width: CGFloat) {
        for sample in Self.samples {
            let text = CellText.body(sample, color: .labelColor)
            let view = shown(text, width: width)

            #expect(
                view.frame.height == TextMeasurer.shared.height(text, width: width),
                "\(sample.prefix(30)) at \(width)")
            #expect(view.frame.height == view.intrinsicContentSize.height)
        }
    }
}
