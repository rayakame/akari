import AkariKit
import AppKit
import SwiftUI

/// The split view's width, Discord's thread sidebar's (its web client, canary 645444): dragged
/// widths last until Akari quits, and each launch starts at the default (amendment 160).
@MainActor @Observable
final class SplitWidth {
    static let standard: CGFloat = 480
    static let minimum: CGFloat = 450
    /// Discord's widest split is the page less this, so the channel keeps 307 beside the gap.
    static let reserved: CGFloat = 315
    /// Between the channel and the split, where the drag handle is.
    static let gap: CGFloat = 8
    static let shared = SplitWidth()

    var width = SplitWidth.standard

    /// A page `page` wide too narrow for both shows the split over the channel.
    static func floats(in page: CGFloat) -> Bool {
        page - reserved <= minimum
    }

    func shown(in page: CGFloat) -> CGFloat {
        Self.floats(in: page) ? Self.minimum : Self.clamped(width, in: page)
    }

    static func clamped(_ width: CGFloat, in page: CGFloat) -> CGFloat {
        min(max(width, minimum), max(minimum, page - reserved))
    }

    /// The width after dragging the split's left edge by `distance`, from `start`.
    static func dragged(from start: CGFloat, by distance: CGFloat, in page: CGFloat) -> CGFloat {
        clamped(start - distance, in: page)
    }
}

/// The gap between the channel and the docked split: dragging it resizes the split. Like
/// Discord's handle it draws nothing of its own, only the cursor changes.
struct SplitGap: View {
    let split: SplitWidth
    let page: CGFloat
    @State private var start: CGFloat?

    var body: some View {
        Color(nsColor: Palette.frame)
            .frame(width: SplitWidth.gap)
            .contentShape(Rectangle())
            .onHover { inside in
                if inside {
                    NSCursor.resizeLeftRight.push()
                } else {
                    NSCursor.pop()
                }
            }
            .gesture(
                DragGesture(minimumDistance: 0, coordinateSpace: .global)
                    .onChanged { drag in
                        let base = start ?? split.shown(in: page)
                        start = base
                        split.width = SplitWidth.dragged(
                            from: base, by: drag.translation.width, in: page)
                    }
                    .onEnded { _ in start = nil }
            )
            .accessibilityHidden(true)
    }
}

/// A card's 1-wide border on its top and sides, with the top-left corner rounded.
nonisolated struct SplitCardEdge: Shape {
    static let radius: CGFloat = 12

    func path(in rect: CGRect) -> Path {
        let inset = rect.insetBy(dx: 0.5, dy: 0.5)
        var path = Path()
        path.move(to: CGPoint(x: inset.minX, y: rect.maxY))
        path.addLine(to: CGPoint(x: inset.minX, y: inset.minY + Self.radius))
        path.addArc(
            tangent1End: CGPoint(x: inset.minX, y: inset.minY),
            tangent2End: CGPoint(x: inset.minX + Self.radius, y: inset.minY),
            radius: Self.radius - 0.5)
        path.addLine(to: CGPoint(x: inset.maxX, y: inset.minY))
        path.addLine(to: CGPoint(x: inset.maxX, y: rect.maxY))
        return path
    }
}

/// What lies outside a card's rounded corner, filled with the frame's color instead of clipping
/// the card (which holds the message table).
nonisolated struct CornerCap: Shape {
    enum Corner {
        case topLeading, topTrailing
    }

    let corner: Corner

    func path(in rect: CGRect) -> Path {
        let radius = SplitCardEdge.radius
        var path = Path()
        switch corner {
        case .topLeading:
            path.move(to: CGPoint(x: rect.minX, y: rect.minY))
            path.addLine(to: CGPoint(x: rect.minX + radius, y: rect.minY))
            path.addArc(
                tangent1End: CGPoint(x: rect.minX, y: rect.minY),
                tangent2End: CGPoint(x: rect.minX, y: rect.minY + radius), radius: radius)
        case .topTrailing:
            path.move(to: CGPoint(x: rect.maxX, y: rect.minY))
            path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY + radius))
            path.addArc(
                tangent1End: CGPoint(x: rect.maxX, y: rect.minY),
                tangent2End: CGPoint(x: rect.maxX - radius, y: rect.minY), radius: radius)
        }
        path.closeSubpath()
        return path
    }
}

