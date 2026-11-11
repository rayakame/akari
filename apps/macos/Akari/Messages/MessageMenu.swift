import AkariKit
import AppKit

/// What a message's context menu offers, in the official client's order: "Create Thread" closes
/// the Reply and Forward group, "Copy Text" opens the next.
enum MessageMenu {
    static func items(
        for message: Message, createThread: (() -> Void)? = nil,
        pasteboard: NSPasteboard = .general
    ) -> [NSMenuItem] {
        var items: [NSMenuItem] = []
        if let createThread {
            items.append(item("Create Thread", run: createThread))
        }
        if !message.content.isEmpty {
            if !items.isEmpty {
                items.append(.separator())
            }
            items.append(
                item("Copy Text") {
                    pasteboard.clearContents()
                    pasteboard.setString(message.content, forType: .string)
                })
        }
        return items
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

final class MenuAction: NSObject {
    private let action: () -> Void

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
