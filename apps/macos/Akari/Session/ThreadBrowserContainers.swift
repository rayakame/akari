import AkariKit
import AppKit
import SwiftUI

/// Where the header's Threads button is, for the popout under it.
struct ThreadsButtonBounds: PreferenceKey {
    static let defaultValue: Anchor<CGRect>? = nil

    static func reduce(value: inout Anchor<CGRect>?, nextValue: () -> Anchor<CGRect>?) {
        value = value ?? nextValue()
    }
}

/// The header's Threads button: first in the toolbar, selected while its popout is open.
struct ThreadsButton: View {
    static let label = "Threads"

    let selected: Bool
    let toggle: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: toggle) {
            Image(systemName: ChannelSymbols.name(for: .publicThread))
                .font(.system(size: 17))
                .foregroundStyle(Color(nsColor: color))
                .frame(width: HeaderIconButton.size, height: HeaderIconButton.size)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .background(CursorArea(.pointingHand))
        .anchorPreference(key: ThreadsButtonBounds.self, value: .bounds) { $0 }
        .help(Self.label)
        .accessibilityLabel(Self.label)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private var color: NSColor {
        selected ? Palette.interactiveTextActive : hovered ? Palette.interactiveText : Palette.textMuted
    }
}

/// Where the browser's panel goes, in the session's space: the popout under the Threads button,
/// its right edge on the button's and its top under the header's line, 35% of the window wide
/// (480–600), as tall as its list (400 to 80% of the window); the modal centered, 680 wide,
/// 56 from the window's top and bottom when the list is long.
enum ThreadBrowserPlacement {
    // 8 below the button's 32 box, and past the header's line.
    static let gap: CGFloat = 9

    static func popout(button: CGRect, list: CGFloat, area: CGSize) -> CGRect {
        let width = ThreadBrowserLayout.popoutWidth(window: area.width)
        let height = ThreadBrowserLayout.popoutHeight(list: list, window: area.height)
        return CGRect(x: button.maxX - width, y: button.maxY + gap, width: width, height: height)
    }

    static func modal(list: CGFloat, area: CGSize) -> CGRect {
        let size = ThreadBrowserLayout.modalSize(list: list, window: area)
        return CGRect(
            x: ((area.width - size.width) / 2).rounded(), y: ((area.height - size.height) / 2).rounded(),
            width: size.width, height: size.height)
    }

    @MainActor
    static func frame(of browser: ThreadBrowserModel, button: CGRect?, area: CGSize) -> CGRect? {
        let list = ThreadBrowserLayout.listHeight(browser)
        switch browser.presentation {
        case .modal: return modal(list: list, area: area)
        case .popout: return button.map { popout(button: $0, list: list, area: area) }
        }
    }
}

/// The browser in its panel: the popout's or the modal's look; the modal's X, Escape or a press
/// outside it closes either.
struct ThreadBrowserPanel: View {
    let browser: ThreadBrowserModel
    let session: SessionModel
    let size: CGSize

    // The popout's and Discord's radius-md for the modal.
    static let popoutRadius: CGFloat = 8
    static let modalRadius: CGFloat = 12

    var body: some View {
        Group {
            switch browser.presentation {
            case .popout:
                ThreadBrowserView(browser: browser, session: session)
                    .frame(width: size.width, height: size.height)
                    .background(Color(nsColor: Palette.surfaceHigh))
                    .clipShape(RoundedRectangle(cornerRadius: Self.popoutRadius))
                    .overlay {
                        RoundedRectangle(cornerRadius: Self.popoutRadius)
                            .strokeBorder(Color(nsColor: Palette.shadowBorder), lineWidth: 1)
                    }
            case .modal:
                ThreadBrowserView(browser: browser, session: session)
                    .padding(.top, 8)
                    .padding(.bottom, 16)
                    .frame(width: size.width - 2, height: size.height - 2)
                    .background(Color(nsColor: Palette.surfaceHigh))
                    .clipShape(RoundedRectangle(cornerRadius: Self.modalRadius))
                    .padding(1)
                    .overlay {
                        RoundedRectangle(cornerRadius: Self.modalRadius + 1)
                            .strokeBorder(Color(nsColor: Palette.borderSubtle), lineWidth: 1)
                    }
            }
        }
        .accessibilityElement(children: .contain)
        .firstClicks()
    }
}

extension View {
    /// A panel that isn't key yet, or never is, takes its first click as a click.
    @ViewBuilder
    func firstClicks() -> some View {
        if #available(macOS 15.0, *) {
            allowsWindowActivationEvents(true)
        } else {
            self
        }
    }
}
