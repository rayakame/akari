# Akari for macOS

The first Akari app: a SwiftUI shell with an AppKit table for the message list, on the
shared `AkariKit` package (`apps/apple/AkariKit`) and akari-ffi's XCFramework. It needs
macOS 14 or later.

So far it has the login screen (QR code, email or phone and password, two-factor codes,
new-location confirmation) and the main window: the server rail, a server's channel list,
the DM list at home, the channel header, the user panel and a read-only message list.
Sending, loading older messages, edits and the connection status come next; markdown,
images, reactions, voice, notifications and settings later.

## Building

Building needs full Xcode (not just the Command Line Tools) and
[XcodeGen](https://github.com/yonaskolb/XcodeGen) (`brew install xcodegen`).

```sh
apps/apple/build-ffi.sh --host-only   # akari-ffi's XCFramework and Swift bindings
xcodegen generate --spec apps/macos/project.yml
open apps/macos/Akari.xcodeproj
```

Run `build-ffi.sh` again after akari-ffi changes, and `xcodegen generate` after
`project.yml` changes or files are added. The Xcode project is generated, so it isn't
committed: `project.yml` is its source.

## Signing and the Keychain

Akari keeps the login token in the login keychain. The keychain lets an app read an item
only if the app's code signature matches the one that saved it. An ad hoc signature
("Sign to Run Locally") changes with every build, so macOS then asks for access to the
token after every rebuild. A development certificate keeps the signature stable:

1. Xcode → Settings → Accounts: add your Apple ID. A free Apple ID works; Xcode creates an
   "Apple Development" certificate for your personal team.
2. Copy `Config/Local.xcconfig.example` to `Config/Local.xcconfig` (not committed) and put
   your team ID in it. The team ID is shown in Xcode's account settings.
3. Run `xcodegen generate` again and build.

Without `Local.xcconfig` the app still builds and runs, signed ad hoc, with the prompts.
The app uses App Sandbox with outgoing network connections only, so it needs no
provisioning profile.

## Tests

```sh
xcodebuild test -project apps/macos/Akari.xcodeproj -scheme Akari -destination 'platform=macOS'
```

The tests run inside the app (a hosted test bundle), which starts nothing under tests: no
client, no Keychain and no network. The logic the iOS app will share lives in AkariKit and
is tested there with `swift test`.

## Logs and launch time

The `-AkariLog <filter>` launch argument sends akari-core's logs to stderr, in every build.
The filter has tracing's syntax: `akari_core=info` shows the gateway's milestones with their
durations, `akari_core=debug` more. The logs hold event names, IDs and durations, never
message content, names or tokens.

- **Xcode:** Product → Scheme → Edit Scheme… → Run → Arguments, tick
  `-AkariLog akari_core=info`, run. The lines show in Xcode's console, e.g.
  `INFO akari_core::gateway::connection::task: gateway connected connect_ms=290`.
- **Terminal:** `Akari.app/Contents/MacOS/Akari -AkariLog akari_core=info`.

`-AkariLog scroll` turns on the message list's scroll log instead (category `scroll`, in Xcode's
console and the unified log): the clip origin, its allowed range, the rows' height, the
elasticity and whether a live scroll is running on every bounds change, plus each scroll the
app makes itself. Numbers only. Combine both as
`-AkariLog akari_core=info,scroll`; a second `-AkariLog` argument would replace the first.

The app logs its launch milestones once each, in milliseconds since process start, to the
unified log (subsystem `app.akari`, category `launch`): app started, client created, token
loaded, session started, first message load started and finished, first channel rendered,
READY in the app. Xcode's console shows them too. In zsh, `log` is a builtin, so call the
tool by its path:

```sh
/usr/bin/log stream --predicate 'subsystem == "app.akari" AND category == "launch"' --level info
```

## Theme

View → Theme switches between Dark (the default), Light and Match System. The colors are the
tokens of `docs/ui/theme.md`.
