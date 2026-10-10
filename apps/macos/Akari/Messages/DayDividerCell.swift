import AppKit

final class DayDividerCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("day")

    private let label = CellText.label()

    init() {
        super.init(frame: .zero)
        identifier = Self.identifier
        label.font = .systemFont(ofSize: 12, weight: .semibold)
        label.textColor = Palette.textMuted
        addSubview(label)
        NSLayoutConstraint.activate([
            label.topAnchor.constraint(equalTo: topAnchor, constant: 16),
            label.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -4),
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
