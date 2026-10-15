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
    static func accessibilityValue(_ badge: Badge) -> String {
        let mentions: String? =
            switch badge.mentions {
            case 0: nil
            case 1: "1 mention"
            default: "\(badge.mentions) mentions"
            }
        return [badge.unread ? "unread" : nil, mentions].compactMap { $0 }
            .joined(separator: ", ")
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
