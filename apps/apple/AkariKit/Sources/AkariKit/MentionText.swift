/// What mentions and other tokens show, in Akari's words where Discord's aren't a plain
/// description.
public enum MentionText {
    /// nil for a missing user: the mention shows its source as plain text, as Discord does.
    public static func user(_ name: MentionName) -> String? {
        switch name {
        case .known(let name): "@\(name)"
        case .missing: nil
        case .noAccess, .notLoaded: "@user"
        }
    }

    public static func role(_ name: MentionName) -> String {
        switch name {
        case .known(let name): "@\(name)"
        case .missing, .noAccess: "@unknown-role"
        case .notLoaded: "@role"
        }
    }

    /// Without the `#`: the pill shows the channel kind's symbol before it.
    public static func channel(_ name: MentionName) -> String {
        switch name {
        case .known(let name): name
        case .missing: "unknown"
        case .noAccess: "No Access"
        case .notLoaded: "channel"
        }
    }

    public static func command(_ name: String) -> String {
        "/\(name)"
    }

    public static func navigation(_ target: GuildNavigation) -> String {
        switch target {
        case .customize: "Channels & Roles"
        case .browse: "Browse Channels"
        case .guide: "Server Guide"
        case .linkedRoles: "Linked Roles"
        }
    }

    /// Names the store doesn't have yet are drawn muted, so nothing reads as deleted.
    public static func isPlaceholder(_ name: MentionName) -> Bool {
        name == .notLoaded
    }
}
