# AkariKit

Shared Swift package with the logic and view models used by both the macOS and iOS
apps, built on the Swift bindings from `akari-ffi`.

## Building and testing

The package needs akari-ffi's XCFramework and generated Swift bindings, which aren't checked
in. Build them first (needs full Xcode), then test:

```sh
apps/apple/build-ffi.sh             # release, Apple silicon and Intel
apps/apple/build-ffi.sh --host-only # just this Mac's architecture, for local iteration
swift test --package-path apps/apple/AkariKit
```

Run the script again whenever akari-ffi changes. Both outputs are build products: the
generated Swift and the library must come from the same akari-ffi build, because UniFFI
compares checksums when the library loads, so committing either would only let them drift.

## Targets

- `akari_ffiFFI`: the XCFramework (static library, C header and module map).
- `AkariFFI`: the generated `akari_ffi.swift` plus hand-written value types: typed IDs
  (`Snowflake<Marker>` with `UserId`, `GuildId`, `ChannelId`, …), the `Permissions` bits, and
  akari-core's error texts as `LocalizedError`. The generated code stays in its own target
  because it doesn't compile with MainActor as the default isolation.
- `AkariKit`: re-exports `AkariFFI` and adds the platform glue (`KeychainTokenStore`,
  `HostInfo.current`, `AccountMemory`) and the view models.

How state crosses the boundary is described in `crates/akari-ffi/README.md`.

## View models

All of them are `@MainActor @Observable` classes. They hold Swift copies of what is on
screen, so views render from them without calling into Rust.

- `AppModel` restores the last account or shows the login screen. `AccountMemory` keeps only
  the user ID; the token stays in the Keychain, and the open session's `Token` object stays in
  memory. After a login it saves the token and opens the session. A session that ends with
  `AuthenticationFailed` returns to the login screen and deletes the token and the account's
  cache with `forgetAccount`, without sending the token to Discord again. Any other close
  keeps the session on screen; `reconnect()` opens a new one from the token in memory, with
  the same drafts; the old account's pending and failed messages become drafts of their
  channels.
  `suspend()` and `resume()` disconnect and reconnect the open session around sleep.
  A restored launch and a wake call `client.warmUp()` first, so the first message load finds
  an open REST connection; a restored launch also calls `client.preload(userId:)`, so the disk
  cache is read while the token is. A launch without an account calls `clearCaches()` in the
  background. `prepareToQuit(limit:)` writes the open session's cache and returns within the
  limit (1 s by default) either way.
  `logOut()` runs once: it deletes the token and the cache with `forgetAccount` first, and if
  that fails it stays on the session and sets `logoutError`; otherwise it shows the login
  screen and ends the session on Discord in the background (`endSession(token:)`), setting
  `warning = .logoutNotConfirmed` if Discord doesn't confirm. A new login can't lose its token
  to a late logout, since the Keychain item is gone before the login screen shows.
- `LoginModel` runs the QR code login beside the email/password form (MFA, SMS, new
  location); whichever finishes first logs in and cancels the other. A captcha shows as not
  supported yet.
