import AkariKit
import AppKit

/// Opens links from messages: plain ones directly, masked ones after asking (D65), unless their
/// host is trusted. Only the schemes the parser links open.
final class LinkFollower {
    struct Answer {
        let open: Bool
        let trust: Bool
    }

    private let trusted: TrustedLinks
    private let open: @MainActor (URL) -> Void
    private let ask: @MainActor (URL, @escaping (Answer) -> Void) -> () -> Void
    private var dismiss: () -> Void = {}

    /// `ask` shows the confirmation and returns what ends it as Back would.
    init(
        trusted: TrustedLinks,
        open: @escaping @MainActor (URL) -> Void = { NSWorkspace.shared.open($0) },
        ask: @escaping @MainActor (URL, @escaping (Answer) -> Void) -> () -> Void = {
            LinkConfirmation.ask($0, in: NSApp.keyWindow, $1)
        }
    ) {
        self.trusted = trusted
        self.open = open
        self.ask = ask
    }

    // What akari-markdown makes links of; any other scheme would hand the URL to another app.
    private static let schemes: Set = ["http", "https", "discord", "steam", "mailto", "tel", "sms"]

    func follow(_ url: URL, masked: Bool) {
        guard let scheme = url.scheme?.lowercased(), Self.schemes.contains(scheme) else {
            return
        }
        guard masked, !trusted.isTrusted(url) else {
            open(url)
            return
        }
        dismiss = ask(url) { [trusted, open] answer in
            guard answer.open else {
                return
            }
            if answer.trust {
                trusted.trust(url)
            }
            open(url)
        }
    }

    /// Ends a confirmation still on screen as Back would, as when the session ends.
    func close() {
        let dismiss = self.dismiss
        self.dismiss = {}
        dismiss()
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
        // Trust is by host; mailto:, tel: and sms: have none.
        if let host = TrustedLinks.host(url) {
            alert.showsSuppressionButton = true
            alert.suppressionButton?.title = "Trust links to \(host) from now on"
        }
        return alert
    }

    // A sheet on the window the link was clicked in; what it returns ends the sheet as Back
    // would.
    static func ask(
        _ url: URL, in window: NSWindow?, _ answer: @escaping (LinkFollower.Answer) -> Void
    ) -> () -> Void {
        let alert = alert(for: url)
        let done = { (response: NSApplication.ModalResponse) in
            answer(
                LinkFollower.Answer(
                    open: response == .alertFirstButtonReturn,
                    trust: alert.showsSuppressionButton && alert.suppressionButton?.state == .on))
        }
        guard let window else {
            done(alert.runModal())
            return {}
        }
        alert.beginSheetModal(for: window, completionHandler: done)
        return { [weak window, weak alert] in
            guard let window, let sheet = alert?.window, window.attachedSheet === sheet else {
                return
            }
            window.endSheet(sheet, returnCode: .cancel)
        }
    }
}
