import Foundation
import Observation

/// The "New Thread" form beside a channel: the thread's name, whether it's private, and the
/// message that starts it. What's typed is kept per channel for the session.
@MainActor @Observable
public final class ThreadFormModel {
    /// What the "Private Thread" checkbox offers.
    public enum PrivateMode: Equatable, Sendable {
        /// The user may create both kinds: the checkbox is theirs.
        case enabled
        /// Only private threads: checked and disabled.
        case privateOnly
        /// No private threads here, or a thread from a message: no checkbox.
        case disabled
    }

    /// A dialog, in Discord's words.
    public struct Alert: Equatable, Sendable {
        public let title: String
        public let message: String

        public init(title: String, message: String) {
            self.title = title
            self.message = message
        }
    }

    public static let title = "New Thread"
    public static let nameRequired = "Thread Name is required"
    public static let messageRequired = "Starter Message is required."
    public static let placeholder = "Enter a message to start the conversation!"
    public static let privateLabel = "Private Thread"
    public static let privateDetail = "Only people you invite and moderators can see"
    public static let privateHint = "You can invite new people by @mentioning them."
    static let blocked =
        "This thread cannot be sent because it contains content blocked by this community. "
        + "Please revise your post and try again."
    /// Discord's limit for a name.
    public static let nameLimit = 100

    public let parentId: ChannelId
    /// The message the thread starts from, shown above the fields; `nil` for a thread without
    /// one.
    public let source: Message?
    public let location: CreateThreadLocation
    /// Line breaks become spaces, and it stops at 100 characters, as Discord's field does.
    public var name: String {
        didSet {
            let cleaned = Self.cleaned(name)
            // Setting it here doesn't run this again.
            if cleaned != name {
                name = cleaned
            }
            drafts.forms[parentId, default: .init()].name = name
            // AutoMod's stays until the next try, as in the official client.
            if nameError == Self.nameRequired {
                nameError = nil
            }
        }
    }
    public var isPrivate: Bool {
        didSet { drafts.forms[parentId, default: .init()].isPrivate = isPrivate }
    }
    /// The thread's first message.
    public var message: String {
        didSet {
            drafts.forms[parentId, default: .init()].message = message
            length = Self.length(of: message)
            messageError = nil
        }
    }
    public private(set) var nameError: String?
    public private(set) var messageError: String?
    /// A dialog the last attempt left, until `dismissAlert()`.
    public private(set) var alert: Alert?
    public private(set) var privateMode: PrivateMode
    /// The parent's slowmode for new threads.
    public private(set) var slowmode: Slowmode? {
        didSet { awaitCooldownEnd() }
    }
    public private(set) var submitting = false
    /// Code points of the message, as Discord counts them.
    public private(set) var length: Int
    public private(set) var lengthLimit: Int

    @ObservationIgnored private let account: Account
    @ObservationIgnored private let store: Store
    @ObservationIgnored private let drafts: Drafts
    @ObservationIgnored private var guildId: GuildId?
    @ObservationIgnored private var cooldownEnd: Task<Void, Never>?

    init(
        parentId: ChannelId, source: Message?, location: CreateThreadLocation, account: Account,
        store: Store, drafts: Drafts
    ) {
        self.parentId = parentId
        self.source = source
        self.location = location
        self.account = account
        self.store = store
        self.drafts = drafts
        var draft = drafts.forms[parentId] ?? .init()
        // Opened for another message, the name starts over; the rest stays.
        if draft.source != source?.id {
            draft.name = ""
            draft.source = source?.id
            drafts.forms[parentId] = draft
        }
        name = draft.name
        isPrivate = draft.isPrivate
        message = draft.message
        length = Self.length(of: draft.message)
        lengthLimit = Int(store.messageLengthLimit())
        privateMode = .disabled
        guildId = store.channel(id: parentId)?.guildId
        readPrivateMode()
        slowmode = store.threadSlowmode(parentId: parentId)
        awaitCooldownEnd()
    }

    /// "Thread Name", or "Thread Name (Optional)" from a message, which names the thread when
    /// left empty.
    public var nameLabel: String {
        source == nil ? "Thread Name" : "Thread Name (Optional)"
    }

    /// What the empty name field shows: from a message, the name the thread gets.
    public var namePlaceholder: String {
        derivedName.isEmpty ? Self.title : derivedName
    }

    // Discord names a thread from a message without text "Thread".
    private var derivedName: String {
        source.map { ThreadName.derived(from: $0) } ?? ""
    }

    /// Whether closing it with the X asks first.
    public var hasText: Bool {
        !name.isEmpty || !message.isEmpty
    }

    /// Characters left once 200 or fewer remain (negative past the limit), else `nil`.
    public var remaining: Int? {
        let left = lengthLimit - length
        return left <= 200 ? left : nil
    }

