import Testing

@testable import AkariKit

// Whether an animated image plays, from the settings, Reduce Motion, the window being key and
// the app active, visibility and hover (D136, research §2 and §3).
struct MotionPolicyTests {
    let defaults = DisplaySettings(
        renderSpoilers: .onClick, viewImageDescriptions: false, inlineAttachmentMedia: true,
        inlineEmbedMedia: true, gifAutoPlay: true, renderEmbeds: true, animateEmoji: true,
        animateStickers: .always, explicitContentFilter: .nonFriends,
        explicitContent: .init(guilds: .unset, friendDm: .unset, nonFriendDm: .unset),
        gore: .init(guilds: .unset, friendDm: .unset, nonFriendDm: .unset),
        selfHarm: .init(guilds: .unset, friendDm: .unset, nonFriendDm: .unset))

    func policy(
        gifAutoPlay: Bool = true, animateEmoji: Bool = true, stickers: StickerAnimation = .always,
        reduceMotion: Bool = false
    ) -> MotionPolicy {
        var settings = defaults
        settings.gifAutoPlay = gifAutoPlay
        settings.animateEmoji = animateEmoji
        settings.animateStickers = stickers
        return MotionPolicy(settings: settings, reduceMotion: reduceMotion)
    }

    func context(
        key: Bool = true, active: Bool = true, hovered: Bool = false, visible: Double = 1,
        nearBottom: Bool = true
    ) -> MotionPolicy.Context {
        MotionPolicy.Context(
            windowKey: key, appActive: active, hovered: hovered, visibleFraction: visible,
            nearBottom: nearBottom)
    }

    // Hovering a message of the group, a DM row or a rail icon plays its avatar; nothing else.
    @Test
    func avatarsPlayOnHoverOnly() {
        for place in [MotionPolicy.Place.messageAvatar, .dmRowAvatar, .railIcon] {
            #expect(policy().plays(place, context(hovered: true)), "\(place)")
            #expect(!policy().plays(place, context(hovered: false)), "\(place)")
            #expect(policy(gifAutoPlay: false).plays(place, context(hovered: true)), "\(place)")
            #expect(policy(reduceMotion: true).plays(place, context(hovered: true)), "\(place)")
        }
    }

    @Test
    func someAvatarsNeverPlay() {
        for place in [MotionPolicy.Place.userPanel, .replyAvatar, .facepile, .threadBoxAvatar] {
            #expect(!policy().plays(place, context(hovered: true)), "\(place)")
            #expect(!policy().plays(place, context(hovered: false)), "\(place)")
        }
    }

    @Test
    func nothingPlaysWhileTheWindowIsntKeyOrTheAppIsInactive() {
        for place in MotionPolicy.Place.allCases {
            #expect(!policy().plays(place, context(key: false, hovered: true)), "\(place)")
            #expect(!policy().plays(place, context(active: false, hovered: true)), "\(place)")
        }
    }

    // animate_emoji, at least 70 % visible and among the 100 messages nearest the bottom; or
    // hovered.
    @Test
    func emojiFollowTheSettingVisibilityAndTheNearestHundred() {
        #expect(policy().plays(.emoji, context()))
        #expect(!policy().plays(.emoji, context(visible: 0.69)))
        #expect(policy().plays(.emoji, context(visible: 0.7)))
        #expect(!policy().plays(.emoji, context(nearBottom: false)))
        #expect(!policy(animateEmoji: false).plays(.emoji, context()))
        #expect(!policy(reduceMotion: true).plays(.emoji, context()))
        #expect(policy(animateEmoji: false).plays(.emoji, context(hovered: true)))
        #expect(policy(reduceMotion: true).plays(.emoji, context(hovered: true)))
        #expect(policy().plays(.emoji, context(hovered: true, visible: 0.1, nearBottom: false)))
    }

    // Always, on interaction (the message hovered) or never; Reduce Motion turns Always into
    // on interaction; at least 70 % visible.
    @Test
    func stickersFollowTheirSetting() {
        #expect(policy(stickers: .always).plays(.sticker, context()))
        #expect(!policy(stickers: .always).plays(.sticker, context(visible: 0.5)))
        #expect(!policy(stickers: .onInteraction).plays(.sticker, context()))
        #expect(policy(stickers: .onInteraction).plays(.sticker, context(hovered: true)))
        #expect(!policy(stickers: .never).plays(.sticker, context(hovered: true)))
        #expect(!policy(stickers: .always, reduceMotion: true).plays(.sticker, context()))
        #expect(
            policy(stickers: .always, reduceMotion: true).plays(.sticker, context(hovered: true)))
    }

    // gif_auto_play and at least 60 % visible, or hovered; Reduce Motion turns autoplay off.
    @Test
    func gifsFollowAutoplayVisibilityAndHover() {
        #expect(policy().plays(.gif, context()))
        #expect(!policy().plays(.gif, context(visible: 0.59)))
        #expect(policy().plays(.gif, context(visible: 0.6)))
        #expect(!policy(gifAutoPlay: false).plays(.gif, context()))
        #expect(policy(gifAutoPlay: false).plays(.gif, context(hovered: true)))
        #expect(!policy(reduceMotion: true).plays(.gif, context()))
        #expect(policy(reduceMotion: true).plays(.gif, context(hovered: true)))
    }

    @Test
    func thePolicyComesFromTheSettings() {
        let policy = MotionPolicy(settings: defaults, reduceMotion: false)

        #expect(policy.gifAutoPlay)
        #expect(policy.animateEmoji)
        #expect(policy.animateStickers == .always)
        #expect(!policy.reduceMotion)
        #expect(MotionPolicy.default == policy)
    }
}
