import AkariKit
import AppKit
import SwiftUI

// From Discord's stylesheet (canary 671549).
enum PinsLayout {
    static let width: CGFloat = 420
    static let minHeight: CGFloat = 200
    static let maxShare: CGFloat = 0.8
    static let radius: CGFloat = 8
    static let inset: CGFloat = 16
    static let iconBox: CGFloat = 24
    static let titleGap: CGFloat = 8
    // 16 around the title's 24 line, and the line under it.
    static let headerHeight: CGFloat = 2 * inset + 24 + 1
    static let listTop: CGFloat = 8
    static let listLeading: CGFloat = 8
    static let cardTrailing: CGFloat = 4
    static let cardGap: CGFloat = 8
    static let cardRadius: CGFloat = 8
    static let cardWidth = width - listLeading - cardTrailing
    // The message is 16 inside the card above and below and starts 4 left of the card's padding.
    static let messageInset: CGFloat = 16
    static let messageShift: CGFloat = 4
    // The list's 2 above a row, which the card's 16 replaces: the avatar 18 inside, the name 16.
    static let rowTop: CGFloat = 2
    // Discord's message keeps 24 on its right, the row 16.
    static let previewWidth = cardWidth - 2 + messageShift - (24 - 16)
    static let buttonsInset: CGFloat = 8
    static let buttonsGap: CGFloat = 8
    static let buttonHeight: CGFloat = 32
    static let moreTop: CGFloat = 8
    static let moreInset: CGFloat = 16
    static let retryTop: CGFloat = 16
    static let spinner: CGFloat = 32
    static let spinnerMargin: CGFloat = 32
    static let firstSpinnerTop: CGFloat = 100
    static let firstSpinnerBottom: CGFloat = 120
    static let emptyTop: CGFloat = 8
    static let emptyArt: CGFloat = 120
    static let emptyTextGap: CGFloat = 20
    static let emptyBottom: CGFloat = 32
    static let footerInset: CGFloat = 16
    static let protipInset: CGFloat = 10
    // Discord asks for the next page within 250 of the list's bottom.
    static let loadDistance: CGFloat = 250

    static func frame(button: CGRect, content: CGFloat, area: CGSize) -> CGRect {
        CGRect(
            x: button.maxX - width, y: button.maxY + ThreadBrowserPlacement.gap, width: width,
            height: height(content: content, window: area.height))
    }

    static func height(content: CGFloat, window: CGFloat) -> CGFloat {
        max(minHeight, min(headerHeight + content, (window * maxShare).rounded(.down)))
    }

    @MainActor
    static func frame(of pins: PinsModel, button: CGRect, area: CGSize) -> CGRect {
        frame(button: button, content: contentHeight(pins, upTo: area.height), area: area)
    }

    @MainActor
    static func contentHeight(_ pins: PinsModel, upTo limit: CGFloat) -> CGFloat {
        switch pins.phase {
        case .loading:
            return firstSpinnerTop + spinner + firstSpinnerBottom
        case .failed:
            return emptyHeight(PinsModel.failedText) + retryTop + buttonHeight + emptyBottom
        case .empty:
            return emptyHeight(pins.emptyText) + emptyBottom + footerHeight(pins)
        case .list:
            var height = listTop
            for row in pins.rows where height <= limit {
                height += cardHeight(row) + cardGap
            }
            // The last card's gap is the space before the spinner or the button.
            if pins.loadingMore {
                height += 2 * spinnerMargin + spinner - cardGap
            } else if pins.offersMore {
                height += moreTop + buttonHeight + moreInset - cardGap
            }
            return height
        }
    }

    @MainActor
    static func previewHeight(_ row: MessageListModel.Row) -> CGFloat {
        MessagePreview.height(of: row.shown, reply: row.reply, spaced: false, width: previewWidth)
    }

    @MainActor
    static func cardHeight(_ row: MessageListModel.Row) -> CGFloat {
        2 + messageInset - rowTop + previewHeight(row) + messageInset
    }

    static func emptyHeight(_ text: String) -> CGFloat {
        let lines = CGFloat(text.split(separator: "\n").count)
        return emptyTop + emptyArt + emptyTextGap + lines * TypeScale.pinsEmpty.line
    }