    /// The private checkbox's state as it is sent.
    public var createsPrivate: Bool {
        switch privateMode {
        case .enabled: isPrivate
        case .privateOnly: true
        case .disabled: false
        }
    }

    public func dismissAlert() {
        alert = nil
    }

    /// Checks what's typed and creates the thread; returns it with the first message as typed
    /// then, or `nil` when nothing was created (the fields' errors or `alert` say why, and
    /// everything typed stays).
    func create() async -> (thread: ChannelId, firstMessage: String)? {
        guard !submitting else {
            return nil
        }
        nameError = nil
        messageError = nil
        let content = message.trimmingCharacters(in: .whitespacesAndNewlines)
        let typed = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if content.isEmpty {
            messageError = Self.messageRequired
        }
        if typed.isEmpty, source == nil {
            nameError = Self.nameRequired
        }
        guard messageError == nil, nameError == nil else {
            return nil
        }
        let length = Self.length(of: content)
        guard length <= lengthLimit else {
            messageError =
                ComposerModel.Problem.tooLong(by: length - lengthLimit, limit: lengthLimit).text
            return nil
        }
        submitting = true
        defer { submitting = false }
        do {
            let thread = try await account.createThread(
                parentId: parentId, messageId: source?.id,
                name: typed.isEmpty ? (derivedName.isEmpty ? "Thread" : derivedName) : typed,
                isPrivate: createsPrivate,
                location: location)
            slowmode = store.threadSlowmode(parentId: parentId)
            return (thread, content)
        } catch {
            slowmode = store.threadSlowmode(parentId: parentId)
            refused(error as? CreateThreadError)
            return nil
        }
    }

    func clearDrafts() {
        drafts.forms[parentId] = nil
    }

    func apply(_ batch: EventBatch) {
        let guildChanged =
            guildId.map { batch.guildsChanged.contains($0) || batch.membersChanged.contains($0) }
            ?? false
        let channelChanged = batch.channelsChanged[guildId]?.contains(parentId) == true
        if batch.everythingChanged || guildChanged || channelChanged {
            readPrivateMode()
            let read = store.threadSlowmode(parentId: parentId)
            if read != slowmode {
                slowmode = read
            }
        }
        if batch.everythingChanged || batch.currentUserChanged {
            lengthLimit = Int(store.messageLengthLimit())
        }
    }

    private func refused(_ error: CreateThreadError?) {
        switch error {
        case .Slowmode?:
            break
        case .TooManyThreads(let announcement)?:
            alert = Alert(
                title: "Cannot create thread",
                message: announcement
                    ? "This server has reached the maximum number of active announcement "
                        + "threads. Free up space by archiving a thread."
                    : "This server has reached the maximum number of active threads. Free up "
                        + "space by archiving a thread.")
        case .NameBlocked(let reason)?:
            nameError = reason.isEmpty ? Self.blocked : reason
        case .NameRequired?:
            nameError = Self.nameRequired
        case .Request(.RateLimited)?:
            alert = Alert(title: "Cannot create thread", message: "You are being rate limited.")
        case .Request?, nil:
            alert = Alert(title: "Error", message: "An error occurred, please try again.")
        }
    }

    private func readPrivateMode() {
        let creation = store.threadCreation(parentId: parentId)
        privateMode =
            switch (source == nil, creation.public, creation.private) {
            case (true, true, true): .enabled
            case (true, false, true): .privateOnly
            default: .disabled
            }
    }

    // Nothing else reads the slowmode when it runs out.
    private func awaitCooldownEnd() {
        cooldownEnd?.cancel()
        guard let until = slowmode?.until else {
            return
        }
        let wait = max(0, until.timeIntervalSinceNow) + 0.05
        cooldownEnd = Task { [weak self] in
            try? await Task.sleep(for: .seconds(wait))
            guard !Task.isCancelled, let self else {
                return
            }
            slowmode = store.threadSlowmode(parentId: parentId)
        }
    }

    static func cleaned(_ name: String) -> String {
        let flat = name.replacingOccurrences(of: "\r\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
            .replacingOccurrences(of: "\n", with: " ")
        guard flat.unicodeScalars.count > nameLimit else {
            return flat
        }
        return String(String.UnicodeScalarView(flat.unicodeScalars.prefix(nameLimit)))
    }

    private static func length(of text: String) -> Int {
        text.isEmpty ? 0 : Int(messageLength(content: text))
    }
}

/// What the "New Thread" form of a channel holds while it's closed.
struct ThreadFormDraft: Equatable {
    var name = ""
    var isPrivate = false
    var message = ""
    // The message the name was typed for.
    var source: MessageId?
}
