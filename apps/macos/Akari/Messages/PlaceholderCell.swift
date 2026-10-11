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
    // Plain colored layers: a drawn bitmap of a block this tall was most of the launch's memory.
    private let shapes = CALayer()
    var color = Palette.hoverBackground {
        didSet { layOutShapes() }
    }

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
        // The table sizes the row; while it moves row views during an update, a required
        // height would conflict with a row of another height and log at length.
        height.priority = .init(999)
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
        wantsLayer = true
        layer?.insertSublayer(shapes, at: 0)
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
        needsLayout = true
    }

    override func layout() {
        super.layout()
        layOutShapes()
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        layOutShapes()
    }

    private func layOutShapes() {
        let band =
            !failed
            ? nil
            : edge == .newer
                ? NSRect(x: 0, y: 0, width: bounds.width, height: Self.band)
                : NSRect(x: 0, y: bounds.height - Self.band, width: bounds.width, height: Self.band)
        let text = max(bounds.width - 72 - 16, 40)
        var frames: [(NSRect, CGFloat)] = []
        var y: CGFloat = 0
        var index = 0
        while y < bounds.height {
            let height = Self.rows[index % Self.rows.count]
            let width = { (offset: Int) in text * Self.widths[(index + offset) % Self.widths.count]
            }
            if height >= 66 {
                frames.append((NSRect(x: 16, y: y + 18, width: 40, height: 40), 20))
                frames.append((bar(x: 72, y: y + 20, width: min(width(0), 160)), 6))
                for line in 0..<Int((height - 44) / 22) {
                    frames.append(
                        (bar(x: 72, y: y + 42 + CGFloat(line) * 22, width: width(line + 1)), 6))
                }
            } else {
                for line in 0..<max(1, Int(height / 22)) {
                    frames.append(
                        (bar(x: 72, y: y + 7 + CGFloat(line) * 22, width: width(line)), 6))
                }
            }
            y += height
            index += 1
        }
        frames.removeAll { frame, _ in band.map { $0.intersects(frame) } ?? false }
        var resolved: CGColor?
        effectiveAppearance.performAsCurrentDrawingAppearance {
            resolved = color.cgColor
        }
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        shapes.frame = bounds
        shapes.isGeometryFlipped = true
        var sublayers = shapes.sublayers ?? []
        while sublayers.count < frames.count {
            let shape = CALayer()
            shapes.addSublayer(shape)
            sublayers.append(shape)
        }
        for (shape, placed) in zip(
            sublayers,
            frames + Array(repeating: (.zero, 0), count: max(0, sublayers.count - frames.count)))
        {
            shape.frame = placed.0
            shape.cornerRadius = placed.1
            shape.backgroundColor = resolved
            shape.isHidden = placed.0.isEmpty
        }
        CATransaction.commit()
    }

    private func bar(x: CGFloat, y: CGFloat, width: CGFloat) -> NSRect {
        NSRect(x: x, y: y, width: width, height: 12)
    }

    @objc private func retryClicked() {
        onRetry()
    }
}
