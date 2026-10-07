# Akari

A native, resource-efficient third-party Discord client.

Akari aims to look and feel like the official Discord client, so switching takes
zero relearning, while being built with native UI toolkits instead of Electron.

> **Status:** early development. Nothing usable yet.

## Goals

- **Native on every platform:** SwiftUI/AppKit on macOS, GTK4/libadwaita on Linux,
  SwiftUI on iOS, Jetpack Compose on Android
- **Fast and light:** low memory use, quick startup, smooth scrolling in large servers
- **Familiar:** same layout, behavior and keyboard shortcuts as Discord

## Platforms

| Platform | Status |
|---|---|
| macOS | in progress |
| Linux | planned |
| iOS | planned |
| Android | planned |
| Windows | eventually |

## Architecture

A shared Rust core handles the Discord gateway, REST API, state and caching. Each
platform has its own native UI on top of it.

```
crates/
  akari-core       gateway, REST, models, state, SQLite cache
  akari-markdown   Discord-flavored markdown parser
  akari-ffi        UniFFI bindings for Swift and Kotlin
  akari-cli        terminal test client
apps/
  macos, ios, android, linux
```

## Building

Requires Rust (the toolchain is pinned in `rust-toolchain.toml`).

```sh
cargo build
cargo test
```

The macOS app additionally requires Xcode and XcodeGen.

## Disclaimer

Akari is an unofficial client and is not affiliated with, endorsed by or sponsored
by Discord Inc. Using third-party clients may violate Discord's Terms of Service and
could put your account at risk. Use at your own risk.

## License

[MIT](LICENSE)