    @MainActor
    static func footerHeight(_ pins: PinsModel) -> CGFloat {
        let width = width - 2 * footerInset
        let text = PinsFooter.text(pins.protip)
        let height = text.boundingRect(
            with: NSSize(width: width, height: .greatestFiniteMagnitude),
            options: [.usesLineFragmentOrigin, .usesFontLeading]
        ).height
        return 2 * (footerInset + protipInset) + height.rounded(.up)
    }
}

struct PinsButtonBounds: PreferenceKey {
    static let defaultValue: Anchor<CGRect>? = nil

    static func reduce(value: inout Anchor<CGRect>?, nextValue: () -> Anchor<CGRect>?) {
        value = value ?? nextValue()
    }
}

struct PinsButton: View {
    static let symbol = "pin.fill"
    static let dot: CGFloat = 8
    // The header's color around the dot, so it stands clear of the pin.
    static let dotRing: CGFloat = 2

    let selected: Bool
    let unread: Bool
    var disabled = false
    let toggle: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: toggle) {
            Image(systemName: Self.symbol)
                .font(.system(size: 17))
                .foregroundStyle(Color(nsColor: color))
                .frame(width: 20, height: 20)
                .overlay(alignment: .bottomTrailing) {
                    if unread {
                        Circle()
                            .fill(Color(nsColor: Palette.danger))
                            .frame(width: Self.dot, height: Self.dot)
                            .padding(Self.dotRing)
                            .background(Circle().fill(Color(nsColor: Palette.chat)))
                            .offset(x: Self.dotRing + 1, y: Self.dotRing + 1)
                            .accessibilityHidden(true)
                    }
                }
                .frame(width: HeaderIconButton.size, height: HeaderIconButton.size)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(disabled)
        .opacity(disabled ? 0.5 : 1)
        .onHover { hovered = $0 }
        .background(CursorArea(disabled ? .arrow : .pointingHand))
        .anchorPreference(key: PinsButtonBounds.self, value: .bounds) { $0 }
        .help(selected ? "" : PinsModel.title)
        .accessibilityLabel(PinsModel.title)
        .accessibilityValue(unread ? "Unread" : "")
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private var color: NSColor {
        selected ? Palette.interactiveTextActive : hovered && !disabled ? Palette.interactiveText : Palette.textMuted
    }
}

struct PinsPanel: View {
    let pins: PinsModel
    let size: CGSize
    /// Fixed for tests; otherwise the pointer's.
    var hovered: MessageId?

    var body: some View {
        PinsPopoutView(pins: pins, hovered: hovered)
            .frame(width: size.width, height: size.height)
            .background(Color(nsColor: Palette.surfaceHigh))
            .clipShape(RoundedRectangle(cornerRadius: PinsLayout.radius))
            .overlay {
                RoundedRectangle(cornerRadius: PinsLayout.radius)
                    .strokeBorder(Color(nsColor: Palette.shadowBorder), lineWidth: 1)
            }
            .accessibilityElement(children: .contain)
            .firstClicks()
            .task(id: ObjectIdentifier(pins)) { await pins.open() }
    }
}

struct PinsPopoutView: View {
    let pins: PinsModel
    var hovered: MessageId?

    var body: some View {
        VStack(spacing: 0) {
            PinsHeader()
            content
                .frame(maxWidth: .infinity, minHeight: 0, maxHeight: .infinity, alignment: .top)
                .clipped()
        }
    }

    @ViewBuilder private var content: some View {
        switch pins.phase {
        case .loading:
            ProgressView()
                .controlSize(.regular)
                .frame(width: PinsLayout.spinner, height: PinsLayout.spinner)
                .padding(.top, PinsLayout.firstSpinnerTop)
                .padding(.bottom, PinsLayout.firstSpinnerBottom)
                .frame(maxWidth: .infinity)
        case .failed:
            VStack(spacing: 0) {
                PinsEmptyState(text: PinsModel.failedText)
                BrandButton(title: PinsModel.retry, size: .small, secondary: true) {
                    Task { await pins.retry() }
                }
                .padding(.top, PinsLayout.retryTop)
                .padding(.bottom, PinsLayout.emptyBottom)
            }
            .frame(maxWidth: .infinity)
        case .empty:
            VStack(spacing: 0) {
                PinsEmptyState(text: pins.emptyText)
                    .padding(.bottom, PinsLayout.emptyBottom)
                Spacer(minLength: 0)
                PinsFooter(text: pins.protip)
            }
        case .list:
            PinsList(pins: pins, hovered: hovered)
        }
    }
}

