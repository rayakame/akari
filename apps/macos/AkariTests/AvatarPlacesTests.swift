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

    func layers<Content: View>(in view: Content, size: NSSize = NSSize(width: 300, height: 100))
        -> [ImageLayerView]
    {
        let host = NSHostingView(rootView: view)
        let window = NSWindow(
            contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled],
            backing: .buffered, defer: false)
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
        let images = TestImages()
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
