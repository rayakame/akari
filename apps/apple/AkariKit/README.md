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
  the user ID; the token stays in the Keychain. After a login it saves the token and opens
  the session. A session that ends with `AuthenticationFailed` returns to the login screen
  and deletes the token with `forgetToken`, without sending it to Discord again; `logOut()`
  logs out on Discord.
- `LoginModel` runs the QR code login beside the email/password form (MFA, SMS, new
  location); whichever finishes first logs in and cancels the other. A captcha shows as not
  supported yet.
- `SessionModel` holds the connection, the current user, the open place (home or a guild) and
  the channel last opened in each guild and at home. It owns `GuildListModel` (the server
  list), `DirectMessageListModel` (the DM list, the latest conversation first, with each
  one's recipients and name), `ChannelListModel` (the open guild's channels) and
  `MessageListModel` (the open channel's messages, loads, sends and whether the user may send
  there). `AccountMemory` keeps the last place and channel per account, and the session's
  first READY reopens them, as Discord does after a restart.
- `CollapsedCategories` keeps the collapsed categories per guild across launches; a collapsed
  category still shows the open channel.

The session replaces `messages` on its own when the open channel is deleted or the user can
no longer see it, and a new `MessageListModel` loads nothing until the view calls `open()`.
The view calls it when the message area appears and whenever `messages.channelId` changes,
which is what `.task(id:)` does. When the area reappears, the same channel is opened again;
that only views its window again, or loads the latest page if the window is empty or stale:

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
converges to the store without duplicates.

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
  local day and where author groups start, by the rule in `docs/ui/message-list.md`.
- `TimelineChanges` diffs two timelines by item keys into the removed, inserted and reloaded
  indexes a table or collection view applies, so a list never reloads as a whole. A
  confirmation keeps its row key (above) and reloads one row.
- `Message.notice` is Akari's sentence for a system message; `Initials` gives the letters a
  server or avatar without an image shows; `Channel.opensMessageList` says which channels
  have a message list.

## Tests

`FFISmokeTests` use the real XCFramework without network. The view model tests use the fakes
in `Tests/AkariKitTests/Fakes`: subclasses of the generated classes made with UniFFI's
`init(noHandle:)`, which keep their state in memory and record every call and read. The
Keychain tests use the login keychain with a new service name per run and delete their items.
