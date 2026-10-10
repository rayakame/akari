import AppKit

// The first row, and the last while detached: a spinner while a page loads, the error with a
// way to try again, or where the channel begins. One height in every state, so nothing moves.
final class EdgeCell: NSTableCellView {
    enum Look: Equatable {
        case idle
        case loading
        case failed(String)
        case beginning(String)
    }

    static let identifier = NSUserInterfaceItemIdentifier("edge")
    static let height: CGFloat = 48

    var onRetry: () -> Void = {}
    private let spinner = NSProgressIndicator()
    private let label = CellText.label()
    private let retry = NSButton(title: "Try again", target: nil, action: nil)

    init() {
        super.init(frame: .zero)
        identifier = Self.identifier
        spinner.style = .spinning
        spinner.controlSize = .small
        spinner.isDisplayedWhenStopped = false
        label.font = .systemFont(ofSize: 14)
        label.textColor = Palette.textMuted
        retry.isBordered = false
        retry.attributedTitle = NSAttributedString(
            string: "Try again",
            attributes: [.font: NSFont.systemFont(ofSize: 14), .foregroundColor: Palette.textLink])
        retry.target = self
        retry.action = #selector(retryClicked)
        let row = NSStackView(views: [spinner, label, retry])
        row.orientation = .horizontal
        row.spacing = 8
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(row)
        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: Self.height),
            row.centerXAnchor.constraint(equalTo: centerXAnchor),
            row.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
        show(.idle)
    }

    required init?(coder: NSCoder) {
        nil
    }

    func show(_ look: Look) {
        switch look {
        case .loading:
            spinner.startAnimation(nil)
        default:
            spinner.stopAnimation(nil)
        }
        spinner.isHidden = look != .loading
        switch look {
        case .failed(let text), .beginning(let text):
            label.stringValue = text
            label.isHidden = false
        default:
            label.stringValue = ""
            label.isHidden = true
        }
        if case .failed = look {
            retry.isHidden = false
        } else {
            retry.isHidden = true
        }
    }

    @objc private func retryClicked() {
        onRetry()
    }
}
