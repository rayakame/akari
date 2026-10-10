import AppKit

final class DayDividerCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("day")

    private let label = CellText.label()
    private static let top: CGFloat = 16
    private static let bottom: CGFloat = 4
    private static let sizing = DayDividerCell()

    init() {
        super.init(frame: .zero)
        identifier = Self.identifier
        label.font = .systemFont(ofSize: 12, weight: .semibold)
        label.textColor = Palette.textMuted
        addSubview(label)
        NSLayoutConstraint.activate([
            label.topAnchor.constraint(equalTo: topAnchor, constant: Self.top),
            label.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Self.bottom),
            label.centerXAnchor.constraint(equalTo: centerXAnchor),
        ])
    }

    required init?(coder: NSCoder) {
        nil
    }

    /// The row height for a divider, as Auto Layout would size the cell.
    static func height(_ text: String) -> CGFloat {
        sizing.configure(text)
        return top + sizing.label.intrinsicContentSize.height + bottom
    }

    func configure(_ text: String) {
        label.stringValue = text
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        Palette.borderSubtle.setFill()
        let y = label.frame.midY.rounded(.down)
        NSRect(x: 16, y: y, width: max(0, label.frame.minX - 24), height: 1)
            .fill(using: .sourceOver)
        NSRect(
            x: label.frame.maxX + 8, y: y, width: max(0, bounds.maxX - 16 - label.frame.maxX - 8),
            height: 1
        )
        .fill(using: .sourceOver)
    }
}
