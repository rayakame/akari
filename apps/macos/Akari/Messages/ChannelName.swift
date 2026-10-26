import AkariKit

// The name is empty before READY, while the store doesn't know the channel yet.
struct ChannelName: Equatable {
    enum Kind: Equatable {
        case guildChannel(ChannelType)
        case thread(owner: String?)
        // The recipient, if the store knows them.
        case direct(User?)
        case group
    }

    let kind: Kind
    let name: String

    var placeholder: String {
        if name.isEmpty {
            return "Message"
        }
        return switch kind {
        case .guildChannel: "Message #\(name)"
        case .thread: "Message \"\(name)\""
        case .direct: "Message @\(name)"
        case .group: "Message \(name)"
        }
    }

    var start: ChannelStart? {
        if name.isEmpty {
            return nil
        }
        return switch kind {
        case .guildChannel(let type):
            ChannelStart(
                icon: .symbol(ChannelSymbols.name(for: type)), title: "Welcome to #\(name)!",
                subtitle: "This is the start of the #\(name) channel.")
        case .thread(let owner):
            ChannelStart(
                icon: .symbol(ChannelSymbols.name(for: .publicThread)), title: name,
                subtitle: owner.map { _ in "Started by " }, subtitleName: owner)
        case .direct(let user):
            ChannelStart(
                icon: .avatar(user?.avatar), title: name,
                username: user?.username,
                subtitle: "This is the beginning of your direct message history with \(name).")
        case .group:
            ChannelStart(
                icon: .symbol(ChannelSymbols.name(for: .groupDm)), title: name,
                subtitle: "Welcome to the beginning of the \(name) group.")
        }
    }
}

struct ChannelStart: Equatable {
    enum Icon: Equatable {
        case symbol(String)
        /// The other person's avatar, 80 points; `nil` while the store doesn't know them.
        case avatar(ImageKey?)
    }

    let icon: Icon
    let title: String
    var username: String?
    let subtitle: String?
    var subtitleName: String?
}
