// What the user typed per channel, kept for the session and handed on by a reconnect.
@MainActor
final class Drafts {
    private var texts: [ChannelId: String] = [:]
    var forms: [ChannelId: ThreadFormDraft] = [:]
    var posts: [ChannelId: PostFormDraft] = [:]
    var replies: [ChannelId: PendingReply] = [:]
    // Failed replies Discord refused because the message they answer was deleted.
    var refusedReplies: Set<MessageId> = []

    subscript(channel: ChannelId) -> String {
        get { texts[channel] ?? "" }
        set { texts[channel] = newValue.isEmpty ? nil : newValue }
    }

    // After what's there, a line each.
    func append(_ lines: [String], to channel: ChannelId) {
        self[channel] = ([self[channel]] + lines).filter { !$0.isEmpty }.joined(separator: "\n")
    }
}
