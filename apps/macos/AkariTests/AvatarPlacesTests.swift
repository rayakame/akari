import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// Every SwiftUI place of D137 shows its image through one layer view, at its size, with its
// motion rule; users never show initials, servers without an icon keep theirs.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class AvatarPlacesTests {
    // The windows stay for the test: a released window takes its views' asks with it.
    var windows: [NSWindow] = []
    let images = TestImages()

    func layers<Content: View>(in view: Content, size: NSSize = NSSize(width: 300, height: 100))
        -> [ImageLayerView]
    {
        let host = NSHostingView(rootView: view)
        let window = TestWindow.make(size: size)
        windows.append(window)
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        return MessageMenuAreaTests.views(ImageLayerView.self, in: host)
    }

    func ray(_ avatar: ImageKey = .avatar(2)) -> User {
        User(
            id: UserId(rawValue: 2), username: "ray", globalName: "Ray", displayName: "Ray",
            bot: false, system: false, avatar: avatar)
    }

    @Test
    func aUserAvatarIsRoundAtItsSizeWithTheDisc() async throws {
        let source = FakeImageSource.serving(try images.png("ray", pixels: 256), pixels: 256)
        let cache = ImageCache(source: source)
        let layer = try #require(
            layers(in: UserAvatar(ray(), size: 32).environment(\.imageCache, cache)).first)

        #expect(layer.content?.key == .avatar(2))
        #expect(layer.cornerRadius == 16)
        #expect(layer.placeholder == .disc)
        #expect(layer.frame.size == NSSize(width: 32, height: 32))
        try await eventually { layer.imageLayer.contents != nil }
        let scale = layer.window?.backingScaleFactor ?? 0
        #expect(source.requests.withLock { $0.first }?.width == UInt32((32 * scale).rounded(.up)))
        #expect(cache.isPinned(ImageSpec(key: .avatar(2), width: Int(32 * scale), height: Int(32 * scale))))
    }

    @Test
    func aUserWithoutAnAvatarShowsTheDefaultAvatarNotInitials() throws {
        let layer = try #require(layers(in: UserAvatar(ray(.defaultAvatar(index: 4)), size: 40)).first)

        #expect(layer.content?.key == .defaultAvatar(index: 4))
    }

    @Test
    func aGuildIconShowsItsImageOverTheInitialsOnlyWithAnIcon() throws {
        let icon = ImageKey.guildIcon(guildId: GuildId(rawValue: 1), hash: "0123456789abcdef0123456789abcdef")
        let withIcon = layers(in: GuildIcon(name: "Lab", icon: icon, highlighted: false))
        let layer = try #require(withIcon.first)

        #expect(layer.content?.key == icon)
        #expect(layer.cornerRadius == RailShape.cornerRadius)
        #expect(layer.placeholder == .none, "the initials show until the image decoded")
        #expect(layer.frame.size == NSSize(width: 40, height: 40))
        #expect(layers(in: GuildIcon(name: "Lab", icon: nil, highlighted: false)).isEmpty)
    }

    // A server icon over a known background with an 8 point margin around it, at any backing
    // scale; the pixel at the icon's center and one in the margin, once the icon's layer settled.
    func serverIcon<Content: View>(
        _ view: Content, source: FakeImageSource, size: CGFloat = 72, settle: Bool = true,
        at point: NSPoint? = nil, scale: CGFloat? = nil
    ) async throws -> (center: NSColor?, outside: NSColor?) {
        let cache = ImageCache(source: source)
        let host = NSHostingView(
            rootView: view.frame(width: size, height: size)
                .frame(width: size + 2 * Self.margin, height: size + 2 * Self.margin)
                .background(Color(nsColor: Self.backdrop))
                .environment(\.imageCache, cache))
        let side = size + 2 * Self.margin
        let window = scale.map { ScaledWindow.make(scale: $0) as NSWindow }
            ?? TestWindow.make(size: NSSize(width: side, height: side))
        window.setContentSize(NSSize(width: side, height: side))
        windows.append(window)
        window.appearance = NSAppearance(named: .darkAqua)
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        let layers = MessageMenuAreaTests.views(ImageLayerView.self, in: host)
        if settle {
            try await eventually { layers.allSatisfy { $0.imageLayer.contents != nil || $0.failed } }
            try await Task.sleep(for: .milliseconds(30))
            host.layoutSubtreeIfNeeded()
        }
        let rep = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / host.bounds.width
        let center = point.map { (Int(($0.x + Self.margin) * scale), Int(($0.y + Self.margin) * scale)) }
            ?? (rep.pixelsWide / 2, rep.pixelsHigh / 2)
        return (
            rep.colorAt(x: center.0, y: center.1)?.usingColorSpace(.sRGB),
            rep.colorAt(x: 1, y: 1)?.usingColorSpace(.sRGB)
        )
    }

    static let backdrop = NSColor(srgbRed: 0.1, green: 0.2, blue: 0.3, alpha: 1)
    static let margin: CGFloat = 8
    static let labIcon = ImageKey.guildIcon(
        guildId: GuildId(rawValue: 1), hash: "0123456789abcdef0123456789abcdef")

    func same(_ a: NSColor?, _ b: NSColor?) -> Bool {
        guard let a, let b else {
            return false
        }
        return abs(a.redComponent - b.redComponent) < 0.02
            && abs(a.greenComponent - b.greenComponent) < 0.02
            && abs(a.blueComponent - b.blueComponent) < 0.02
    }

    func clearIcon() throws -> FakeImageSource {
        FakeImageSource.serving(try images.png("clear", pixels: 80, color: [0, 0, 0, 0]), pixels: 80)
    }

    // The acronym tile belongs to a server without an icon (and one whose icon failed). A
    // loaded icon shows what's behind its transparent pixels, as the official client does;
    // nothing is filled under it, loading or loaded, hovered or selected.
    @Test(arguments: [1, 2] as [CGFloat])
    func aServerIconShowsWhatsBehindItsTransparency(_ scale: CGFloat) async throws {
        let loaded = try await serverIcon(
            GuildIcon(name: "Lab", icon: Self.labIcon, highlighted: false), source: try clearIcon(),
            scale: scale)
        #expect(same(loaded.center, loaded.outside), "\(String(describing: loaded))")

        let selected = try await serverIcon(
            GuildIcon(name: "Lab", icon: Self.labIcon, highlighted: true), source: try clearIcon())
        #expect(same(selected.center, selected.outside), "no brand fill under a selected icon")

        let loading = try await serverIcon(
            GuildIcon(name: "Lab", icon: Self.labIcon, highlighted: false),
            source: FakeImageSource.stalling(), settle: false)
        #expect(same(loading.center, loading.outside), "nothing while it loads")

        let none = try await serverIcon(
            GuildIcon(name: "Lab", icon: nil, highlighted: false), source: try clearIcon())
        #expect(!same(none.center, none.outside), "a server without an icon keeps its tile")
    }

    // A press and release at a rail item's center, as the window gets them: its clicks.
    func railClicks<Icon: View>(source: FakeImageSource, settle: Bool = true, @ViewBuilder icon: @escaping (Bool, Bool) -> Icon) async throws -> Int {
        var clicks = 0
        let cache = ImageCache(source: source)
        let window = TestWindow.hide(
            ThreadBrowserWindowsTests.KeyWindow(
                contentRect: NSRect(origin: NSPoint(x: 100, y: 100), size: NSSize(width: 72, height: 60)), styleMask: [.titled],
                backing: .buffered, defer: false))
        window.isReleasedWhenClosed = false
        window.animationBehavior = .none
        windows.append(window)
        let host = NSHostingView(
            rootView: RailItem(selected: false, help: "Lab", icon: icon) { clicks += 1 }
                .environment(\.imageCache, cache))
        window.contentView = host
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }
        host.layoutSubtreeIfNeeded()
        let layers = MessageMenuAreaTests.views(ImageLayerView.self, in: host)
        if settle {
            try await eventually { layers.allSatisfy { $0.imageLayer.contents != nil || $0.failed } }
        }
        try settleEvents(in: window)
        let center = host.convert(NSPoint(x: host.bounds.midX, y: host.bounds.midY), to: nil)
        let event = { (type: NSEvent.EventType) in
            NSEvent.mouseEvent(
                with: type, location: center, modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                context: nil, eventNumber: 0, clickCount: 1, pressure: 1)
        }
        window.sendEvent(try #require(event(.leftMouseDown)))
        try await Task.sleep(for: .milliseconds(30))
        window.sendEvent(try #require(event(.leftMouseUp)))
        try await Task.sleep(for: .milliseconds(100))
        return clicks
    }

    // With nothing drawn under an icon, the rail item's whole tile still takes the click (and
    // the hover): a server with an icon, loading or loaded, transparent or not, and an unread DM.
    @Test
    func aRailItemTakesClicksOverItsIcon() async throws {
        let opaque = { FakeImageSource.serving(try self.images.png("red", pixels: 80), pixels: 80) }
        let lab = { (icon: ImageKey?) in
            { (highlighted: Bool, hovered: Bool) in
                GuildIcon(name: "Lab", icon: icon, highlighted: highlighted, hovered: hovered)
            }
        }
        #expect(try await railClicks(source: try opaque(), icon: lab(nil)) == 1, "the tile")
        #expect(try await railClicks(source: try opaque(), icon: lab(Self.labIcon)) == 1, "a loaded icon")
        #expect(try await railClicks(source: try clearIcon(), icon: lab(Self.labIcon)) == 1, "a clear icon")
        #expect(
            try await railClicks(source: FakeImageSource.stalling(), settle: false, icon: lab(Self.labIcon)) == 1,
            "a loading icon")
        #expect(
            try await railClicks(source: try opaque()) { _, hovered in
                UserAvatar(key: .avatar(2), size: 40, place: .railIcon, hovered: hovered)
            } == 1, "an unread DM's avatar")
    }

    // A failed icon falls back to the acronym tile (the official client shows a broken image),
    // and the icon is asked again later: the layer stays.
    @Test
    func aFailedServerIconShowsTheTile() async throws {
        let failed = try await serverIcon(
            GuildIcon(name: "Lab", icon: Self.labIcon, highlighted: false),
            source: FakeImageSource.failing())
        try await Task.sleep(for: .milliseconds(50))
        #expect(!same(failed.center, failed.outside))
    }

    @Test
    func theFolderMinisAndTheTitleBarIconShowWhatsBehindTheirIcons() async throws {
        let lab = Guild(id: GuildId(rawValue: 1), name: "Lab", icon: Self.labIcon)
        let folder = ServerListModel.FolderItem(
            folder: Folder(id: 7, name: nil, color: nil, guildIds: [lab.id]), guilds: [lab])
        // One server: its mini at the grid's top, centered (from the top left: 12...28, 3...19).
        let mini = NSPoint(x: 20, y: 11)
        let minis = try await serverIcon(
            FolderTile(item: folder, expanded: false).frame(width: 40, height: 40),
            source: try clearIcon(), size: 40, at: mini)
        let plain = try await serverIcon(
            FolderTile(
                item: ServerListModel.FolderItem(
                    folder: Folder(id: 7, name: nil, color: nil, guildIds: [lab.id]),
                    guilds: [Guild(id: lab.id, name: "Lab")]),
                expanded: false
            ).frame(width: 40, height: 40), source: try clearIcon(), size: 40, at: mini)
        #expect(!same(minis.center, plain.center), "the mini's tile is gone under a loaded icon")
        let empty = try await serverIcon(
            FolderTile(
                item: ServerListModel.FolderItem(
                    folder: Folder(id: 8, name: nil, color: nil, guildIds: []), guilds: []),
                expanded: false
            ).frame(width: 40, height: 40), source: try clearIcon(), size: 40, at: mini)
        #expect(same(minis.center, empty.center), "the folder's own tile shows through the icon")

        for scale in [1, 2] as [CGFloat] {
            let title = try await serverIcon(
                TitleBar.ServerIcon(name: "Lab", icon: Self.labIcon), source: try clearIcon(), size: 16,
                scale: scale)
            let titleTile = try await serverIcon(
                TitleBar.ServerIcon(name: "Lab", icon: nil), source: try clearIcon(), size: 16, scale: scale)
            #expect(same(title.center, title.outside), "\(scale)x")
            #expect(!same(titleTile.center, titleTile.outside), "\(scale)x")
        }
    }

    @Test
    func aConversationShowsTheRecipientOrTheGroupsIcon() throws {
        let dm = DirectMessageListModel.Conversation(
            channel: Channel(
                id: ChannelId(rawValue: 10), kind: .dm, guildId: nil, parentId: nil, name: nil,
                position: 0, topic: nil, nsfw: false, rateLimitPerUser: 0,
                recipientIds: [UserId(rawValue: 2)], thread: nil),
            recipients: [ray()], name: "Ray")
        #expect(layers(in: ConversationAvatar(conversation: dm, size: 32)).first?.content?.key == .avatar(2))

        let icon = ImageKey.groupIcon(channelId: ChannelId(rawValue: 11), hash: "0123456789abcdef0123456789abcdef")
        let group = DirectMessageListModel.Conversation(
            channel: Channel(
                id: ChannelId(rawValue: 11), kind: .groupDm, guildId: nil, parentId: nil,
                name: "Weekend plans", position: 0, topic: nil, nsfw: false, rateLimitPerUser: 0,
                recipientIds: [UserId(rawValue: 2)], icon: icon, thread: nil),
            recipients: [ray()], name: "Weekend plans")
        #expect(layers(in: ConversationAvatar(conversation: group, size: 32)).first?.content?.key == icon)

        var plain = group.channel
        plain.icon = nil
        let withoutIcon = DirectMessageListModel.Conversation(
            channel: plain, recipients: [ray()], name: "Weekend plans")
        #expect(layers(in: ConversationAvatar(conversation: withoutIcon, size: 32)).isEmpty)
    }

    // The environment's cache, policy and focus reach the layer view.
    @Test
    func theEnvironmentDecidesTheCacheAndTheMotion() throws {
        let cache = ImageCache(source: FakeImageSource.failing())
        let moving = ray(.avatar(2, animated: true))
        let hovered = UserAvatar(moving, size: 40, place: .dmRowAvatar, hovered: true)
            .environment(\.imageCache, cache)
            .environment(\.windowFocused, true)
        let layer = try #require(layers(in: hovered).first)
        #expect(layer.images === cache)
        #expect(layer.animates)

        let unfocused = UserAvatar(moving, size: 40, place: .dmRowAvatar, hovered: true)
            .environment(\.windowFocused, false)
        #expect(try #require(layers(in: unfocused).first).animates == false)

        let notHovered = UserAvatar(moving, size: 40, place: .dmRowAvatar)
            .environment(\.windowFocused, true)
        #expect(try #require(layers(in: notHovered).first).animates == false)

        let panel = UserAvatar(moving, size: 32, place: .userPanel, hovered: true)
            .environment(\.windowFocused, true)
        #expect(try #require(layers(in: panel).first).animates == false)
    }

    @Test
    func theTitleBarIconIsTheConversationsAvatar() throws {
        let title = WindowTitle.at(.home, server: nil, conversation: (name: "Ray", avatar: .avatar(2)))

        #expect(title == WindowTitle(text: "Ray", icon: .avatar(.avatar(2))))
        let layer = try #require(layers(in: TitleBar(title: title), size: NSSize(width: 600, height: 32)).first)
        #expect(layer.content?.key == .avatar(2))
        #expect(layer.frame.size == NSSize(width: 16, height: 16))
        #expect(layer.cornerRadius == 8)
    }

    @Test
    func theUserPanelShowsTheCurrentUsersAvatarStatic() throws {
        let layer = try #require(layers(in: UserPanel(user: ray(.avatar(2, animated: true)))).first)

        #expect(layer.content?.key == .avatar(2, animated: true))
        #expect(layer.frame.size == NSSize(width: 32, height: 32))
        #expect(!layer.animates)
    }
}
