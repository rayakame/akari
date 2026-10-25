import AkariKit
import SwiftUI

/// An image from the session's cache in a SwiftUI place: a round avatar, a server tile, a
/// title bar icon. The same layer view as the message list's, sized by its frame.
struct RemoteImage: NSViewRepresentable {
    let key: ImageKey?
    var cornerRadius: CGFloat = 0
    var placeholder = ImageLayerView.Placeholder.none
    var place = MotionPolicy.Place.userPanel
    var hovered = false
    /// Told whether the image failed, after each change (never during a view update).
    var onFailure: ((Bool) -> Void)?
    @Environment(\.imageCache) private var images
    @Environment(\.motion) private var motion
    @Environment(\.windowFocused) private var focused

    final class Coordinator {
        var onFailure: ((Bool) -> Void)?
        var reported: Bool?
    }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> ImageLayerView {
        let view = ImageLayerView()
        let coordinator = context.coordinator
        view.onStateChange = { view in
            let failed = view.failed
            guard coordinator.reported != failed else {
                return
            }
            coordinator.reported = failed
            // SwiftUI state can't change while it updates this view.
            DispatchQueue.main.async { coordinator.onFailure?(failed) }
        }
        return view
    }

    func updateNSView(_ view: ImageLayerView, context: Context) {
        context.coordinator.onFailure = onFailure
        view.images = images
        view.cornerRadius = cornerRadius
        view.placeholder = placeholder
        view.show(key.map { ImageLayerView.Content(key: $0) })
        view.animates = motion.plays(
            place, MotionPolicy.Context(windowKey: focused, appActive: focused, hovered: hovered))
    }
}

/// A user's avatar: round, the loading disc, animated where its place allows.
struct UserAvatar: View {
    let key: ImageKey?
    let size: CGFloat
    var place = MotionPolicy.Place.userPanel
    var hovered = false

    init(_ user: User?, size: CGFloat, place: MotionPolicy.Place = .userPanel, hovered: Bool = false) {
        key = user?.avatar
        self.size = size
        self.place = place
        self.hovered = hovered
    }

    init(key: ImageKey?, size: CGFloat, place: MotionPolicy.Place = .userPanel, hovered: Bool = false) {
        self.key = key
        self.size = size
        self.place = place
        self.hovered = hovered
    }

    var body: some View {
        RemoteImage(
            key: key, cornerRadius: size / 2, placeholder: .disc, place: place, hovered: hovered
        )
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

/// A conversation's picture: the other person's avatar in a DM; a group's icon, or the group
/// symbol on a disc without one (Discord's defaults are its own assets).
struct ConversationAvatar: View {
    let conversation: DirectMessageListModel.Conversation
    let size: CGFloat
    var place = MotionPolicy.Place.dmRowAvatar
    var hovered = false

    var body: some View {
        if conversation.channel.kind == .dm {
            UserAvatar(conversation.recipients.first, size: size, place: place, hovered: hovered)
        } else if let icon = conversation.channel.icon {
            RemoteImage(key: icon, cornerRadius: size / 2, placeholder: .disc)
                .frame(width: size, height: size)
                .accessibilityHidden(true)
        } else {
            Circle()
                .fill(Color(nsColor: Palette.hoverBackground))
                .frame(width: size, height: size)
                .overlay {
                    Image(systemName: ChannelSymbols.name(for: .groupDm))
                        .font(.system(size: size * 0.45, weight: .medium))
                        .foregroundStyle(Color(nsColor: Palette.textDefault))
                }
                .accessibilityHidden(true)
        }
    }
}

extension EnvironmentValues {
    /// The session's decoded images; `nil` shows placeholders only (previews, tests).
    @Entry var imageCache: ImageCache?
    @Entry var motion = MotionPolicy.default
    /// The window is key and the app active.
    @Entry var windowFocused = false
}

extension View {
    /// What every image in the session's views needs, set once at a hosting root.
    func imageEnvironment(_ session: SessionModel) -> some View {
        environment(\.imageCache, session.images)
            .environment(\.motion, session.motion)
            .environment(\.windowFocused, session.focused)
    }
}
