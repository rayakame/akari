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
  the channel last opened in each guild. It owns `GuildListModel` (the server list),
  `ChannelListModel` (the open guild's channels) and `MessageListModel` (the open channel's
  messages, loads, sends and whether the user may send there). The view calls
  `MessageListModel.open()` when the channel appears.

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

## Tests

`FFISmokeTests` use the real XCFramework without network. The view model tests use the fakes
in `Tests/AkariKitTests/Fakes`: subclasses of the generated classes made with UniFFI's
`init(noHandle:)`, which keep their state in memory and record every call and read. The
Keychain tests use the login keychain with a new service name per run and delete their items.
