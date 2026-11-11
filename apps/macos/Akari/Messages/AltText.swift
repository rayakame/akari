import AppKit

/// An ALT button's popout: Discord's title over the media's description, 360 wide at most.
final class AltTextController: NSViewController {
    static let title = "IMAGE DESCRIPTION (ALT TEXT)"
    static let maxWidth: CGFloat = 360
    let text: String

    init(text: String) {
        self.text = text
        super.init(nibName: nil, bundle: nil)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override func loadView() {
        let title = NSTextField(labelWithString: Self.title)
        title.font = .systemFont(ofSize: TypeScale.size(12), weight: .bold)
        title.textColor = Palette.textStrong
        let body = NSTextField(wrappingLabelWithString: text)
        body.font = TypeScale.body.nsFont
        body.textColor = Palette.textDefault
        body.isSelectable = true
        body.preferredMaxLayoutWidth = Self.maxWidth - 24
        let stack = NSStackView(views: [title, body])
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 9.5
        stack.edgeInsets = NSEdgeInsets(top: 13.5, left: 12, bottom: 13.5, right: 12)
        stack.setHuggingPriority(.required, for: .horizontal)
        view = stack
    }
}
