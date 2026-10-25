import AkariKit
import AppKit
import SwiftUI

/// The card over a hovered emoji or sticker (canary 649848): shown after 300 ms above the item,
/// gone at once when the pointer leaves it, on a click and while its popout is open. A
/// borderless child window that takes no mouse events: the list stays the pointer's authority.
@MainActor
final class ExpressionHoverCard {
    var delay: Duration = .milliseconds(300)
    var images: ImageCache?
    private(set) var shown: ExpressionCard?
    private(set) var panel: NSPanel?
    private var pending: (target: ExpressionTarget, rect: NSRect)?
    private var waiting: Task<Void, Never>?
    // Between the arrow's tip and the item, as measured in the official client.
    static let gap: CGFloat = 3

    /// The item under the pointer (its box in `window`'s coordinates), or `nil`.
    func hover(_ item: (target: ExpressionTarget, rect: NSRect)?, in window: NSWindow?) {
        if let item, let pending, pending.target == item.target, pending.rect == item.rect {
            return
        }
        hide()
        guard let item, let window else {
            return
        }
        pending = item
        waiting = Task { [weak self, delay] in
            try? await Task.sleep(for: delay)
            guard let self, !Task.isCancelled else {
                return
            }
            self.present(item, in: window)
        }
    }

    func hide() {
        waiting?.cancel()
        waiting = nil
        pending = nil
        shown = nil
        if let panel {
            panel.parent?.removeChildWindow(panel)
            panel.orderOut(nil)
        }
    }

    private func present(_ item: (target: ExpressionTarget, rect: NSRect), in window: NSWindow) {
        let card = item.target.card
        let host = NSHostingView(
            rootView: ExpressionCardView(card: card).environment(\.imageCache, images))
        let size = host.fittingSize
        let panel = self.panel ?? Self.makePanel()
        panel.contentView = host
        panel.appearance = window.effectiveAppearance
        let anchor = window.convertToScreen(item.rect)
        let bounds = window.screen?.visibleFrame ?? window.frame
        let x = min(max(anchor.midX - size.width / 2, bounds.minX + 4), bounds.maxX - size.width - 4)
        panel.setFrame(
            NSRect(x: x.rounded(), y: (anchor.maxY + Self.gap).rounded(), width: size.width, height: size.height),
            display: true)
        if panel.parent !== window {
            panel.parent?.removeChildWindow(panel)
            window.addChildWindow(panel, ordered: .above)
        }
        panel.orderFront(nil)
        self.panel = panel
        shown = card
    }

    private static func makePanel() -> NSPanel {
        let panel = NSPanel(
            contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered,
            defer: true)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.ignoresMouseEvents = true
        panel.isReleasedWhenClosed = false
        panel.animationBehavior = .none
        return panel
    }
}

/// A hover card: the emoji at 48 with its name and "Click to learn more" (272 at most), or a
/// sticker's symbol and name over it (240 at most); an arrow under it toward the item.
struct ExpressionCardView: View {
    let card: ExpressionCard
    static let arrow = CGSize(width: 16, height: 10)

    var body: some View {
        VStack(spacing: 0) {
            content
                .padding(.vertical, 8)
                .padding(.horizontal, 12)
                .background(Color(nsColor: Palette.surfaceHigh), in: RoundedRectangle(cornerRadius: 8))
                .overlay {
                    RoundedRectangle(cornerRadius: 8).strokeBorder(Color(nsColor: Palette.borderSubtle))
                }
            Arrow()
                .fill(Color(nsColor: Palette.surfaceHigh))
                .overlay { Arrow().stroke(Color(nsColor: Palette.borderSubtle), lineWidth: 1) }
                .frame(width: Self.arrow.width, height: Self.arrow.height)
                .offset(y: -1)
        }
        .fixedSize()
    }

    @ViewBuilder private var content: some View {
        switch card.picture {
        case .image, .glyph:
            HStack(spacing: 8) {
                picture
                VStack(alignment: .leading, spacing: 2) {
                    Text(card.title)
                        .font(.system(size: TypeScale.size(14), weight: .medium))
                        .foregroundStyle(Color(nsColor: Palette.textStrong))
                        .lineLimit(1)
                    Text(card.prompt)
                        .font(.system(size: TypeScale.size(14)))
                        .foregroundStyle(Color(nsColor: Palette.textMuted))
                        .lineLimit(1)
                }
            }
            .frame(maxWidth: 272 - 24, alignment: .leading)
        case .stickerSymbol:
            VStack(spacing: 2) {
                HStack(spacing: 4) {
                    Image(systemName: StickerView.symbol)
                        .font(.system(size: 13, weight: .semibold))
                        .frame(width: 16, height: 16)
                    Text(card.title)
                        .font(.system(size: TypeScale.size(14), weight: .semibold))
                        .lineLimit(1)
                }
                .foregroundStyle(Color(nsColor: Palette.textStrong))
                Text(card.prompt)
                    .font(.system(size: TypeScale.size(14)))
                    .foregroundStyle(Color(nsColor: Palette.textMuted))
                    .lineLimit(1)
            }
            // Discord's 200 with gg sans; the system font is wider, so a name that fits there
            // gets 40 more here.
            .frame(maxWidth: 240 - 24)
        }
    }

    @ViewBuilder private var picture: some View {
        switch card.picture {
        case .image(let key):
            RemoteImage(key: key, fit: .contain).frame(width: 48, height: 48)
        case .glyph(let glyph):
            Text(glyph).font(.system(size: 40)).frame(width: 48, height: 48)
        case .stickerSymbol:
            EmptyView()
        }
    }

    nonisolated private struct Arrow: Shape {
        func path(in rect: CGRect) -> Path {
            var path = Path()
            path.move(to: CGPoint(x: rect.minX, y: rect.minY))
            path.addLine(to: CGPoint(x: rect.midX, y: rect.maxY))
            path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY))
            return path
        }
    }
}
