import AkariKit
import AppKit

/// Opens links from messages: plain ones directly, masked ones after asking (D65), unless their
/// host is trusted.
final class LinkFollower {
    struct Answer {
        let open: Bool
        let trust: Bool
    }

    private let trusted: TrustedLinks
    private let open: @MainActor (URL) -> Void
    private let ask: @MainActor (URL, @escaping (Answer) -> Void) -> Void

    init(
        trusted: TrustedLinks,
        open: @escaping @MainActor (URL) -> Void = { NSWorkspace.shared.open($0) },
        ask: @escaping @MainActor (URL, @escaping (Answer) -> Void) -> Void = LinkConfirmation.ask
    ) {
        self.trusted = trusted
        self.open = open
        self.ask = ask
    }

    func follow(_ url: URL, masked: Bool) {
        guard masked, !trusted.isTrusted(url) else {
            open(url)
            return
        }
        ask(url) { [trusted, open] answer in
            guard answer.open else {
                return
            }
            if answer.trust {
                trusted.trust(url)
            }
            open(url)
        }
    }
}

enum LinkConfirmation {
    static func alert(for url: URL) -> NSAlert {
        let alert = NSAlert()
        alert.messageText = "Open This Link?"
        alert.informativeText = "This link goes to the following website:\n\n\(url.absoluteString)"
        alert.addButton(withTitle: "Open Link")
        let back = alert.addButton(withTitle: "Back")
        back.keyEquivalent = "\u{1b}"
        alert.showsSuppressionButton = true
        alert.suppressionButton?.title =
            "Trust links to \(url.host(percentEncoded: true) ?? url.absoluteString) from now on"
        return alert
    }

    // A sheet on the window the link was clicked in.
    static func ask(_ url: URL, _ answer: @escaping (LinkFollower.Answer) -> Void) {
        let alert = alert(for: url)
        let done = { (response: NSApplication.ModalResponse) in
            answer(
                LinkFollower.Answer(
                    open: response == .alertFirstButtonReturn,
                    trust: alert.suppressionButton?.state == .on))
        }
        if let window = NSApp.keyWindow {
            alert.beginSheetModal(for: window, completionHandler: done)
        } else {
            done(alert.runModal())
        }
    }
}
