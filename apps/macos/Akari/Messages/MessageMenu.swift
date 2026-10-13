import AkariKit
import AppKit

/// What a message's context menu offers.
enum MessageMenu {
    static func items(for message: Message, pasteboard: NSPasteboard = .general) -> [NSMenuItem] {
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
