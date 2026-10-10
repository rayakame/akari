import AkariKit
import AppKit
import SwiftUI
import Testing

let berlin: Calendar = {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = TimeZone(identifier: "Europe/Berlin")!
    calendar.locale = Locale(identifier: "en_US")
    return calendar
}()

func berlinTime(_ year: Int, _ month: Int, _ day: Int, _ hour: Int, _ minute: Int) -> Date {
    berlin.date(
        from: DateComponents(year: year, month: month, day: day, hour: hour, minute: minute))!
}

func author(_ raw: UInt64, _ name: String = "Mira", bot: Bool = false) -> User {
    User(
        id: UserId(rawValue: raw), username: name.lowercased(), globalName: name,
        displayName: name, bot: bot, system: false)
}

func row(
    _ raw: UInt64, by user: User = author(1), at time: Date, kind: MessageType = .default,
    content: String? = nil, key: UInt64? = nil, componentsV2: Bool = false,
    attachments: [String] = [], embedCount: UInt32 = 0, stickers: [String] = []
) -> MessageListModel.Row {
    let files = attachments.enumerated().map { index, name in
        Attachment(
            id: AttachmentId(rawValue: raw * 100 + UInt64(index)), filename: name,
            contentType: nil, size: 2_000_000)
    }
    let message = Message(
        id: MessageId(rawValue: raw), channelId: ChannelId(rawValue: 10), kind: kind,
        author: user, fromWebhook: false, content: content ?? "message \(raw)", timestamp: time,
        editedTimestamp: nil, pinned: false, mentionEveryone: false, attachments: files,
        embedCount: embedCount, stickerNames: stickers, componentsV2: componentsV2,
        delivery: .sent)
    return MessageListModel.Row(id: MessageId(rawValue: key ?? raw), message: message)
}

/// A view rendered offscreen in a window with a transparent title bar, read by pixel.
@MainActor
struct Rendered {
    let rep: NSBitmapImageRep
    let scale: CGFloat

    init<Content: View>(
        _ view: Content, appearance: NSAppearance.Name = .darkAqua,
        size: NSSize = NSSize(width: 1100, height: 600)
    ) throws {
        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: size),
            styleMask: [.titled, .fullSizeContentView], backing: .buffered, defer: false)
        window.titlebarAppearsTransparent = true
        window.appearance = NSAppearance(named: appearance)
        let host = NSHostingView(rootView: view)
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        rep = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: rep)
        scale = CGFloat(rep.pixelsWide) / host.bounds.width
    }

    var width: CGFloat { CGFloat(rep.pixelsWide) / scale }

    /// The pixel at a point, measured from the top left.
    func color(_ x: CGFloat, _ y: CGFloat) -> NSColor? {
        pixel(Int(x * scale), Int(y * scale))
    }

    func pixel(_ x: Int, _ y: Int) -> NSColor? {
        rep.colorAt(x: x, y: y)?.usingColorSpace(.sRGB)
    }

    /// The first pixel row below `y` whose color at `x` differs from the color at `y`.
    func firstChange(at x: CGFloat, below y: CGFloat) -> Int? {
        let column = Int(x * scale)
        let start = Int(y * scale)
        guard let base = pixel(column, start) else {
            return nil
        }
        return (start + 1..<rep.pixelsHigh).first { !same(pixel(column, $0), base) }
    }

    /// The first pixel row above `y` whose color at `x` differs from the color at `y`.
    func firstChange(at x: CGFloat, above y: CGFloat) -> Int? {
        let column = Int(x * scale)
        let start = Int(y * scale)
        guard let base = pixel(column, start) else {
            return nil
        }
        return (0..<start).reversed().first { !same(pixel(column, $0), base) }
    }
}

// Tokens come out shifted by the bitmap's color space, so tests compare rendered colors.
func same(_ color: NSColor?, _ other: NSColor?) -> Bool {
    guard let color, let other else {
        return false
    }
    return abs(color.redComponent - other.redComponent) < 0.5 / 255
        && abs(color.greenComponent - other.greenComponent) < 0.5 / 255
        && abs(color.blueComponent - other.blueComponent) < 0.5 / 255
}
