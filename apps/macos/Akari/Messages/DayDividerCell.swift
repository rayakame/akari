import AppKit

final class DayDividerCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("day")

    private let label = CellText.label()
    private static let top: CGFloat = 16
    private static let bottom: CGFloat = 4
    static var height: CGFloat { top + TypeScale.dayDivider.line + bottom }

    init() {
        super.init(frame: .zero)
        identifier = Self.identifier
        label.font = TypeScale.dayDivider.nsFont
        label.textColor = Palette.textMuted
        addSubview(label)
        // The table sizes the row; a required height would conflict while it moves row views.
        let height = heightAnchor.constraint(equalToConstant: Self.height)
        height.priority = .init(999)
        NSLayoutConstraint.activate([
            height,
            label.centerYAnchor.constraint(
                equalTo: topAnchor, constant: Self.top + TypeScale.dayDivider.line / 2),
            label.centerXAnchor.constraint(equalTo: centerXAnchor),
        ])
    }

    required init?(coder: NSCoder) {
        nil
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
