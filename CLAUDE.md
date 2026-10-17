# Akari

Akari is a native, extremely resource-efficient third-party Discord client. macOS comes
first, then Linux (GTK4/libadwaita), later iOS and Android, eventually Windows.

## Product goal

Discord users must be able to switch to Akari with zero relearning. Layout, navigation,
interaction patterns and keyboard shortcuts closely mirror the official Discord client,
implemented with native controls so the app feels at home on each platform.

## UI/UX requirements

- **Layout** mirrors Discord: server rail on the far left (DMs/home on top, server
  icons, folders), channel list with categories, message area in the center, optional
  member list on the right, user panel at the bottom left.
- **Behavior** mirrors Discord: unread and mention indicators, pings, replies, threads,
  reactions, embeds, attachments, typing indicators, jump to unread, message grouping by
  author and time, hover actions on messages, context menus.
- **Keyboard shortcuts** match Discord where the platform allows it: Ctrl/Cmd+K quick
  switcher, Alt+Up/Down channel navigation, Escape marks read, Up arrow edits the last
  message, and so on.
- **Themes**: dark by default with colors close to Discord's, plus a light theme.
- **No Discord assets**: no Discord logo, icons, illustrations (Wumpus), sounds or the
  gg sans font. Use platform icons (SF Symbols on Apple, Adwaita icons on Linux,
  Material icons on Android) and system fonts. Akari has its own name and app icon.

Reference notes on Discord's layout, behavior and shortcuts live in `docs/ui/`.

## Repository layout

This is the intended layout. So far `akari-core` has the wire models, gateway decoding, the
shared `DiscordClient` with both login flows (email/password and QR code), the token
storage trait, the gateway connection (zstd-stream, heartbeats, resume, rate-limited
sends), the state store (`akari_core::state`, which also keeps the user's settings: server
folders and order, mutes and notification settings; and the read states, with unread dots
and mention counts per channel and server) and `Account`, which keeps the store
current from the gateway, ends timed mutes, loads message history and sends messages over
rate-limited REST, applying the message length limit and slowmode; it also acks what the user
reads (paced per channel, one request at a time, "now" message IDs from Discord's clock, not the
Mac's) and marks channels and servers read, in bulk chunks;
`akari-cli` can log in, log out, connect, list guilds and channels, read, send and tail
messages, and show where a channel's unread state and mentions come from. `akari-ffi` exposes the client, both logins, `Account`, store reads, change events
and the message APIs, including the send rules, the read calls and `end_session`, through UniFFI, and `apps/apple/AkariKit` is a Swift package around its
XCFramework and generated bindings, with a Keychain token store, the view models (app,
login, session, server, channel, DM and message lists, and the composer), the message
timeline and the reading signal (what tells the core which channel the user reads). `apps/macos` is the first app: the login screen and the main window with the server
rail (the user's order, with folders, unread pills and mention badges, and unread DMs under
Home), the channel and DM lists (muted channels dimmed, unread rows bright with the pill, channel
rows with their counts), a
message list that loads history in both directions, with the "new messages" line and the unread
bar, the composer, and marking read with Escape, Shift+Escape and the context menus; it shows the
connection state and reconnects. akari-core also has the SQLite disk
cache (`akari_core::cache`): one file per account with the last session's state and recent
messages, which the macOS app restores before the gateway connects. `akari-markdown` parses
Discord's message markdown (the rules, a flat document with source spans, a source map, UTF-16
offsets and a document cache); akari-ffi gives each message its parsed `formatted` text, parsed
on Akari's runtime through akari-core's message preparer hook; the macOS app renders it (styles,
code, quotes, lists, mentions, timestamps, spoilers and links), drawing boxes, pills and spoiler
backgrounds as layers behind the text. The other app folders contain only a README.

