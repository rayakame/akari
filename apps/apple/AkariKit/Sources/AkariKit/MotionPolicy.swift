/// Whether an animated image plays (D136): the user's settings and the system's Reduce Motion,
/// which the official client reads the same way, and a place's own rule over the window being
/// key, the app active, how much of the image shows and whether it's hovered.
public struct MotionPolicy: Equatable, Sendable {
    /// Where the image is; each place has its rule.
    public enum Place: Equatable, Sendable, CaseIterable {
        /// A message group's header; hovering any message of the group counts.
        case messageAvatar
        case dmRowAvatar
        case railIcon
        case userPanel
        case replyAvatar
        case facepile
        case threadBoxAvatar
        case emoji
        case sticker
        case gif
    }

    /// What the view knows about the moment.
    public struct Context: Equatable, Sendable {
        public var windowKey: Bool
        public var appActive: Bool
        public var hovered: Bool
        /// How much of the image is inside the visible area, 0–1.
        public var visibleFraction: Double
        /// Among the 100 messages nearest the bottom, where custom emoji may animate.
        public var nearBottom: Bool

        public init(
            windowKey: Bool, appActive: Bool, hovered: Bool, visibleFraction: Double = 1,
            nearBottom: Bool = true
        ) {
            self.windowKey = windowKey
            self.appActive = appActive
            self.hovered = hovered
            self.visibleFraction = visibleFraction
            self.nearBottom = nearBottom
        }
    }

    public var gifAutoPlay: Bool
    public var animateEmoji: Bool
    public var animateStickers: StickerAnimation
    public var reduceMotion: Bool

    public init(settings: DisplaySettings, reduceMotion: Bool) {
        gifAutoPlay = settings.gifAutoPlay
        animateEmoji = settings.animateEmoji
        animateStickers = settings.animateStickers
        self.reduceMotion = reduceMotion
    }

    /// Discord's defaults, Reduce Motion off.
    public static let `default` = MotionPolicy(
        gifAutoPlay: true, animateEmoji: true, animateStickers: .always, reduceMotion: false)

    init(
        gifAutoPlay: Bool, animateEmoji: Bool, animateStickers: StickerAnimation,
        reduceMotion: Bool
    ) {
        self.gifAutoPlay = gifAutoPlay
        self.animateEmoji = animateEmoji
        self.animateStickers = animateStickers
        self.reduceMotion = reduceMotion
    }

    public func plays(_ place: Place, _ context: Context) -> Bool {
        guard context.windowKey, context.appActive else {
            return false
        }
        switch place {
        case .messageAvatar, .dmRowAvatar, .railIcon:
            return context.hovered
        case .userPanel, .replyAvatar, .facepile, .threadBoxAvatar:
            return false
        case .emoji:
            let setting = animateEmoji && !reduceMotion
            return context.hovered
                || (setting && context.visibleFraction >= 0.7 && context.nearBottom)
        case .sticker:
            guard context.visibleFraction >= 0.7 else {
                return false
            }
            switch animateStickers {
            case .always where !reduceMotion: return true
            case .always, .onInteraction: return context.hovered
            case .never: return false
            }
        case .gif:
            guard context.visibleFraction >= 0.6 else {
                return false
            }
            return context.hovered || (gifAutoPlay && !reduceMotion)
        }
    }
}
