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
        displayName: name, bot: bot, system: false, avatar: .defaultAvatar(index: UInt8(raw % 6)))
}

/// A PNG sticker named `name`.
func sticker(_ raw: UInt64, _ name: String, format: StickerFormatType = .png) -> Sticker {
    Sticker(
        id: StickerId(rawValue: raw), name: name, format: format,
        key: .sticker(stickerId: StickerId(rawValue: raw), format: format))
}

func row(
    _ raw: UInt64, by user: User = author(1), at time: Date, kind: MessageType = .default,
    content: String? = nil, key: UInt64? = nil, componentsV2: Bool = false,
    attachments: [String] = [], embedCount: UInt32 = 0, stickers: [String] = [],
    stickerFormat: StickerFormatType = .png, delivery: Delivery = .sent, edited: Date? = nil, reference: MessageReference? = nil,
    hasThread: Bool = false, fields: [EmbedField] = [],
    context: Message.NoticeContext = .init(), authorName: String? = nil,
    threadBox: ThreadBox? = nil
) -> MessageListModel.Row {
    let files = attachments.enumerated().map { index, name in
        Attachment(
            id: AttachmentId(rawValue: raw * 100 + UInt64(index)), filename: name,
            contentType: nil, size: 2_000_000)
    }
    let content = content ?? "message \(raw)"
    let message = Message(
        id: MessageId(rawValue: raw), channelId: ChannelId(rawValue: 10), kind: kind,
        author: user, fromWebhook: false, content: content, timestamp: time,
        editedTimestamp: edited, pinned: false, mentionEveryone: false, attachments: files,
        embedCount: embedCount, stickers: stickers.enumerated().map { index, name in
            sticker(raw * 10 + UInt64(index), name, format: stickerFormat)
        }, componentsV2: componentsV2,
        delivery: delivery, formatted: formattedText(content: content), mentionsMe: false,
        reference: reference, hasThread: hasThread, mentions: [],
        call: nil, roleSubscription: nil, purchasedProduct: nil, actor: nil,
        applicationName: nil, embedFields: fields)
    return MessageListModel.Row(
        id: MessageId(rawValue: key ?? raw), message: message, noticeContext: context,
        authorName: authorName, threadBox: threadBox)
}

func keyDown(_ characters: String, keyCode: UInt16, modifiers: NSEvent.ModifierFlags = [])
    -> NSEvent?
{
    NSEvent.keyEvent(
        with: .keyDown, location: .zero, modifierFlags: modifiers, timestamp: 0, windowNumber: 0,
        context: nil, characters: characters, charactersIgnoringModifiers: characters,
        isARepeat: false, keyCode: keyCode)
}

// Escape and Cmd+. send the same command, cancelOperation(_:).
func escapeKey(_ modifiers: NSEvent.ModifierFlags = []) -> NSEvent? {
    keyDown("\u{1b}", keyCode: 53, modifiers: modifiers)
}

func periodKey(_ modifiers: NSEvent.ModifierFlags) -> NSEvent? {
    keyDown(".", keyCode: 47, modifiers: modifiers)
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

/// A red like the mention badge's.
func isRed(_ color: NSColor?) -> Bool {
    guard let color else {
        return false
    }
    return color.redComponent > 0.6 && color.greenComponent < 0.35 && color.blueComponent < 0.35
}

/// Near white, like a bright name or the unread pill.
func isBright(_ color: NSColor?) -> Bool {
    guard let color else {
        return false
    }
    return min(color.redComponent, color.greenComponent, color.blueComponent) > 0.85
}

/// The pixels in the columns `xs` that pass `test`.
func count(_ rendered: Rendered, in xs: Range<CGFloat>, where test: (NSColor?) -> Bool)
    -> Int
{
    let columns = Int(xs.lowerBound * rendered.scale)..<Int(xs.upperBound * rendered.scale)
    return columns.reduce(0) { total, x in
        total + (0..<rendered.rep.pixelsHigh).filter { test(rendered.pixel(x, $0)) }.count
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

func loadReport(newConnection: Bool = false) -> LoadReport {
    LoadReport(
        outcome: .applied, queued: 0, limited: 0, headers: 0.182, newConnection: newConnection,
        body: 0.012,
        bytes: 48_213, decode: 0.0021, store: 0.0004, received: 50, retries: 0, remaining: 4,
        resetAfter: 1.2, parse: 0.0004, parsed: 50)
}

// Where `takingFirstClicks()` works.
nonisolated let firstClicks = ProcessInfo.processInfo.isOperatingSystemAtLeast(
    OperatingSystemVersion(majorVersion: 15, minorVersion: 0, patchVersion: 0))

extension View {
    // A test's app never becomes active, and SwiftUI takes a click in an inactive app's window
    // only as its activation.
    @ViewBuilder
    func takingFirstClicks() -> some View {
        if #available(macOS 15.0, *) {
            allowsWindowActivationEvents(true)
        } else {
            self
        }
    }
}
