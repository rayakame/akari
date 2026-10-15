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
own settings mute now, in one read; a muted category or guild doesn't mute its channels.
`ServerListChanged` says to read `server_list` again, `GuildSettingsChanged` to reread a
guild's settings and mutes; it also comes when a timed mute ends. Category collapse isn't
here: Discord's clients don't sync it, so each app keeps its own.

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
previews.

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
  `view_channel` only signal.
- The host's `TokenStore` runs on a blocking thread, never on the main thread or a runtime
  worker, so it may block or show a dialog.
- Markdown is parsed on the runtime where it can be: each account registers its documents as
  akari-core's message preparer, so history pages and the disk cache's fill are parsed on a
  blocking thread before they enter a window, and `messages` only looks them up. Live
  changes (new, edited, confirmed messages) are parsed when read, on the reader's thread: a
  typical message takes microseconds. Those count as event parses; a parse on read of something
  a load had parsed is a miss and logs one debug line at `akari_ffi::content` (the channel ID
  and the count).
- **Swift task cancellation doesn't reach Rust** (UniFFI 0.32 doesn't support it). A request
  runs to completion; a send that the host stopped waiting for still resolves through the
  store. Waits end through explicit calls: `PasswordLogin.cancel()`, `QrLogin.cancel()`,
  `StoreSubscription.close()`, `Account.close()`.

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

## Measuring loads

`load_messages` returns a `LoadReport` (or `None` when there was nothing to load): the time
behind earlier requests and waiting for rate limits, until the headers, for the body,
decoding, parsing (`parse`, `parsed`: the page's new messages and, on a first page, the disk
cache's) and storing, whether a new connection was opened, and Discord's rate-limit headers.
Numbers only; the apps log them. `DiscordClient::warm_up()` opens a REST connection ahead of
the first request (an unauthenticated `GET /gateway`) and returns at once.

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
