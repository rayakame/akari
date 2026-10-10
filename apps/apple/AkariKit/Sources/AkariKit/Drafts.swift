// What the user typed per channel, kept for the session and handed on by a reconnect.
@MainActor
final class Drafts {
    private var texts: [ChannelId: String] = [:]

    subscript(channel: ChannelId) -> String {
        get { texts[channel] ?? "" }
        set { texts[channel] = newValue.isEmpty ? nil : newValue }
    }

    // After what's there, a line each.
    func append(_ lines: [String], to channel: ChannelId) {
        self[channel] = ([self[channel]] + lines).filter { !$0.isEmpty }.joined(separator: "\n")
    }
}
