import AkariKit
import AppKit
import SwiftUI

/// A clicked emoji's or sticker's popout: a panel to the right of the item, centered on it, in
/// the session window's `ChildWindows` (a press outside or Escape closes it), as tall as its
/// content as that loads.
@MainActor
final class ExpressionPopoutPresenter {
    let windows = ChildWindows(keyable: true)
    private(set) var model: ExpressionPopoutModel?
    var openGuild: (GuildId) -> Void = { _ in }
    private var anchor = NSRect.zero
    private weak var window: NSWindow?
    private var images: ImageCache?
    private var loading: Task<Void, Never>?

    init() {
        windows.label = "Emoji or sticker"
        windows.onClose = { [weak self] in self?.close() }
    }

    /// `anchor` is the item's box in `window`'s coordinates.
    func show(_ model: ExpressionPopoutModel, anchor: NSRect, in window: NSWindow, images: ImageCache?) {
        loading?.cancel()
        self.model = model
        self.anchor = anchor
        self.window = window
        self.images = images
        place()
        loading = Task { [weak self] in
            await model.load()
            guard let self, self.model === model else {
                return
            }
            self.place()
        }
    }

    /// The server row of a server the user is in: opens it.
    func openServer(_ id: GuildId) {
        close()
        openGuild(id)
    }

    func close() {
        loading?.cancel()
        loading = nil
        model = nil
        windows.tearDown(refocus: true)
    }

    func relayout() {
        place()
    }

    private func place() {
        guard let model, let window else {
            return
        }
        let view = ExpressionPopoutView(
            model: model, openServer: { [weak self] in self?.openServer($0) },
            resized: { [weak self] in self?.relayout() }
        )
        .environment(\.imageCache, images)
        let measure = NSHostingController(rootView: view)
        let height = measure.sizeThatFits(
            in: CGSize(width: ExpressionPopoutView.width, height: CGFloat.greatestFiniteMagnitude)
        ).height.rounded(.up)
        let item = window.convertToScreen(anchor)
        let frame = window.frame
        let size = NSSize(width: ExpressionPopoutView.width, height: height)
        var origin = NSPoint(x: item.maxX + 8, y: item.midY - size.height / 2)
        if origin.x + size.width > frame.maxX - 8 {
            origin.x = item.minX - 8 - size.width
        }
        origin.y = min(max(origin.y, frame.minY + 8), frame.maxY - size.height - 8)
        windows.show(
            AnyView(view), at: NSRect(origin: origin, size: size).integral, in: window, dimmed: false)
    }
}

/// The popout's content (canary 649848's layout): the emoji or sticker, its name and what it
/// is; the server it's from on a darker band, with "Show More".
struct ExpressionPopoutView: View {
    static let width: CGFloat = 288
    let model: ExpressionPopoutModel
    let openServer: (GuildId) -> Void
    let resized: () -> Void
    @State private var expanded = false

