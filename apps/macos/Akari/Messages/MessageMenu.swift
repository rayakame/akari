import AkariKit
import AppKit

/// A message's context menu: the model's groups in their order, a separator between two.
enum MessageMenu {
    static func items(
        _ groups: [[MessageActionItem]], run: @escaping (MessageAction) -> Void
    ) -> [NSMenuItem] {
        var items: [NSMenuItem] = []
        for group in groups where !group.isEmpty {
            if !items.isEmpty {
                items.append(.separator())
            }
            for entry in group {
                let item = item(entry.title) { run(entry.action) }
                item.isEnabled = entry.enabled
                if entry.destructive {
                    item.attributedTitle = NSAttributedString(
                        string: entry.title,
                        attributes: [
                            .foregroundColor: Palette.textError,
                            .font: NSFont.menuFont(ofSize: 0),
                        ])
                }
                items.append(item)
            }
        }
        return items
    }

    /// "Copy Text" alone, where a message shows outside a list (the New Thread form).
    static func copyText(_ message: Message, pasteboard: NSPasteboard = .general) -> [NSMenuItem] {
        guard !message.content.isEmpty else {
            return []
        }
        return [
            item("Copy Text") {
                pasteboard.clearContents()
                pasteboard.setString(message.content, forType: .string)
            }
        ]
    }

    static func item(_ title: String, run: @escaping () -> Void) -> NSMenuItem {
        let action = MenuAction(run)
        let item = NSMenuItem(title: title, action: #selector(MenuAction.run(_:)), keyEquivalent: "")
        item.target = action
        // The target is weak.
        item.representedObject = action
        return item
    }
}

/// The list and row a menu item belongs to: the row stays lit while its menu is open.
struct RowMenu {
    weak var list: MessageTableController?
    let row: MessageTimeline.ItemId
}

final class MenuAction: NSObject {
    private let action: () -> Void
    var row: RowMenu?

    init(_ run: @escaping () -> Void) {
        self.action = run
    }

    @objc func run(_ sender: Any?) {
        action()
    }
}

/// A row that shows its message's menu (MessageCell, NoticeCell). A Control-click asks only the
/// view under the pointer for a menu, and a right-click asks it first, so each part of the row
/// without a menu of its own answers with the row's.
@MainActor
protocol MessageMenuRow: NSView {
    /// The message's menu for a click at the event's location; nil outside the message's box.
    func messageMenu(for event: NSEvent) -> NSMenu?
    /// The message's own area in the row: hover, the menu and the hover bar go by it.
    var box: NSRect { get }
}

extension MessageMenu {
    static func menu(_ items: [NSMenuItem]) -> NSMenu? {
        guard !items.isEmpty else {
            return nil
        }
        let menu = NSMenu()
        for item in items {
            menu.addItem(item)
        }
        return menu
    }
}

extension NSView {
    /// The menu of the message row this view is part of; nil outside one.
    func rowMessageMenu(for event: NSEvent) -> NSMenu? {
        var view = superview
        while let current = view {
            if let row = current as? MessageMenuRow {
                return row.messageMenu(for: event)
            }
            view = current.superview
        }
        return nil
    }
}

final class RowLabel: NSTextField {
    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }
}

final class RowStack: NSStackView {
    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }
}

final class RowButton: NSButton {
    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }
}

// An image view takes a right-click without showing the menu it's given: clicks go to the row.
final class RowIcon: NSImageView {
    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }
}
