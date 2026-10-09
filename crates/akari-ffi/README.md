# akari-ffi

UniFFI bindings over `akari-core` for Swift (macOS now, iOS later) and Kotlin (Android,
later). This crate holds bindings only: logic belongs in `akari-core`, and Apple-specific
view-model behavior in AkariKit (`apps/apple/AkariKit`).

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
  `messages(channel_id, ids)`); lists cross as ID lists (`guild_ids`, `channel_list`,
  `window`).

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
  `account`). Only the host's `TokenStore` sees the string, to put it into the platform's
  secret store. Swift's default printing of a struct includes every field, so a string in a
  login result would end up in logs.

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