/// The channel's right border beside a docked split, curving into its top.
nonisolated struct ChannelCardEdge: Shape {
    func path(in rect: CGRect) -> Path {
        let x = rect.maxX - 0.5
        let top = rect.minY + 0.5
        let radius = SplitCardEdge.radius
        var path = Path()
        path.move(to: CGPoint(x: x, y: rect.maxY))
        path.addLine(to: CGPoint(x: x, y: top + radius))
        path.addArc(
            tangent1End: CGPoint(x: x, y: top), tangent2End: CGPoint(x: x - radius, y: top),
            radius: radius - 0.5)
        path.addLine(to: CGPoint(x: rect.minX, y: top))
        return path
    }
}

/// The split view's header (Discord's thread header): the thread's symbol and name, which opens
/// the thread full width, its "…" and the X.
struct SplitHeader: View {
    let thread: Channel
    var menu: ThreadMenu?
    var act: (ThreadMenu.Item) -> Void = { _ in }
    var openFullView: () -> Void = {}
    var close: () -> Void = {}

    var body: some View {
        HStack(spacing: 0) {
            Image(systemName: ChannelSymbols.name(for: thread.kind))
                .font(.system(size: 18))
                .foregroundStyle(Color(nsColor: Palette.channelDefault))
                .frame(width: 20, height: 20)
                .padding(.leading, -1)
                .padding(.trailing, 9)
                .accessibilityHidden(true)
            Button(action: openFullView) {
                FadingTitle(text: thread.name ?? "")
            }
            .buttonStyle(.plain)
            .background(PointingHandArea())
            .accessibilityLabel(thread.name ?? "")
            .padding(.trailing, 8)
            HStack(spacing: 8) {
                if let menu {
                    ThreadMenuButton(menu: menu, act: act)
                }
                HeaderIconButton(symbol: "xmark", label: "Close", action: close)
            }
            .padding(.leading, 8)
        }
        .padding(.leading, 16)
        .padding(.trailing, 8)
        .frame(height: ChannelHeader.height)
        .background(Color(nsColor: Palette.chat))
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(Color(nsColor: Palette.borderSubtle))
                .frame(height: 1)
                .offset(y: 1)
        }
        // Room for the line under it.
        .padding(.bottom, 1)
    }
}

// Discord clips a long name and fades its last 8 into the header instead of an ellipsis.
private struct FadingTitle: View {
    let text: String

    var body: some View {
        // In an overlay, so the name never sizes the header.
        Color.clear
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .overlay(alignment: .leading) {
                Text(text)
                    .font(TypeScale.channelHeader.font)
                    .foregroundStyle(Color(nsColor: Palette.textStrong))
                    .lineLimit(1)
                    .fixedSize()
            }
            .clipped()
            // A mask, not a gradient over it: nothing is drawn where there's no name.
            .mask {
                HStack(spacing: 0) {
                    Color.black
                    LinearGradient(colors: [.black, .clear], startPoint: .leading, endPoint: .trailing)
                        .frame(width: 8)
                }
            }
            .contentShape(Rectangle())
    }
}

/// A header's 32-point icon button: muted at rest, brighter while hovered, no background.
struct HeaderIconButton: View {
    static let size: CGFloat = 32

    let symbol: String
    let label: String
    let action: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(
                    Color(nsColor: hovered ? Palette.interactiveText : Palette.textMuted))
                .frame(width: Self.size, height: Self.size)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .help(label)
        .accessibilityLabel(label)
    }
}
