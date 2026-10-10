import AkariKit
import AppKit

// Gray stand-ins for the messages beyond the loaded ones, so a fling carries on into them; the
// real messages take their place when the page lands.
final class PlaceholderCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("placeholder")
    // Group starts and follow-up lines, repeated; fixed, so a row's height never changes.
    private static let rows: [CGFloat] = [66, 26, 48, 88, 26, 66, 110, 48]
    private static let widths: [CGFloat] = [0.55, 0.8, 0.35, 0.7, 0.6, 0.45]
    private static let band: CGFloat = 48

    /// Whole rows covering at least one and a half views.
    static func height(forViewHeight viewHeight: CGFloat) -> CGFloat {
        let target = 1.5 * max(viewHeight, 400)
        var total: CGFloat = 0
        var index = 0
        while total < target {
            total += rows[index % rows.count]
            index += 1
        }
        return total
    }

    var onRetry: () -> Void = {}
    private var edge = MessageTimeline.Edge.older
    private var failed = false
    private let label = CellText.label()
    private let retry = NSButton(title: "Try again", target: nil, action: nil)
    private let errorRow: NSStackView
    private var heightConstraint: NSLayoutConstraint?
    private var atTop: NSLayoutConstraint?
    private var atBottom: NSLayoutConstraint?

    override var isFlipped: Bool { true }

    init() {
        errorRow = NSStackView(views: [label, retry])
        super.init(frame: .zero)
        identifier = Self.identifier
        label.font = .systemFont(ofSize: 14)
        label.textColor = Palette.textMuted
        retry.isBordered = false
        retry.attributedTitle = NSAttributedString(
            string: "Try again",
            attributes: [.font: NSFont.systemFont(ofSize: 14), .foregroundColor: Palette.textLink])
        retry.target = self
        retry.action = #selector(retryClicked)
        errorRow.orientation = .horizontal
        errorRow.spacing = 8
        errorRow.translatesAutoresizingMaskIntoConstraints = false
        addSubview(errorRow)
        let height = heightAnchor.constraint(equalToConstant: Self.height(forViewHeight: 0))
        let atTop = errorRow.centerYAnchor.constraint(
            equalTo: topAnchor, constant: Self.band / 2)
        let atBottom = errorRow.centerYAnchor.constraint(
            equalTo: bottomAnchor, constant: -Self.band / 2)
        heightConstraint = height
        self.atTop = atTop
        self.atBottom = atBottom
        NSLayoutConstraint.activate([
            height, errorRow.centerXAnchor.constraint(equalTo: centerXAnchor), atBottom,
        ])
    }

    required init?(coder: NSCoder) {
        nil
    }

    // Older placeholders show a failure next to the messages below them, newer ones above.
    func show(_ edge: MessageTimeline.Edge, height: CGFloat, failure: String?) {
        self.edge = edge
        failed = failure != nil
        heightConstraint?.constant = height
        atBottom?.isActive = edge != .newer
        atTop?.isActive = edge == .newer
        label.stringValue = failure ?? ""
        errorRow.isHidden = failure == nil
        needsDisplay = true
    }

    override func draw(_ dirtyRect: NSRect) {
        Palette.hoverBackground.setFill()
        let band =
            !failed
            ? nil
            : edge == .newer
                ? NSRect(x: 0, y: 0, width: bounds.width, height: Self.band)
                : NSRect(x: 0, y: bounds.height - Self.band, width: bounds.width, height: Self.band)
        let text = max(bounds.width - 72 - 16, 40)
        var y: CGFloat = 0
        var index = 0
        while y < bounds.height {
            let height = Self.rows[index % Self.rows.count]
            var shapes: [NSBezierPath] = []
            let width = { (offset: Int) in text * Self.widths[(index + offset) % Self.widths.count]
            }
            if height >= 66 {
                shapes.append(NSBezierPath(ovalIn: NSRect(x: 16, y: y + 18, width: 40, height: 40)))
                shapes.append(bar(x: 72, y: y + 20, width: min(width(0), 160)))
                for line in 0..<Int((height - 44) / 22) {
                    shapes.append(
                        bar(x: 72, y: y + 42 + CGFloat(line) * 22, width: width(line + 1)))
                }
            } else {
                for line in 0..<max(1, Int(height / 22)) {
                    shapes.append(bar(x: 72, y: y + 7 + CGFloat(line) * 22, width: width(line)))
                }
            }
            for shape in shapes where band.map({ !$0.intersects(shape.bounds) }) ?? true {
                shape.fill()
            }
            y += height
            index += 1
        }
    }

    private func bar(x: CGFloat, y: CGFloat, width: CGFloat) -> NSBezierPath {
        NSBezierPath(
            roundedRect: NSRect(x: x, y: y, width: width, height: 12), xRadius: 6, yRadius: 6)
    }

    @objc private func retryClicked() {
        onRetry()
    }
}
