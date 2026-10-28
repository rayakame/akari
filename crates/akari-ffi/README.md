# akari-ffi

UniFFI bindings over `akari-core` and `akari-markdown` for Swift (macOS now, iOS later) and
Kotlin (Android, later). This crate holds bindings only: logic belongs in `akari-core` and
`akari-markdown`, and Apple-specific view-model behavior in AkariKit (`apps/apple/AkariKit`).

## How state crosses the boundary

- **Objects** (UniFFI objects, Swift classes holding an `Arc` handle) are the things with
  identity and behavior: `DiscordClient`, `Token`, `PasswordLogin`, `QrLogin`, `Account`,
  `Store`, `StoreSubscription`. Every method call costs a handle clone and two FFI calls.
- **Records** (Swift structs) are single items, copied once per read: `User`, `Guild`,
  `Channel`, `Message`, `MessageWindow`. The host keeps its copy until an event says the
  item changed, so views render from Swift values and never call into Rust per render.
  Messages and channels are deliberately not objects: a table cell reading three fields
  would cross the boundary three times on every configure.
- **Events carry IDs only**, plus the IDs needed to route them (a channel's guild, a
  message's channel). A subscriber pays only for what it shows, repeated changes to one
  item in a batch collapse into one read, and a view model always converges to the store's
  state at batch time.
- **Reads by ID** come in single and batch form (`channel(id)`, `channels(ids)`,
  `messages(channel_id, ids)`, `muted_channels(ids)`, `channel_badges(ids)`); lists cross as
  ID lists (`guild_ids`, `channel_list`, `window`), or as `server_list`'s entries (a guild ID,
  or a folder with its guild IDs).

## Settings

`server_list()` is the rail in the user's order, with their folders (`ServerListItem`:
`Guild` or `Folder`, whose `name` is `None` when the user gave none). `guild_ids()` is the same
list with every folder's guilds in its place. `guild_settings(guild_id)` (`None`: the DM
settings) says whether the guild is muted now, when that ends, and its suppress and "hide
muted channels" settings. `muted_channels(ids)` returns the channels among `ids` that their
own settings mute now, in one read, and the threads muted by the user's membership; a muted
category or guild doesn't mute its channels, nor a muted channel its threads.
`ServerListChanged` says to read `server_list` again, `GuildSettingsChanged` to reread a
guild's settings and mutes; it also comes when a timed mute ends. Category collapse isn't
here: Discord's clients don't sync it, so each app keeps its own.

`display_settings()` is how media shows and animates (`DisplaySettings`: spoilers, inline
media, GIF autoplay, embeds, emoji and sticker animation, the explicit content filter and
redactions), with Discord's defaults where the user set nothing;
`DisplaySettingsChanged` says to read it again ([user-settings.md](../../docs/protocol/user-settings.md)).

## Badges and read states

`channel_badges(ids)` and `guild_badges(ids)` return a `Badge` (`unread`, `mentions`) per ID,
in the order given, in one read: what a channel row, a server icon or (added up by the host)
a folder shows. The core keeps them current, so reads compute nothing. A channel the user
can't view, or a category, has `Badge { unread: false, mentions: 0 }`; a muted guild's
`unread` is false and its mentions stay. `read_state(channel_id)` is where the user stopped
reading (`acked`, `mentions`), `None` while there is none; one Akari started because Discord
sent none is `estimated`, and its `acked` isn't a message the user read.
`unread_private_channels()` lists the DMs and group DMs with mentions, newest message first,
for the rail. `BadgeChanged { channel_id, guild_id }` says to reread that channel's badge or
read state, and `GuildBadgeChanged { guild_id }` that guild's badge and its folder's.

## Threads

