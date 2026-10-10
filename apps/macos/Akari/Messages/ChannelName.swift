// The name is empty before READY, while the store doesn't know the channel yet.
struct ChannelName: Equatable {
    let inGuild: Bool
    let name: String

    var placeholder: String {
        if name.isEmpty {
            return "Write a message"
        }
        return inGuild ? "Write a message in #\(name)" : "Write a message to \(name)"
    }

    var beginning: String {
        if name.isEmpty {
            return ""
        }
        return inGuild
            ? "This is the beginning of #\(name)."
            : "This is the beginning of your conversation with \(name)."
    }
}
