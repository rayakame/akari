import AppKit

// Where the channel begins, above its oldest message.
final class EdgeCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("edge")
    static let height: CGFloat = 48

    private let label = CellText.label()

    init() {
        super.init(frame: .zero)
        identifier = Self.identifier
        label.font = .systemFont(ofSize: 14)
        label.textColor = Palette.textMuted
        addSubview(label)
        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: Self.height),
            label.centerXAnchor.constraint(equalTo: centerXAnchor),
            label.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    required init?(coder: NSCoder) {
        nil
    }

    func show(_ text: String) {
        label.stringValue = text
    }
}
