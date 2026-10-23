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

    private static func item(_ title: String, run: @escaping () -> Void) -> NSMenuItem {
        let action = MenuAction(run)
        let item = NSMenuItem(title: title, action: #selector(MenuAction.run(_:)), keyEquivalent: "")
        item.target = action
        // The target is weak.
        item.representedObject = action
        return item
    }
}

private final class MenuAction: NSObject {
    private let action: () -> Void

    init(_ run: @escaping () -> Void) {
        self.action = run
    }

    @objc func run(_ sender: Any?) {
        action()
    }
}