    private var content: ExpressionPopout { model.content }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            top
            if let server = content.server {
                section(server)
            }
        }
        .frame(width: Self.width, alignment: .leading)
        .background(Color(nsColor: Palette.surfaceHigh))
        .clipShape(RoundedRectangle(cornerRadius: 8))
        .overlay { RoundedRectangle(cornerRadius: 8).strokeBorder(Color(nsColor: Palette.borderSubtle)) }
        .onChange(of: expanded) { resized() }
    }

    private var top: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let picture = content.picture {
                pictureView(picture)
                    .padding(.top, 4)
                    .padding(.bottom, 8)
            }
            Text(content.title)
                .font(.system(size: TypeScale.size(16), weight: .semibold))
                .foregroundStyle(Color(nsColor: Palette.textStrong))
                .lineLimit(2)
            if !content.body.isEmpty {
                body(content.body)
                    .font(.system(size: TypeScale.size(14)))
                    .foregroundStyle(Color(nsColor: Palette.textDefault))
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 4)
            }
            if content.loading {
                ProgressView().controlSize(.small).padding(.top, 8)
            }
            if !content.others.isEmpty {
                HStack(spacing: 8) {
                    ForEach(content.others, id: \.id) { sticker in
                        stickerImage(sticker, size: 80)
                    }
                }
                .padding(.top, 12)
            }
        }
        .padding(16)
    }

    private func section(_ server: ExpressionPopout.Server) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(server.heading)
                .font(.system(size: TypeScale.size(14), weight: .medium))
                .foregroundStyle(Color(nsColor: Palette.textMuted))
            HStack(spacing: 12) {
                ServerPicture(name: server.name, icon: server.icon, size: 32, cornerRadius: 8)
                VStack(alignment: .leading, spacing: 2) {
                    HStack(spacing: 4) {
                        if let badge = server.badge {
                            Image(systemName: badge == .verified ? "checkmark.seal.fill" : "star.circle.fill")
                                .foregroundStyle(Color(nsColor: Palette.brand))
                        }
                        Text(server.name)
                            .font(.system(size: TypeScale.size(16), weight: .semibold))
                            .foregroundStyle(Color(nsColor: Palette.textStrong))
                            .lineLimit(1)
                    }
                    Text(server.lineText)
                        .font(.system(size: TypeScale.size(12)))
                        .foregroundStyle(Color(nsColor: Palette.textMuted))
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
            }
            .contentShape(Rectangle())
            .background(server.opens ? CursorArea(.pointingHand) : nil)
            .onTapGesture {
                if server.opens {
                    openServer(server.id)
                }
            }
            .accessibilityAddTraits(server.opens ? .isButton : [])
            if let label = content.moreLabel, let more = content.more {
                Button {
                    expanded.toggle()
                } label: {
                    HStack(spacing: 4) {
                        Text(label)
                        Image(systemName: "chevron.down")
                            .rotationEffect(.degrees(expanded ? 0 : -90))
                    }
                    .font(.system(size: TypeScale.size(14), weight: .medium))
                    .foregroundStyle(Color(nsColor: Palette.textMuted))
                }
                .buttonStyle(.plain)
                .background(CursorArea(.pointingHand))
                if expanded {
                    moreGrid(more)
                }
            }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: Palette.frame))
    }

    private func moreGrid(_ more: ExpressionPopout.More) -> some View {
        LazyVGrid(columns: Array(repeating: GridItem(.fixed(32), spacing: 8), count: 6), spacing: 8) {
            switch more {
            case .emoji(let emojis):
                ForEach(emojis, id: \.id) { emoji in
                    RemoteImage(key: emoji.key, fit: .contain)
                        .frame(width: 32, height: 32)
                        .help(":\(emoji.name):")
                }
            case .stickers(let stickers):
                ForEach(stickers, id: \.id) { sticker in
                    stickerImage(sticker, size: 32).help(sticker.name)
                }
            }
        }
    }

    @ViewBuilder private func pictureView(_ picture: ExpressionPopout.Picture) -> some View {
        switch picture {
        case .emoji(let key):
            RemoteImage(key: key, fit: .contain).frame(width: 48, height: 48)
        case .glyph(let glyph):
            Text(glyph).font(.system(size: 40)).frame(width: 48, height: 48)
        case .sticker(let sticker):
            stickerImage(sticker, size: 48)
        }
    }

    @ViewBuilder private func stickerImage(_ sticker: Sticker, size: CGFloat) -> some View {
        if sticker.format == .lottie {
            Image(systemName: StickerView.symbol)
                .font(.system(size: size * 0.5))
                .foregroundStyle(Color(nsColor: Palette.textMuted))
                .frame(width: size, height: size)
        } else {
            RemoteImage(key: sticker.key, fit: .contain).frame(width: size, height: size)
        }
    }

    private func body(_ runs: [ExpressionPopout.Run]) -> Text {
        runs.reduce(Text("")) { text, run in
            text + Text(run.text).fontWeight(run.bold ? .bold : nil)
        }
    }
}