struct PinsHeader: View {
    var body: some View {
        HStack(spacing: PinsLayout.titleGap) {
            Image(systemName: PinsButton.symbol)
                .font(.system(size: 18))
                .foregroundStyle(Color(nsColor: Palette.interactiveText))
                .frame(width: PinsLayout.iconBox, height: PinsLayout.iconBox)
                .accessibilityHidden(true)
            Text(PinsModel.title)
                .font(TypeScale.popoutTitle.font)
                .foregroundStyle(Color(nsColor: Palette.textStrong))
                .lineLimit(1)
                .accessibilityAddTraits(.isHeader)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, PinsLayout.inset)
        .frame(height: PinsLayout.headerHeight - 1)
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(Color(nsColor: Palette.borderSubtle))
                .frame(height: 1)
                .offset(y: 1)
        }
        .padding(.bottom, 1)
    }
}

struct PinsList: View {
    let pins: PinsModel
    var hovered: MessageId?

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: PinsLayout.cardGap) {
                ForEach(pins.rows) { row in
                    PinnedCard(
                        row: row, unpins: pins.canUnpin(row), busy: pins.unpinning.contains(row.message.id),
                        hoveredLook: hovered.map { $0 == row.id },
                        jump: { shift in pins.jump(to: row, shift: shift) },
                        unpin: { shift in pins.unpin(row, shift: shift) })
                }
                if pins.loadingMore {
                    ProgressView()
                        .controlSize(.regular)
                        .frame(width: PinsLayout.spinner, height: PinsLayout.spinner)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, PinsLayout.spinnerMargin - PinsLayout.cardGap)
                } else if pins.offersMore {
                    BrandButton(title: PinsModel.loadMore, size: .small, secondary: true, fills: true) {
                        Task { await pins.loadMore() }
                    }
                    .padding(.horizontal, PinsLayout.moreInset)
                    .padding(.top, PinsLayout.moreTop - PinsLayout.cardGap)
                    .padding(.bottom, PinsLayout.moreInset - PinsLayout.cardGap)
                }
            }
            .padding(.top, PinsLayout.listTop)
            .padding(.bottom, PinsLayout.cardGap)
            .padding(.leading, PinsLayout.listLeading)
            .padding(.trailing, PinsLayout.cardTrailing)
            .background {
                ScrollEnd(distance: PinsLayout.loadDistance) {
                    Task { await pins.reachedEnd() }
                }
            }
        }
    }
}

extension EnvironmentValues {
    // The tests hold Shift through it.
    @Entry var clickModifiers: @MainActor () -> NSEvent.ModifierFlags = { NSEvent.modifierFlags }
}

struct PinnedCard: View {
    let row: MessageListModel.Row
    let unpins: Bool
    let busy: Bool
    /// Fixed for tests; otherwise the pointer's.
    var hoveredLook: Bool?
    let jump: (_ shift: Bool) -> Void
    let unpin: (_ shift: Bool) -> Void
    @State private var hovered = false
    @Environment(\.clickModifiers) private var modifiers

    var body: some View {
        ZStack(alignment: .topLeading) {
            PinnedPreview(row: row)
                .frame(width: PinsLayout.previewWidth, height: PinsLayout.previewHeight(row))
                .offset(x: 1 - PinsLayout.messageShift, y: 1 + PinsLayout.messageInset - PinsLayout.rowTop)
            if hoveredLook ?? hovered {
                buttons
                    .frame(width: PinsLayout.cardWidth - 2 - PinsLayout.buttonsInset, alignment: .trailing)
                    .padding(.top, 1 + PinsLayout.buttonsInset)
                    .padding(.leading, 1)
            }
        }
        .frame(width: PinsLayout.cardWidth, height: PinsLayout.cardHeight(row), alignment: .topLeading)
        .background(Color(nsColor: Palette.surfaceHigher))
        .clipShape(RoundedRectangle(cornerRadius: PinsLayout.cardRadius))
        .overlay {
            RoundedRectangle(cornerRadius: PinsLayout.cardRadius)
                .strokeBorder(Color(nsColor: Palette.borderSubtle), lineWidth: 1)
        }
        .contentShape(RoundedRectangle(cornerRadius: PinsLayout.cardRadius))
        .onHover { hovered = $0 }
        .accessibilityElement(children: .contain)
        .accessibilityAction(named: PinsModel.jump) { jump(false) }
        .accessibilityActions {
            if unpins && !busy {
                Button(PinsModel.unpin) { unpin(false) }
            }
        }
    }

