import AkariFFI
import Foundation
import Observation

/// The form a forum's header row turns into: a new post's title, tags and first message. What's
/// typed stays with the forum for the session, also while the form is closed.
@MainActor @Observable
public final class PostFormModel {
    public struct Alert: Equatable, Sendable {
        public let title: String
        public let message: String
    }

    public static let newPost = "New Post"
    public static let post = "Post"
    public static let titlePlaceholder = "Title"
    public static let rowPlaceholder = "Search or create a post..."
    public static let messagePlaceholder = "Enter a message..."
    public static let titleRequired = "A title is required"
    public static let messageRequired = "A message is required"
    public static let tagRequired = "A tag is required to create a post in this channel"
    public static let noPermission = "You do not have permission to post in this channel."
    public static let slowmodeOn = "Slowmode is enabled."
    public static let needsUploads =
        "Creating media posts needs uploads, which Akari doesn't have yet."
    public static let titleLimit = ThreadFormModel.nameLimit
    public static let tagLimit = 5

    public let forumId: ChannelId
    public private(set) var isOpen = false
    /// Line breaks become spaces, and it stops at 100 characters, as Discord's field does.
    public var title: String {
        didSet {
            let cleaned = ThreadFormModel.cleaned(title)
            if cleaned != title {
                title = cleaned
            }
            drafts.posts[forumId, default: .init()].title = title
            // As the official client: emptied with no message begun, it's a search's field again.
            if typedTitle.isEmpty, tags.isEmpty, !hasMessage || typedMessage == template {
                begun = false
            }
            // AutoMod's stays until the next try, as in the official client.
            if titleError == Self.titleRequired {
                titleError = nil
            }
        }
    }
    /// The post's first message; it starts as the forum's template.
    public var message: String {
        didSet {
            // The template alone is no draft.
            drafts.posts[forumId, default: .init()].message = typedMessage == template ? nil : message
            length = Self.length(of: message)
            messageError = nil
        }
    }
    /// The chosen tags, in the order chosen.
    public private(set) var tags: [TagId]
    /// The forum's tags the user may apply, in the forum's order.
    public private(set) var usableTags: [ForumTag] = []
    /// Whether the user may post here at all.
    public private(set) var allowed = false
    /// A media channel: its posts need uploads.
    public let isMedia: Bool
    public private(set) var requiresTag = false
    public private(set) var titleError: String?
    public private(set) var messageError: String?
    /// A dialog the last attempt left, until `dismissAlert()`.
    public private(set) var alert: Alert?
    /// The forum's slowmode for new posts.
    public private(set) var slowmode: Slowmode? {
        didSet { awaitCooldownEnd() }
    }
    public private(set) var submitting = false
    public private(set) var length: Int
    public private(set) var lengthLimit: Int

    @ObservationIgnored private let account: Account
    @ObservationIgnored private let store: Store
    @ObservationIgnored private let drafts: Drafts
    @ObservationIgnored private var guildId: GuildId?
    // Discord's hasClickedForm: the form was opened since the title was last empty. Until then
    // the title is only what the search field holds.
    private var begun: Bool {
        didSet { drafts.posts[forumId, default: .init()].begun = begun }
    }
    @ObservationIgnored private var template = ""
    @ObservationIgnored private var cooldownEnd: Task<Void, Never>?

    init(forumId: ChannelId, account: Account, store: Store, drafts: Drafts) {
        self.forumId = forumId
        self.account = account
        self.store = store
        self.drafts = drafts
        let channel = store.channel(id: forumId)
        guildId = channel?.guildId
        isMedia = channel?.kind == .guildMedia
        template = Self.template(of: channel)
        let draft = drafts.posts[forumId]
        title = draft?.title ?? ""
        tags = draft?.tags ?? []
        begun = draft?.begun ?? false
        message = draft?.message ?? template
        length = Self.length(of: draft?.message ?? template)
        lengthLimit = Int(store.messageLengthLimit())
        readRules()
        slowmode = store.threadSlowmode(parentId: forumId)
        awaitCooldownEnd()
        // As the official client: a message begun earlier opens the form with the page.
        isOpen = canCreate && hasMessage && typedMessage != template
        if isOpen {
            begun = true
        }
    }

    /// Whether "New Post" does anything: the user may post and Akari can make the post.
    public var canCreate: Bool {
        allowed && !isMedia
    }

    /// Something typed or chosen that a closed form still holds: the row then shows the pencil
    /// and a click reopens the form. What a search typed is none.
    public var hasDraft: Bool {
        begun && (!typedTitle.isEmpty || !tags.isEmpty || (hasMessage && typedMessage != template))
    }

    /// REQUIRE_TAG with nothing chosen, once there is a title and a message to post.
    public var tagMissing: Bool {
        requiresTag && tags.isEmpty && !typedTitle.isEmpty && hasMessage
    }

    /// Whether "Post" is on.
    public var canPost: Bool {
        canCreate && !submitting && slowmode?.until == nil && !typedTitle.isEmpty && hasMessage
            && !tagMissing
    }

    /// Characters left once 200 or fewer remain (negative past the limit), else `nil`.
    public var remaining: Int? {
        let left = lengthLimit - length
        return left <= 200 ? left : nil
    }

    /// A tag's pill is off once five others are chosen.
    public func canChoose(_ tag: TagId) -> Bool {
        tags.contains(tag) || tags.count < Self.tagLimit
    }

