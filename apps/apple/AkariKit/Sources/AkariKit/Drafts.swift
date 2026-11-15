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
    // By the save that waits, so an earlier save's answer leaves a later one's alone.
    var unansweredSaves: [MessageId: Int] = [:]
    var saves = 0
    // Refused before asking, so the core keeps no record: they wait for the editor here.
    var thrownRefusals: [MessageId: (text: String, error: EditError)] = [:]
    // Refusals the editor took; a row read before the core forgot them still shows them.
    var takenRefusals: Set<MessageId> = []
    var editOpenings = 0

    subscript(channel: ChannelId) -> String {
        get { texts[channel] ?? "" }
        set { texts[channel] = newValue.isEmpty ? nil : newValue }
    }

    // Deletes the open lists didn't see end their channels' edits too.
    func forget(_ deleted: [ChannelId: Set<MessageId>]) {
        for (channel, messages) in deleted {
            if let edit = edits[channel], messages.contains(edit.messageId) {
                edits[channel] = nil
            }
            for message in messages {
                thrownRefusals[message] = nil
            }
        }
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