`channel_list(guild_id)` puts each channel's listed threads right after it, the last joined
first: those the user joined that aren't archived and are inside their "Hide After Inactivity"
window, unread, or with a mention. A thread's `Channel` carries `thread: ThreadInfo` (joined,
archived, locked, private, its message count, owner, newest message ID, creation time, member
count, a preview of its member IDs and when it was last archived). The
count moves with every message in the thread without `ChannelUpdated`: `MessageUpdated` for
the row that shows the thread's box (the message with `has_thread` and the thread's ID) says
when to read it again, and so does a change of the thread's newest message,
`Store.latest_message(thread_id)` (from THREAD_LIST_SYNC, live messages and the thread's
newest page, for the session; `None` while none is known). `ThreadListingChanged { guild_id,
thread_id }` says to read the list again when a thread left or rejoined it without changing
itself (its window ended, it was read past it, a message came after it); joining, leaving and
closing come as `ChannelUpdated`. Which listed threads a list hides (under a collapsed
category, a muted parent, "hide muted channels") is the host's rule.

A `Message` carries its `reference` (what a reply, pin, forward or thread notice points at; a
"started a thread" message's `channel_id` is its thread) and `has_thread`, set when a thread was
started from it. For system lines it also carries its `mentions`, a call's `call` (participants,
end), `role_subscription`, `purchased_product`, a bot join's `actor`, `application_name`, and the
first embed's `embed_fields` (a poll result's and an AutoMod alert's data). `starter_message(thread_id)` reads a thread's first message when it started
from a message in its channel: `Loaded { message }` with the parent's `Message`, `Loading`
while the core fetches it (Discord sometimes leaves it out), or `Unavailable` when it was
deleted or can't be read; `None` until the thread's window holds its first message. When it
settles, `MessageUpdated` names that first message.