    private var buttons: some View {
        HStack(spacing: PinsLayout.buttonsGap) {
            BrandButton(title: PinsModel.jump, size: .small, secondary: true) {
                jump(modifiers().contains(.shift))
            }
            .background(Color(nsColor: Palette.surfaceHigh), in: RoundedRectangle(cornerRadius: 8))
            if unpins {
                UnpinButton(busy: busy) {
                    unpin(modifiers().contains(.shift))
                }
                .background(Color(nsColor: Palette.surfaceHigh), in: RoundedRectangle(cornerRadius: 8))
            }
        }
    }
}

struct UnpinButton: View {
    let busy: Bool
    let action: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            Group {
                if busy {
                    ProgressView().controlSize(.small)
                } else {
                    Image(systemName: "xmark")
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(Color(nsColor: Palette.textStrong))
                }
            }
            .frame(width: PinsLayout.buttonHeight, height: PinsLayout.buttonHeight)
            .background(
                Color(nsColor: hovered && !busy ? Palette.backgroundModStrong : Palette.backgroundModNormal),
                in: RoundedRectangle(cornerRadius: 8))
            .overlay {
                RoundedRectangle(cornerRadius: 8).strokeBorder(Color.white.opacity(0.08), lineWidth: 1)
            }
            .contentShape(RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .disabled(busy)
        .onHover { hovered = $0 }
        .background(CursorArea(busy ? .arrow : .pointingHand))
        .help(PinsModel.unpin)
        .accessibilityLabel(PinsModel.unpin)
    }
}

// Links, spoilers and media open from the list, after Jump.
struct PinnedPreview: NSViewRepresentable {
    let row: MessageListModel.Row
    var now = Date()
    @Environment(\.imageCache) private var images

    func makeNSView(context: Context) -> MessagePreviewView {
        MessagePreviewView(interactive: false)
    }

    func updateNSView(_ view: MessagePreviewView, context: Context) {
        view.cell.images = images
        MessagePreview.configure(
            view.cell, with: row.shown, reply: row.reply, authorName: row.authorName, menu: false,
            spaced: false, now: now)
    }
}

// Discord's illustration is a bundled picture; a symbol stands in.
struct PinsEmptyState: View {
    let text: String

    var body: some View {
        VStack(spacing: 0) {
            Image(systemName: "pin")
                .font(.system(size: 56, weight: .light))
                .foregroundStyle(Color(nsColor: Palette.textMuted))
                .frame(width: PinsLayout.emptyArt, height: PinsLayout.emptyArt)
                .accessibilityHidden(true)
            Text(text)
                .font(TypeScale.pinsEmpty.font)
                .foregroundStyle(Color(nsColor: Palette.textDefault))
                .multilineTextAlignment(.center)
                .lineSpacing(TypeScale.pinsEmpty.line - TypeScale.pinsEmpty.nsFont.lineHeight)
                .fixedSize(horizontal: false, vertical: true)
                .padding(.top, PinsLayout.emptyTextGap)
        }
        .padding(.top, PinsLayout.emptyTop)
        .frame(maxWidth: .infinity)
    }
}

struct PinsFooter: View {
    let text: String

    var body: some View {
        (Text(PinsModel.protipLabel + " ")
            .font(TypeScale.protip.font)
            .foregroundStyle(Color(nsColor: Palette.statusPositive))
            + Text(text)
            .font(TypeScale.protipText.font)
            .foregroundStyle(Color(nsColor: Palette.textMuted)))
            .multilineTextAlignment(.center)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity)
            .padding(.vertical, PinsLayout.protipInset)
            .padding(PinsLayout.footerInset)
            .background(Color(nsColor: Palette.hoverBackground))
    }

    // As the view draws it, for the popout's height.
    static func text(_ words: String) -> NSAttributedString {
        let text = NSMutableAttributedString(
            string: PinsModel.protipLabel + " ",
            attributes: [.font: TypeScale.protip.nsFont, .foregroundColor: Palette.statusPositive])
        text.append(
            NSAttributedString(
                string: words,
                attributes: [.font: TypeScale.protipText.nsFont, .foregroundColor: Palette.textMuted]))
        return text
    }
}

extension NSFont {
    fileprivate var lineHeight: CGFloat {
        (ascender - descender + leading).rounded(.up)
    }
}
