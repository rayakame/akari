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

This is the intended layout. So far `akari-core` has the wire models and gateway decoding;
the other crates are still empty skeletons, and each app folder contains only a README.

| Path | Contents |
|---|---|
| `crates/akari-core` | Discord gateway (WebSocket, zstd-stream), REST with rate limit handling, models, state store, SQLite disk cache |
| `crates/akari-markdown` | Discord-flavored markdown parser |
| `crates/akari-ffi` | UniFFI bindings only (Swift for macOS/iOS, Kotlin for Android) |
| `crates/akari-cli` | Terminal test client for developing the core |
| `apps/apple/AkariKit` | Shared Swift package: logic and view models for macOS and iOS |
| `apps/macos` | SwiftUI shell + AppKit message list, XcodeGen project, consumes `akari-ffi` via an XCFramework / Swift package |
| `apps/ios` | Later: SwiftUI/UIKit, same XCFramework, uses AkariKit |
| `apps/android` | Later: Jetpack Compose on the `akari-ffi` Kotlin bindings |
| `apps/linux` | Later: gtk4-rs + libadwaita, uses `akari-core` directly |
| `docs/protocol` | Notes on the user-account Discord API (main reference: https://docs.discord.food) |
| `docs/ui` | Notes on Discord's layout, behavior and shortcuts that the apps mirror |

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
  cache never stores them; a state layer converts them into its own memory-efficient types.
  Value types (`Snowflake<M>` with its markers, `Timestamp`, `Permissions`) are shared
  vocabulary the state layer may reuse.
- `akari-ffi` contains bindings only; logic belongs in `akari-core` or `akari-markdown`.
- Swift and Kotlin apps reach Rust only through `akari-ffi`; the Linux app links the
  crates directly.

## Cross-platform requirements

Decided up front; changing them later is expensive.

- **TLS**: rustls everywhere (reqwest, tokio-tungstenite), never OpenSSL or native-tls.
  aws-lc-rs is the only rustls crypto provider. Certificates are verified against the
  OS trust store via rustls-platform-verifier. The gateway WebSocket uses the same
  rustls `ClientConfig` as REST, never tokio-tungstenite's default connector.
  `deny.toml` bans `openssl`, `openssl-sys`, `native-tls` and `ring`.
- **SQLite**: rusqlite with the `bundled` feature.
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

# Cross-target checks (CI runs them; rust-toolchain.toml installs both targets).
# Locally, the iOS check needs the iOS SDK (full Xcode) once crates compile C code.
# cargo-ndk always needs an Android NDK: ANDROID_NDK_HOME or Android Studio's SDK.
cargo check --workspace --exclude akari-cli --target aarch64-apple-ios
cargo ndk -t arm64-v8a check --workspace --exclude akari-cli  # needs cargo-ndk

# Banned dependencies (needs cargo-deny)
cargo deny check bans
```