- `SessionModel` holds the connection, the current user, the open place (home or a guild) and
  the channel last opened in each guild and at home. The disk cache's state (`restored`, or a
  store already restored when the session starts) shows before READY: everything is read as
  after READY and the last place and channel reopen once. The first READY then checks that
  place and falls back as it would at a READY restore (a gone guild to home, a hidden channel
  to the guild's first one, a gone conversation to nothing). It owns `ServerListModel` (the
  server list in the user's order: `items` are servers and folders, `guilds` the same servers
  with folders opened; a folder's expansion is Akari's own, kept per account in
  `AccountMemory`, and starts collapsed; `badges` holds each server's `Badge`, and
  `tileBadge(of:)` adds up a collapsed folder's, while an expanded one shows nothing),
  `DirectMessageListModel` (the DM list, the latest conversation first, with each one's
  recipients, name and `Badge`; `unread` holds the conversations with mentions, newest first,
  for the rail's icons under Home), `ChannelListModel` (the open guild's channels with their
  listed threads after each parent, their badges, which of them their own settings mute, and
  `shown(open:)`, which shows them as the official client does: the thread open full width
  (`setOpen(_:)`, called by the session) keeps a row at the top of its parent's threads while
  it isn't listed; a muted channel that isn't open shows only its unread and mentioned threads;
  with "hide muted channels" muted channels and threads are left out, except the open one and
  a channel with threads that show; and `staysWhenCollapsed(_:open:)`) and
  `MessageListModel` (the open channel's messages and loads; `failedLoads` keeps each kind of
  load's last failure; `lastLoad` is the last load's `LoadTrace` with akari-core's
  `LoadReport` and the model's read time, and `openedAt`/`openedFrom` say when the channel
  opened and where its first rows came from, for the `load` log; `revealedSpoilers` holds the
  spoilers revealed with `reveal(_:)` for as long as the model lives, so switching channels
  hides them again, as Discord does; rows are read again after the events that rename the
  users, roles or channels its mentions show). `go(to:)` opens a channel where it lives, in
  its guild or at home, for links and mentions that name another server's channel.
  `AccountMemory` keeps the last place and channel per account, and the session's
  first READY reopens them, as Discord does after a restart. `notice` is what a connection bar
  says (`ConnectionNotice`: connecting, reconnecting, offline after `suspend()`, or closed with
  its error), with the delay before a bar shows it.
- `ComposerModel` (`messages.composer`) holds the channel's draft, kept per channel for the
  session, its length as Discord counts it and the limit, whether the user may send there
  (`locked` for a locked thread, where only moderators can),
  the slowmode from akari-core, and the problem shown under the composer in Akari's words.
  `submit()` sends the trimmed draft unless it is blank, too long or held back by slowmode,
  and clears it only once the core has queued the message;
  `retry(_:)` and `discard(_:)` act on failed messages.
- `CollapsedCategories` (`session.collapsed`) keeps the collapsed categories per guild across
  launches; `visible(_:in:keeps:)` leaves out a collapsed category's channels except those the
  channel list keeps: the open channel, channels with mentions, and unread channels unless the
  category is muted (then only mentions stay, as in Discord); unread and mentioned threads
  stay, muted category or not, with their channel. Discord's clients don't sync
  collapse, so it stays Akari's own (per guild, shared by the accounts on this Mac), and Akari
  never collapses a category on its own.

The session replaces `messages` on its own when the open channel is deleted or the user can
no longer see it, and a new `MessageListModel` loads nothing until the view calls `open()`.
An open thread that stops being listed (left, closed, or read past its window) stays open,
as in the official client; only its deletion closes it. `openParent()` opens the open
thread's channel, for the thread header's link. `openBeside(thread:from:)` opens the thread a
box or a "started a thread" line in the open channel names beside it, in the split view (`split`,
with the thread's record in `splitChannel` for its header and menu), fetched first when the
store doesn't have it; nothing opens when Discord refuses it, it's gone or the fetch fails, nor
once the user opened something else or clicked another thread meanwhile. Each channel keeps its
split thread: opening another channel hides it, coming back reopens it from the store's window,
and `AccountMemory` keeps the threads per account, with each channel's server, across launches.
After READY a remembered thread READY didn't list as joined is checked once per session with
`Account.checkThread` (the cache keeps unjoined threads, deleted ones too); refused or gone
drops it, a failed request keeps it for the next launch, and an answer for a server the store
lacks is asked again. One request per thread runs at a time, which a box click shares. A
server the user leaves (not one down for an outage) takes its splits, at once or at the next
READY. `closeSplit()` (the X), a thread's row and the open channel's own row (`open(channel:)`)
forget it; `openInFullView()` only opens the thread full width, and the channel keeps its
split, as in the official client; `openInSplitView()` opens a full-width thread's channel with
the thread beside it; the thread's deletion closes it, and so does losing it: a private thread
the user is no longer a member of (without Manage Threads), or a thread whose channel they can
no longer see, which Discord announces with no THREAD_DELETE. `focusSplit` and `focusChannel` ask
the view to move the focus to the split's or the channel's composer (`splitFocused()`,
`channelFocused()`). The sidebar keeps the channel selected: the split thread never gets a kept
row. `reading` holds both lists while each is fresh at its bottom; scrolling one clearly away
stops only that one until the next click or key press, and so does a split the session shows on
its own (after a request or a batch, not with its channel or the user's click). `escape()` marks
both read, ends both lines and loads each present, or, with nothing to load or mark read in
either (as the official client's key handler, also with a list scrolled up), ends both lines,
closes the split and returns false. A row's `threadLinked` is false for such a line once the thread is unavailable. A
row's `threadBox` (`ThreadBox`) is the box of the thread started from its message, or that its
"started a thread" line is about: the name, the count as Discord words it, the second line
(`Bottom`: the newest message with its author, no recent messages, no messages yet) and the time
it's measured from; `ThreadBox.relativeTime` words that time ("25m ago") and says when it
changes, `placeholder(for:)` what an empty newest message shows. `membershipItem(for:)` gives a
thread of the open server's menus, or the split thread's, Join Thread or Leave Thread (disabled while archived);
`join(thread:from:)` and `leave(thread:from:)` send the menu's `ThreadLocation`, and a failure
waits in `membershipError`, in Discord's words, until `dismissMembershipError()`.
`threadsButtonParent` is the channel the header's Threads button browses (a text or
announcement channel, or a thread's parent of those kinds); `openThreadBrowser(of:as:)` opens
its `ThreadBrowserModel` as the popout or the modal ("See all threads"), which another channel or
`closeThreadBrowser()` closes; the core hears which channel's browser is open
(`Account.set_browsing`), so a reconnect keeps its threads. The browser's `sections` are Discord's ("9 joined threads", "2
other active threads", "older threads", empty ones left out) with rows (name; the newest
message, "No recent messages", "Started by" with the owner, or "Last active", each with its
time; faces from the member preview and "+N" or "50+"); `open()` loads the first page of
older threads (only while none is loaded) and `loadMore()` the next, with READ_MESSAGE_HISTORY
(`canSearch`); `setQuery(_:)` and `submit()` search on Enter, the latest text winning over a
search still running, `results` holding the found rows until the field is cleared, a failed
search changing nothing; `showsSpinner` and `isEmpty` say what shows instead.
`open(browsed:beside:)` is a row's click (full width, closing the channel's split) or
Shift+click (beside its channel), and `moreActiveThreads(of:)` the channel list's "More Active
Threads" (`ThreadPeek`): up to three active threads the list doesn't show; while `peek(at:)`
names the channel it shows for, `peekedThreads` follows the store. In a
thread, a row's `starter` is set for
its first message when it started from one in the channel: the row draws `shown`, the
parent's message, once loaded; `notice` says it couldn't be loaded; while it's fetched the
timeline leaves the row out. `MessageListModel.threadOwner` names who started the thread,
from the store or from a loaded message of theirs.
`MessageListModel.jump(to:)` is the jump a pin's "a message" makes: it loads the messages
around the message unless the loaded ones reach it and no other load is placing the list, then
counts a `messageJump` for the view to scroll to; the requests that cancel a jump to unread
cancel it too, and the later of the two jumps wins.
A list the session replaces, also on a channel switch, starts no more loads, and a jump to
unread it is still loading doesn't land. The view calls `open()` when the message area appears
and whenever `messages.channelId` changes, which is what `.task(id:)` does. When the area
reappears, the same channel is opened again; that views its window again, or loads the latest
page if the window is empty or stale, and either way a jump to unread that an earlier list of
the channel started doesn't land:

```swift
MessageList(model: messages)
    .task(id: messages.channelId) { await messages.open() }
```

### The event loop

`SessionModel.start()` subscribes to the store, reads, connects, then runs one loop on the
main actor. `next()` waits off the main actor, and each batch is applied synchronously
before the next `next()`: to the session first, then the guild, channel and message lists.
Every model sees the same events in the same order.

A batch becomes an `EventBatch`, the IDs it touched per kind. A list model re-reads its ID
list once when the batch touched it, then reads records only for new and changed IDs. Events
that a model's first read already shows change nothing, so a model created mid-stream
converges to the store without duplicates. `BadgeChanged` and `GuildBadgeChanged` make a list
read only those badges (`channelBadges`, `guildBadges`), and the DM list its `unread` order.

Swift cancellation doesn't reach Rust, so the loop closes the subscription when its task is
cancelled: by `close()`, or by releasing the session without calling it.

### Stable row keys

The app's message table diffs `MessageListModel.rows` by their keys. When Discord confirms a
pending message, its row keeps the pending ID as its key for as long as the message stays in
the window, and the row's message carries the confirmed ID. A confirmation then reloads that
one row instead of removing and inserting it.

## Presentation

Rules every Apple app shows the same way, so they live here rather than in the app:

- `MessageTimeline` turns `MessageListModel.rows` into table items: a divider before each
  local day and where author groups start, by the rule in `docs/ui/message-list.md` (a thread's
  starter and a message with a thread are groups of their own), wrapped
  in edge items when asked: `.edge(.older)` and `.edge(.newer)` where more history lies beyond
  the loaded messages (the app shows placeholder rows there), `.edge(.beginning)` where the
  channel begins. Without rows only `.edge(.older)` is left, standing in for messages that
  aren't loaded yet.
- `TimelineChanges` diffs two timelines by item keys into the removed, inserted and reloaded
  indexes a table or collection view applies, so a list never reloads as a whole. A
  confirmation keeps its row key (above) and reloads one row.
- `Message.formatted` (the parsed content, UTF-16 offsets into `content`) and
  `Message.mentionsMe` come from akari-ffi; the app renders them (`docs/ui/markdown.md`).
  `MentionText` says what a mention shows, including Akari's words for names that can't be
  resolved and the neutral placeholders shown before a server's lists load. `TimestampText`
  formats `<t:…>` timestamps in the system locale and time zone, gives the full date for
  their tooltip, and says when a relative one next reads differently. `TrustedLinks` keeps the
  hosts whose masked links open without asking, per account, in the session's memory
  (`SessionModel.trustedLinks()`); lists of one account share their hosts. Logging out and an
  expired session forget them with the account's spot (`AccountMemory.forget`), and a list
  made before that stores nothing more.
- `Message.notice(in:)` is a system message's sentence in Discord's wording and
  `noticeParts(in:)` the same in parts: the actor, other names and bold text (`strong`), the
  links Akari can follow (a thread, a message to jump to, a channel, a server), "threads",
  "pinned messages", the text between. Both take a `NoticeContext`: the reader, where the
  line is (channel, thread, forum post, group DM, DM) and what the store knows that the line
  names. `nil` is a message people write, an empty list a kind Discord shows no line for.
  A list's rows carry their context, `Row.isHidden` marks the empty ones, and the timeline
  leaves them out. `Row.authorName` is the header's name; it and the people a line names
  follow one rule, the display name (the global name, else the username); `Initials` gives the letters a
  server or avatar without an image shows; `Channel.opensMessageList` says which channels
  have a message list.

`LoadLog` writes the `load` category's lines (`-AkariLog load` on macOS): one per page with
where its time went (including `parse`, the runtime's markdown parse for the page), and one
per channel open. The formats are here, so every Apple app logs
the same lines; the app adds the render time it measures.

## Tests

`FFISmokeTests` use the real XCFramework without network. The view model tests use the fakes
in `Tests/AkariKitTests/Fakes`: subclasses of the generated classes made with UniFFI's
`init(noHandle:)`, which keep their state in memory and record every call and read. The
Keychain tests use the login keychain with a new service name per run and delete their items.

Settings in tests live in `MemoryDefaults`, never in a `UserDefaults` suite. Each suite a test
removes holds one of cfprefsd's file descriptors for seconds, and a loop of test runs once used
them all up: the real app's settings could then be neither read nor written until cfprefsd was
restarted. A test fails if a test file in AkariKit or the macOS app creates a suite.