    public func toggle(tag: TagId) {
        guard !submitting else {
            return
        }
        if let index = tags.firstIndex(of: tag) {
            tags.remove(at: index)
        } else if tags.count < Self.tagLimit, usableTags.contains(where: { $0.id == tag }) {
            tags.append(tag)
        } else {
            return
        }
        drafts.posts[forumId, default: .init()].tags = tags
    }

    /// "New Post" and its kin: the title is what the field holds.
    func open() {
        guard canCreate else {
            return
        }
        begun = true
        isOpen = true
    }

    /// Escape: the form closes and keeps what was typed.
    public func close() {
        guard !submitting else {
            return
        }
        isOpen = false
    }

    /// The ×: nothing typed or chosen is left.
    public func discard() {
        guard !submitting else {
            return
        }
        reset()
    }

    public func dismissAlert() {
        alert = nil
    }

    /// Checks what's typed and creates the post; returns it, or `nil` when nothing was created
    /// (the fields' errors or `alert` say why, and everything typed stays).
    func create() async -> ChannelId? {
        guard !submitting, canCreate else {
            return nil
        }
        titleError = typedTitle.isEmpty ? Self.titleRequired : nil
        messageError = hasMessage ? nil : Self.messageRequired
        guard titleError == nil, messageError == nil, !tagMissing else {
            return nil
        }
        let content = typedMessage
        let length = Self.length(of: content)
        guard length <= lengthLimit else {
            messageError =
                ComposerModel.Problem.tooLong(by: length - lengthLimit, limit: lengthLimit).text
            return nil
        }
        guard slowmode?.until == nil else {
            return nil
        }
        submitting = true
        defer { submitting = false }
        do {
            let post = try await account.createPost(
                forumId: forumId, name: typedTitle, tagIds: tags, content: content)
            slowmode = store.threadSlowmode(parentId: forumId)
            reset()
            return post
        } catch {
            slowmode = store.threadSlowmode(parentId: forumId)
            refused(error as? CreateThreadError)
            return nil
        }
    }

    /// The forum's page shows again: what changed while it was closed.
    func reread() {
        readRules()
        let read = store.threadSlowmode(parentId: forumId)
        if read != slowmode {
            slowmode = read
        }
        lengthLimit = Int(store.messageLengthLimit())
    }

    func apply(_ batch: EventBatch) {
        let guildChanged =
            guildId.map { batch.guildsChanged.contains($0) || batch.membersChanged.contains($0) }
            ?? false
        let channelChanged = batch.channelsChanged[guildId]?.contains(forumId) == true
        if batch.everythingChanged || guildChanged || channelChanged {
            readRules()
            let read = store.threadSlowmode(parentId: forumId)
            if read != slowmode {
                slowmode = read
            }
        }
        if batch.everythingChanged || batch.currentUserChanged {
            lengthLimit = Int(store.messageLengthLimit())
        }
    }

    private var typedTitle: String {
        title.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var typedMessage: String {
        message.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var hasMessage: Bool {
        !typedMessage.isEmpty
    }

    private func reset() {
        isOpen = false
        begun = false
        title = ""
        tags = []
        message = template
        titleError = nil
        messageError = nil
        drafts.posts[forumId] = nil
    }

    private func readRules() {
        let channel = store.channel(id: forumId)
        let creation = store.postCreation(forumId: forumId)
        let available = channel?.forum?.tags ?? []
        let usable = creation.tags.compactMap { id in available.first { $0.id == id } }
        if usable != usableTags {
            usableTags = usable
        }
        // A tag the forum dropped, or one the user may no longer apply.
        let kept = tags.filter { id in usable.contains { $0.id == id } }
        if kept != tags {
            tags = kept
            drafts.posts[forumId, default: .init()].tags = kept
        }
        if creation.allowed != allowed {
            allowed = creation.allowed
        }
        if creation.requiresTag != requiresTag {
            requiresTag = creation.requiresTag
        }
        if !canCreate, isOpen {
            isOpen = false
        }
        // A new template replaces the old one only where the user typed nothing over it.
        let next = Self.template(of: channel)
        if next != template {
            let untouched = typedMessage == template
            template = next
            if untouched {
                message = next
            }
        }
    }

    private func refused(_ error: CreateThreadError?) {
        switch error {
        case .Slowmode?:
            break
        case .MaybeCreated?:
            alert = Alert(
                title: "Post may exist",
                message: "Discord didn't confirm the post, but it may have been created: "
                    + "check the forum before trying again.")
        case .TooManyThreads?:
            alert = Alert(
                title: "Cannot create post",
                message: "This server has reached the maximum number of active posts. Free up "
                    + "space by archiving a post.")
        case .NameBlocked(let reason)?:
            titleError = reason.isEmpty ? ThreadFormModel.blocked : reason
        case .MessageBlocked(let reason)?:
            messageError = reason.isEmpty ? ThreadFormModel.blocked : reason
        case .NameRequired?:
            titleError = Self.titleRequired
        case .TagRequired?:
            readRules()
            requiresTag = true
        case .Request(.RateLimited)?:
            alert = Alert(title: "Cannot create post", message: "You are being rate limited.")
        case .Request?, nil:
            alert = Alert(title: "Error", message: "An error occurred, please try again.")
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
            slowmode = store.threadSlowmode(parentId: forumId)
        }
    }

    private static func template(of channel: Channel?) -> String {
        channel?.forum?.template?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
    }

    private static func length(of text: String) -> Int {
        text.isEmpty ? 0 : Int(messageLength(content: text))
    }
}

/// What a forum's post form holds while its page is closed.
struct PostFormDraft: Equatable {
    var title = ""
    var tags: [TagId] = []
    var message: String?
    var begun = false
}