| Path | Contents |
|---|---|
| `crates/akari-core` | Discord gateway (WebSocket, zstd-stream), REST with rate limit handling, models, state store, SQLite disk cache |
| `crates/akari-markdown` | Discord-flavored markdown parser |
| `crates/akari-ffi` | UniFFI bindings only (Swift for macOS/iOS, Kotlin for Android) |
| `crates/akari-cli` | Terminal test client for developing the core |
| `crates/akari-bindgen` | UniFFI's Swift bindings generator at the workspace's UniFFI version (a build tool, run by `apps/apple/build-ffi.sh`) |
| `apps/apple/AkariKit` | Shared Swift package: logic and view models for macOS and iOS |
| `apps/macos` | SwiftUI shell + AppKit message list on AkariKit; the Xcode project is generated by XcodeGen from `project.yml` |
| `apps/ios` | Later: SwiftUI/UIKit, same XCFramework, uses AkariKit |
| `apps/android` | Later: Jetpack Compose on the `akari-ffi` Kotlin bindings |
| `apps/linux` | Later: gtk4-rs + libadwaita, uses `akari-core` directly |
| `docs/protocol` | Notes on the user-account Discord API (main reference: https://docs.discord.food) |
| `docs/ui` | Notes on Discord's layout, behavior and shortcuts that the apps mirror |
| `assets` | Akari's logo (white, on transparent) and app icon master as SVG; each app derives its own icon formats from them |

## Crate boundaries

```
akari-ffi  ──► akari-core, akari-markdown
akari-cli  ──► akari-core, akari-markdown
apps/linux ──► akari-core, akari-markdown   (later, as a workspace member)
```

- `akari-core` has no UI dependencies and does not depend on `akari-markdown`: it
  stores raw message content, and consumers parse it for display.
- `akari-core` emits fine-grained events/diffs, never whole lists.
- The wire layer (`akari_core::model` and the payload types in `akari_core::gateway`)
  mirrors what Discord sends and nothing else. UIs never see wire structs and the SQLite
  cache never stores them; the state layer (`akari_core::state`) converts them into its own
  memory-efficient types. Value types (`Snowflake<M>` with its markers and ID aliases,
  `Timestamp`, `Permissions`) and the integer enums (`ChannelType`, `MessageType`, …) are
  shared vocabulary the state layer reuses.
- `akari-ffi` contains bindings only; logic belongs in `akari-core` or `akari-markdown`.
- Dev-only cargo features (`capture`, which hands out the raw READY, `repeat-nonce`, which
  repeats a send with the same nonce, and `flags-only`, which leaves member lists out of op
  37) are enabled only by `akari-cli`. Cargo unifies features across a workspace build, so
  `cargo build --workspace` would compile `akari-core` with them for `akari-ffi` too: app
  builds always build `-p akari-ffi`, and CI checks that `-p akari-ffi` pulls in none of
  them, nor `insecure-test-endpoints` (plaintext test servers), which only test suites
  enable as a dev-dependency.
- Swift and Kotlin apps reach Rust only through `akari-ffi`; the Linux app links the
  crates directly. How state crosses the boundary (records read by ID, ID-only events,
  Akari's own tokio runtime) is in `crates/akari-ffi/README.md`.
- The generated Swift bindings, the XCFramework and the macOS app's Xcode project are build
  outputs, never committed; neither is `apps/macos/Config/Local.xcconfig`, which holds a
  developer's team ID. The bindings stay in their own target (`AkariFFI`), because they
  don't compile with MainActor as the default isolation.

## Cross-platform requirements

Decided up front; changing them later is expensive.

- **TLS**: rustls everywhere (reqwest, tokio-tungstenite), never OpenSSL or native-tls.
  aws-lc-rs is the only rustls crypto provider. Certificates are verified against the
  OS trust store via rustls-platform-verifier. The gateway WebSocket uses the same
  rustls `ClientConfig` as REST, never tokio-tungstenite's default connector.
  `deny.toml` bans `openssl`, `openssl-sys`, `native-tls` and `ring`.
- **SQLite**: rusqlite, linking the system `libsqlite3` on Apple targets (macOS and iOS ship
  it) and with the `bundled` feature everywhere else. akari-core adds `bundled` for non-Apple
  targets; AkariKit links `sqlite3`.
- **Token storage** is not done in Rust. The core defines a trait / UniFFI callback
  interface that each host app implements (Keychain on Apple, Android Keystore,
  libsecret on Linux).
- **Suspend/resume**: the core handles frequent suspend/resume (mobile backgrounding):
  fast resume, fast cold start from the SQLite cache, explicit pause/resume API.
- **Targets**: every dependency must compile for macOS, Linux, iOS and Android. CI
  checks `aarch64-apple-ios` on a macOS runner and `aarch64-linux-android` via
  cargo-ndk with the runner's NDK.
- Versions and cross-platform features of external dependencies are pinned in
  `[workspace.dependencies]`. Member crates use `dep.workspace = true` and add only
  crate-specific features.
- The Android app must initialize rustls-platform-verifier from Kotlin before the core
  makes network calls.

## Rules

- Tokens must never be logged, printed, committed or written to plain files.
- No `unwrap()`/`expect()` in library crates (`akari-core`, `akari-markdown`,
  `akari-ffi`); clippy denies them outside tests. Every crate sets
  `[lints] workspace = true`; `akari-cli` allows `clippy::unwrap_used` and
  `clippy::expect_used` in `main.rs`.
- The toolchain is pinned in `rust-toolchain.toml` (exact version, rustfmt, clippy,
  rust-src, iOS and Android targets) and `rust-version` in `Cargo.toml` matches it.
  Bump both together; CI installs the toolchain from the file.
- Errors via `thiserror`, logging via `tracing`.
- `cargo fmt`, `cargo clippy -- -D warnings` and `cargo test` must pass.
- **Most important rule:** commits, pull requests and branch names carry no AI or tool
  attribution: no `Co-Authored-By` trailers for AI assistants, no "Generated with …"
  lines, no `claude/` branch prefixes, no AI tool files (`.claude/`, specs, plans) in
  the repo.

## Commands

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace

# Cross-target checks (CI runs them; rust-toolchain.toml installs the targets).
# Locally, the iOS check needs the iOS SDK (full Xcode) once crates compile C code.
# cargo-ndk always needs an Android NDK: ANDROID_NDK_HOME or Android Studio's SDK.
cargo check --workspace --exclude akari-cli --exclude akari-bindgen --target aarch64-apple-ios
cargo ndk -t arm64-v8a check --workspace --exclude akari-cli --exclude akari-bindgen  # needs cargo-ndk

# akari-core without akari-cli's dev features, and the feature boundary apps rely on
cargo clippy -p akari-core --all-targets -- -D warnings
cargo tree -p akari-ffi -e features,no-dev -i akari-core   # must list no "capture", "flags-only", "repeat-nonce" or "insecure-test-endpoints"

# akari-ffi for Apple: the XCFramework and Swift bindings into AkariKit (needs full Xcode),
# then the package's tests. --host-only builds just this Mac's architecture.
apps/apple/build-ffi.sh
swift test --package-path apps/apple/AkariKit

# The macOS app (needs XcodeGen, `brew install xcodegen`, after build-ffi.sh)
xcodegen generate --spec apps/macos/project.yml
xcodebuild test -project apps/macos/Akari.xcodeproj -scheme Akari -destination 'platform=macOS'

# Banned dependencies (needs cargo-deny)
cargo deny check bans
```
