import AppKit
import SwiftUI

// A transparent native title bar keeps dragging, zooming and the traffic lights native.
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
            window.styleMask.insert(.fullSizeContentView)
            window.backgroundColor = Palette.frame
        }
    }
}
