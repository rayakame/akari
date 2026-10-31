import AkariKit
import AppKit
import SwiftUI

/// The red count of unread mentions, 16 tall.
struct MentionBadge: View {
    let count: UInt32

    var body: some View {
        Text(Self.text(count))
            .font(TypeScale.mentionBadge.font)
            .foregroundStyle(Color(nsColor: Palette.mentionBadgeText))
            .lineLimit(1)
            .minimumScaleFactor(0.7)
            .frame(width: Self.width(count), height: 16)
            .background(Capsule().fill(Color(nsColor: Palette.mentionBadge)))
    }

    static func width(_ count: UInt32) -> CGFloat {
        switch count {
        case ..<10: 16
        case ..<100: 22
        default: 30
        }
    }

    static func text(_ count: UInt32) -> String {
        count < 1000 ? "\(count)" : "\(min(count / 1000, 9))k+"
    }

    /// What VoiceOver reads after a row's or an icon's name.
    static func accessibilityValue(_ badge: Badge, muted: Bool = false) -> String {
        let mentions: String? =
            switch badge.mentions {
            case 0: nil
            case 1: "1 mention"
            default: "\(badge.mentions) mentions"
            }
        let posts: String? =
            switch badge.posts {
            case .new(let count)?: count == 1 ? "1 new post" : "\(count) new posts"
            case .unread(let count)?: count == 1 ? "1 unread post" : "\(count) unread posts"
            case nil: nil
            }
        return [muted ? "muted" : nil, badge.unread ? "unread" : nil, mentions, posts]
            .compactMap { $0 }
            .joined(separator: ", ")
    }
}

/// A forum row's count where it has no mentions: "3 New" in the brand's color for new posts,
/// else the muted count of posts with unread activity (Discord's text-xs/semibold).
struct PostCountText: View {
    let count: PostCount

    static func text(_ count: PostCount) -> String {
        switch count {
        case .new(let count): "\(MentionBadge.text(count)) New"
        case .unread(let count): MentionBadge.text(count)
        }
    }

    var body: some View {
        Text(Self.text(count))
            .font(TypeScale.forumStatusAction.font)
            .foregroundStyle(
                Color(nsColor: {
                    if case .new = count { Palette.textLink } else { Palette.textMuted }
                }())
            )
            .lineLimit(1)
            .fixedSize()
    }
}

extension View {
    /// The mention badge at the bottom right corner, cut out of the icon by a ring of the
    /// color behind it.
    func mentionBadge(_ count: UInt32, ring: NSColor = Palette.frame) -> some View {
        overlay(alignment: .bottomTrailing) {
            if count > 0 {
                MentionBadge(count: count)
                    .padding(3)
                    .background(Capsule().fill(Color(nsColor: ring)))
                    .offset(x: 4, y: 4)
            }
        }
    }
}