`Account.fetch_thread(parent_id, thread_id)` fetches a thread a "started a thread" line in
`parent_id` names when the store doesn't have it (archived, or never synced) and returns whether
the store has it now. When Discord refuses it or it is gone, `Store.thread_unavailable(thread_id)`
says so for the session, and `StoreEvent::ThreadUnavailable { guild_id, thread_id }` tells the
lists of the parent's guild to read the lines that name it again. A later READY keeps a thread
fetched this session, and every thread the user hasn't joined. `Account.check_thread(parent_id,
thread_id)` asks even when the store has the thread (a split view remembered from an earlier
session, which the cache may have kept after it was deleted); when Discord refuses it or it is
gone, the store also drops it, with `ChannelRemoved`.

`Account.join_thread(thread_id, location)` and `leave_thread(thread_id, location)` join and
leave, telling Discord where the user chose it (`ThreadLocation`: `SidebarOverflow`,
`ToolbarOverflow`, `ContextMenu`); the store has it joined or left once Discord says yes, which
comes as `ChannelUpdated`. An archived thread, or one the store doesn't have, fails with
`InvalidRequest` before asking.

`Account.create_thread(parent_id, message_id, name, is_private, location)` creates a thread as
the official client does, from the message when one is given (never private then), telling
Discord where the user started it (`CreateThreadLocation`: `Message`, `ThreadBrowserToolbar`,
`ThreadBrowserEmptyState`, `KeyboardShortcut`), and returns its ID once the store has it from
Discord's answer (`ChannelAdded`); the first message is an ordinary send in the thread. What may
be created, and from which messages, are `Store.thread_creation(parent_id)` (`ThreadCreation {
public, private }`) and `Store.can_start_thread(channel_id, message_id)`; anything else fails
with `CreateThreadError::Request { error: InvalidRequest }` before asking, as does a name that is
empty or over 100 characters once trimmed. Creating threads has its own slowmode in the parent,
`Store.thread_slowmode(parent_id)`: each created thread starts it, Discord's slowmode answer
holds it, and while it runs `create_thread` fails with `Slowmode` without asking. Discord's other
refusals are `TooManyThreads { announcement }`, `NameBlocked { reason }` (AutoMod) and
`NameRequired`; `MaybeCreated` says the answer was lost after the request left (the thread may
exist; the slowmode starts as if it did). Creating needs Send Messages in Threads too.

`Store.thread_browser(parent_id)` is what a channel's Threads browser lists: the active threads
the user joined and those it hasn't (only ones this connection's gateway, or a request, brought:
a thread kept across a READY may have been archived or left meanwhile), and the archived ones
the loaded pages reach, each the newest message first, with `older: OlderThreads` (`NotLoaded`,
`Loading`, `More`, `All`, `Failed`). `Account.load_older_threads(parent_id)` loads the next page
of 25 as Discord's client asks for it; nothing while one loads, after the last, or after a
failure or Discord's "not indexed yet" until the next connection. `Account.search_threads(parent_id,
name)` returns Discord's first 25 matches by name, active and archived, in its order (`None`
while Discord hasn't indexed the channel), and keeps them in the store. `ThreadActivity {
guild_id, parent_id, thread_id }` comes when a thread's newest or latest message changes, so a
browser reorders, `OlderThreadsChanged { parent_id }` when its archived pages move on (began,
came, failed, given up); its threads' `ChannelAdded`, `ChannelUpdated` and `ChannelRemoved` say
the rest. `Account.set_browsing(parent_id)` names the channel whose browser is open (`None` once
it closes): a new connection lets go of the unjoined archived threads nothing shows (no window,
box or "started a thread" line), but not the open browser's. A page or search asked before a
READY confirms and joins nothing.

Acks: `set_reading(channel_ids)` names the channels the user is reading at their newest
messages: none, one, or two in the split view (a channel and the thread beside it; further ones
are ignored). `mark_read(channel_id)` and `mark_guilds_read(guild_ids)` mark read on request
(Escape, context menus). All three return at once, and the core sends the acks in the
background. `mark_read` and `mark_guilds_read` clear the badges at once: the events fire before
Discord answers. `set_reading` clears a channel's only when its list, at the present and not
stale, shows messages past the read position (or the channel has mentions), and only up to the
newest message it shows; it reads again as that list grows. An empty list clears nothing. The
core stops reading on its own when the connection drops, on `disconnect()` and when the account
closes: set the channels again once the session is online and their lists are refreshed. An ack
Discord refuses can bring a channel's badge back (`BadgeChanged`). The host
decides when the user is reading; the core never infers it
([read-states.md](../../docs/protocol/read-states.md#how-akari-acks)).

## Formatted messages

`Message.formatted` is the content as Discord formats it (`FormattedText`), flat so it maps
onto an attributed string: blocks (paragraph, heading, subtext, code block) inside their
quotes and list items, each with spans that carry their styles, spoiler and link, and the
links (masked or not, the title, the embed flag, and for Discord channel and message URLs
the typed IDs and the channel's name). Offsets are UTF-16 code units into `content`, so Swift
cuts it with `NSString` ranges. `Message.mentions_me` says whether the message mentions the
current user (akari-core's rule; suppress settings don't change it, as in the official client,
which drops only the badge).

Mention names come from the message's own users, else the store, at read time, so a rename
needs no invalidation: `Known`, `NoAccess` (a channel the user can't view), `Missing` (the
store has the list but not the item) or `NotLoaded` (before READY, a pending message's users,
a guild not in the store). `formatted_text(content)` parses without a store, for tests and
previews. A custom emoji span (`SpanKind::Emoji`) carries its `ImageKey`, so the text's layer
asks for it like any image; `Message.stickers` are `Sticker` records (ID, name, format and the
sticker's key). `FormattedText.jumbo` follows Discord's limit of 30 emoji. For their popouts,
`Account.emoji_source`, `sticker`, `sticker_guild` and `sticker_pack` ask Discord where an emoji or
sticker is from (`EmojiSource`, `StickerDetails`, `SourceGuild`, `StickerPack`), `Store.emoji_guild`
names the user's own server an emoji is from, `Store.nitro()` the user's tier, and
`emoji_shortcode` a Unicode emoji's name.

`Message.attachments` are in the order they show (the mosaic's images and videos, then the
files; thumbnails left out), each an `Attachment` with its `kind`, pixel size (0 unknown),
`placeholder` thumbhash, `spoiler` and `animated` flags, alt text, the `key` its image (a video's
poster) is asked by, its `display` (`Media` or `File`) and its `cover` (`Sensitive`, `Pending`,
`Spoiler`), decided by the core from the user's settings, the conversation and Discord's scan;
`Message.spoilers_shown` says the channel's spoilers show without a click. Covers can change
without a message event: a host rereads its messages after `DisplaySettingsChanged` and `Ready`,
and the core sends `MessageUpdated` when a scan's wait ends. `Store.age_gate(channel)` says
whether a channel waits behind the age gate; `Account.agree_to_age_restricted(guild)` is
Continue, and `AgeGateChanged` follows it.

Names and `mentions_me` can change without a message event, so a host rereads the messages it
shows after `GuildUpdated` (role names and colors), `ChannelUpdated` (channel names, access),
`UserUpdated`, `CurrentUserUpdated` and `CurrentMemberUpdated` (the user's roles: their
mentions and channel access).

Each account keeps up to 4,096 parsed documents, keyed by message ID and edit time, with a
hash of the content as a guard (SipHash under a random key per process), since an update can
change the content without an edit time. `Store.content_stats()` counts them: parsed, `misses` (parsed
when read though a load had parsed them, because the cache dropped them since) and
`event_parses` (parsed when read because no load did: live messages and edits, also those the
store held back while a page loaded).

## Events

Subscribe before the first read: `store.subscribe()`, then the reads, then apply each
batch. `StoreSubscription.next()` returns everything buffered, at most 256 events, or waits
for the next one. A host runs one loop per session: on Apple a `@MainActor` task that
awaits `next()` and applies each batch synchronously before asking for the next, which
keeps the order. Nothing is dropped: the store buffers unread events, and `next()` is polled
directly, so it only takes events in the poll that returns them.

`next()` returns `nil` after `close()`, or once the account is closed and every event was
read.

## Threads and the runtime

- akari-ffi owns one multi-threaded tokio runtime (2 workers named `akari`), started by the
  first `DiscordClient` and never shut down: accounts and running requests can outlive any
  one client, and a runtime can't be dropped from its own threads. UniFFI's own
  `async_runtime = "tokio"` isn't used; it polls inside async-compat's single-threaded
  runtime, where akari-core's background tasks would land.
- Request-style async methods (`submit`, `load_messages`, `send_message`, `save_token`, …)
  spawn onto the runtime and await the result. Pull-style ones (`QrLogin.next`,
  `StoreSubscription.next`) are polled directly.
- Sync methods are cheap. A store read takes one read lock, and akari-core warns when a
  write holds the lock for more than 4 ms. `connect`, `disconnect`, `close` and
  `view_channel`, `set_reading`, `set_browsing`, `mark_read` and `mark_guilds_read` only
  signal.
- The host's `TokenStore` runs on a blocking thread, never on the main thread or a runtime
  worker, so it may block or show a dialog.
- Markdown is parsed on the runtime where it can be: each account registers its documents as
  akari-core's message preparer, so history pages and the disk cache's fill are parsed on a
  blocking thread before they enter a window, and `messages` only looks them up. Live
  changes (new, edited, confirmed messages) are parsed when read, on the reader's thread: a
  typical message takes microseconds. Those count as event parses; a parse on read of something
  a load had parsed is a miss and logs one debug line at `akari_ffi::content` (the channel ID
  and the count).
- **Swift task cancellation doesn't reach Rust** (UniFFI 0.32's Swift glue awaits without a
  cancellation handler and never cancels the Rust future). A request runs to completion; a
  send that the host stopped waiting for still resolves through the store. Waits end through
  explicit calls: `PasswordLogin.cancel()`, `QrLogin.cancel()`, `StoreSubscription.close()`,
  `Account.close()`, and for images `PendingImage.cancel()` and `PendingSave.cancel()`
  (below), which AkariKit's wrappers call from the task's cancellation handler.

## IDs, errors and tokens

- IDs cross as `u64` custom types. Swift lifts them into typed `Snowflake<Marker>` values
  (`UserId`, `GuildId`, `ChannelId`, …; see `uniffi.toml` and
  `apps/apple/AkariKit/Sources/AkariFFI/Snowflake.swift`), so one kind of ID can't be passed
  for another.
- Each akari-core error has a mirror here with the variants and data the UI needs: Discord's
  code and message, `retry_after`, the captcha challenge. Network errors keep only their
  kind. Display texts are akari-core's safe-to-show wording; Swift gets them as
  `localizedDescription`. A Rust panic reaches the host as UniFFI's internal error.
- A token is an opaque `Token` object; the host can only hand it back (`save_token`,
  `account`, `end_session`). `end_session` ends that token's session on Discord without
  touching the token store, so an app can delete the stored token first and end the session
  afterwards, in the background. Only the host's `TokenStore` sees the string, to put it into the platform's
  secret store. Swift's default printing of a struct includes every field, so a string in a
  login result would end up in logs.

## Send rules

The length limit and slowmode are akari-core's rules, so every app applies them the same way:
`message_length(content)` counts code points as Discord does, `Store::message_length_limit()`
gives the current user's limit, and `Store::slowmode(channel_id)` says whether the channel has
slowmode, whether the user is exempt and when they may send again. `send_message` refuses a
message over the limit with `RequestError::TooLong` before anything is queued, and Discord's own
refusal for length maps to the same error. No event reports a cooldown; re-read `slowmode` on
the open channel's window events and after a send.

## Jumps

`Latest` and `Around` loads place a channel's list: at the present, or around a message. When
such requests for one channel overlap, the one the host made last wins. `reserve_placing()`
returns the next number in that order; a host takes it where the user acts and passes it to
`load_messages(…, placing)`, since the load itself begins later, on Akari's runtime (`None`
takes a number when the load begins). `supersede_around_loads(channel_id)` takes one without
loading, for Escape, a mark as read, a send, leaving the channel or opening it again during a
jump; both return at once. The core drops an `Around` page when a newer request came after it,
and a `Latest` page that would replace the window while a newer `Around` load is the channel's
last request (the page still raises the channel's newest message). `LoadReport.outcome` says what
became of a page: `Applied`, `Overtaken` (a newer request came first) or `Outdated` (the window
was replaced, or the end the page continues from changed, meanwhile). A jump has happened only
once its page is `Applied`. The rules are in
[messages.md](../../docs/protocol/messages.md#loading-history).

## Measuring loads

`load_messages` returns a `LoadReport` (or `None` when there was nothing to load): what became
of the page (`outcome`, above), the time behind earlier requests and waiting for rate limits,
until the headers, for the body, decoding, parsing (`parse`, `parsed`: the page's new messages
and, on a first page, the disk cache's) and storing, whether a new connection was opened, and
Discord's rate-limit headers. The apps log them. `DiscordClient::warm_up()` opens a REST
connection ahead of the first request (an unauthenticated `GET /gateway`) and returns at once.

## The disk cache

`DiscordClient`'s constructors take a `cache_directory`; without one nothing is written. In it
akari-core keeps one SQLite file per account ([cache.md](../../docs/protocol/cache.md)).

- `preload(user_id)` starts reading an account's file in the background, e.g. beside the token
  read; it returns at once.
- `account(token, user_id)` restores that account's cached state as soon as it's read, before
  the gateway connects: `StoreEvent::Restored`, after which the host reads everything as after
  `Ready`. It never waits for the disk.
- `Account::flush_cache()` writes what changed now, e.g. before the app quits; the core also
  writes every 10 s while the state changes, and on `disconnect()` and `close()`.
- `forget_account(user_id)` deletes the stored token, then the account's cache, also while the
  account is open; `logout` deletes it too. `clear_caches()` deletes the files of every account
  that isn't open, e.g. at a login screen.

All SQLite work runs on the cache's own thread, never on the runtime's workers or the caller's.

## Images

`DiscordClient`'s constructors take an `image_directory` too (on Apple, a folder in the caches
directory); without one accounts have no images. akari-core keeps each account's images in a
directory of its own there, downloads them without the token, renews signed attachment links
and evicts past 256 MB ([cdn.md](../../docs/protocol/cdn.md)). No decoded image crosses the
boundary: a file's path does. The one image that crosses as bytes is a placeholder's few RGBA
pixels.

- An image is named by an `ImageKey` (Discord's IDs; hashes as Discord sends them).
  `ImageRequest` adds the box's pixels (points times the backing scale, rounded up) and
  `animated` (`None` for the still, else the `AnimatedFormat` the host's decoder shows right).
- Records carry their keys, so every view asks the same way: `User.avatar` is the avatar, or
  one of Discord's default avatars for a user without one (the client's index rule: a bot
  with "0000" and no avatar is 0, a legacy tag is `tag % 5`, else `(id >> 22) % 6`; a hash
  that isn't the CDN's counts as none); `Guild.icon` and `Channel.icon` (group DMs) are
  `None` without one. Hosts draw no default of their own.
- `Account.request_image(request)` returns a `PendingImage` at once. `result()` waits for the
  `ImageFile` (path, the file's pixels, `animated`, `from_disk`); every call, also a second or
  a concurrent one, gets the same answer. `cancel()` stops a request that hasn't answered:
  `result()` is then `Cancelled`, and the download stops when nobody else waits for it; an
  answer that has arrived is still returned. Releasing the object cancels only a request nobody
  is waiting for: a running `result()` holds the object (UniFFI moves the `Arc` into the call),
  so a Swift task's cancellation must call `cancel()` itself. AkariKit's `Account.image(_:)`
  does that with `withTaskCancellationHandler`; use it rather than `result()`.
- A file stays at least a minute after it's handed out; one gone at decode (the system purged
  it) is asked for again.
- `ImageError` says why there's none: `NotFound` (remembered for the session), `Unavailable`
  (offline or failing: ask again online), `TooLarge`, `NotAnImage`, `UnknownKey` (the store
  doesn't have the message), `Unsupported` (Lottie stickers, other hosts), `Io`, `Closed`,
  `Cancelled`, and `Failed` (the request panicked inside Akari: a bug, never a cancel).
- `Account.original_url(key)` gives a link for Open in Browser and Copy Link (attachments'
  renewed when due). It is the one signed link that leaves the core, for the user's own action:
  the host hands it to the browser or the clipboard at once and never logs or keeps it.
  `Account.save_original(key, destination)` returns a `PendingSave` (`result()` is the bytes
  written, `cancel()` as above) that streams the original into a file beside `destination` and
  moves it into place once it's whole: a failed or cancelled save leaves what was there. Saves
  have slots of their own (two), so a large original never holds up a row's images.
  `Account.clear_images()` empties the account's directory. Logout, `forget_account` and
  `clear_caches` delete images as they delete the cache.
- `placeholder_image(thumbhash)` decodes an attachment's placeholder as Discord's client
  does, into at most 32 × 32 RGBA.
- Layout from sizes alone (points), so a row's height is final before anything loads:
  `media_contain`, `media_cover`, `media_box`, `media_mosaic`, `media_fit_width`,
  `sticker_size`, `embed_image_size`, `embed_thumbnail_size`, `gallery_columns`,
  `gallery_card_height`.

## Changing the API

- A read: a `Store` method that returns a record built with `From<&state::X>`; never a core
  type.
- An event: a `StoreEvent` variant with IDs, converted in `StoreEvent::new`.
- An error: mirror the core variant with its Display text, and keep the conversion
  exhaustive where the core enum allows it.
- Then run `apps/apple/build-ffi.sh`: the generated Swift isn't checked in.

## Building for Apple

`apps/apple/build-ffi.sh` builds `-p akari-ffi` (never the workspace, which would turn on
akari-cli's dev features) for `aarch64-apple-darwin` and `x86_64-apple-darwin`, generates
the bindings with `akari-bindgen` (UniFFI's generator at the workspace's version), and packs
`apps/apple/AkariKit/Frameworks/akari_ffiFFI.xcframework`. `--host-only` builds only this
Mac's architecture; `--debug` builds the library without optimizations.

The crate's only crate type is `staticlib`: with an rlib among a crate's outputs, Cargo skips
LTO, so the release profile's fat LTO wouldn't reach the library the apps link.

## Building for Android (later)

The Android app will need a `cdylib`. Build it as a separate crate type at that time, e.g.
`cargo rustc -p akari-ffi --release --crate-type cdylib`, instead of adding `cdylib` to the
crate's crate types, so the Apple staticlib keeps its own LTO build.
