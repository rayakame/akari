// What the user typed per channel, kept for the session and handed on by a reconnect.
@MainActor
final class Drafts {
    private var texts: [ChannelId: String] = [:]
    var forms: [ChannelId: ThreadFormDraft] = [:]
    var posts: [ChannelId: PostFormDraft] = [:]
    var replies: [ChannelId: PendingReply] = [:]
    // Failed replies Discord refused because the message they answer was deleted.
    var refusedReplies: Set<MessageId> = []
    var edits: [ChannelId: EditDraft] = [:]
    // Saves whose answer hasn't come back to the model that sent them.
    var unansweredSaves: Set<MessageId> = []
    // Refusals the editor took; a row read before the core forgot them still shows them.
    var takenRefusals: Set<MessageId> = []
    var editOpenings = 0

    subscript(channel: ChannelId) -> String {
        get { texts[channel] ?? "" }
        set { texts[channel] = newValue.isEmpty ? nil : newValue }
    }

    // After what's there, a line each.
    func append(_ lines: [String], to channel: ChannelId) {
        self[channel] = ([self[channel]] + lines).filter { !$0.isEmpty }.joined(separator: "\n")
    }
}

struct EditDraft {
    let messageId: MessageId
    var text: String
    var problem: EditProblem?
}
