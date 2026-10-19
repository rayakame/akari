import AkariKit

// The name is empty before READY, while the store doesn't know the channel yet.
struct ChannelName: Equatable {
    enum Kind: Equatable {
        case guildChannel(ChannelType)
        // Who started it, if known.
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
                icon: .avatar(name: name, id: user?.id.rawValue ?? 0), title: name,
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
        case avatar(name: String, id: UInt64)
    }

    let icon: Icon
    let title: String
    var username: String?
    // Without one, the start ends with its title.
    let subtitle: String?
    // After the subtitle, strong: a thread's owner.
    var subtitleName: String?
}
