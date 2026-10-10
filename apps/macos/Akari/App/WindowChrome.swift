import AppKit
import SwiftUI

/// Makes the native title bar transparent over full-size content, like the 32 pt title bar of
/// docs/ui/layout.md, while keeping the traffic lights, dragging and zooming native.
struct WindowChrome: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        ChromeView()
    }

    func updateNSView(_ nsView: NSView, context: Context) {}

    private final class ChromeView: NSView {
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            guard let window else {
                return
            }
            window.titlebarAppearsTransparent = true
            window.styleMask.insert(.fullSizeContentView)
            window.backgroundColor = Palette.frame
        }
    }
}
