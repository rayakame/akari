# Akari for macOS

The first Akari app: a SwiftUI shell with an AppKit table for the message list, on the
shared `AkariKit` package (`apps/apple/AkariKit`) and akari-ffi's XCFramework. It needs
macOS 14 or later.

So far it has the login screen (QR code, email or phone and password, two-factor codes,
new-location confirmation) and the main window: the server rail, a server's channel list,
the DM list at home, the channel header, the user panel, the message list and the composer.
Markdown, images, attachments, reactions, replies, threads, voice, notifications and settings
come later.

## Messages and the composer

- **Sending:** Return sends; Shift+Return and Option+Return add a line; Return while an input
  method is composing commits the composition. The composer grows to half the message area,
  keeps a draft per channel for the session, counts down from 200 characters left, and holds a
  message back during a slowmode cooldown (`docs/ui/composer.md`).
- **Pending and failed messages:** pending ones are dimmed; failed ones are red with "Retry" and
  "Delete", and the reason shows under the composer.
- **History:** older pages load before the top is reached and newer ones near the bottom while
  older messages are shown; the text being read never moves when a page lands, an edit
  changes a message's height or a message is deleted. A bar above the composer jumps to the
  present, and so does Escape, from the list or the composer (`docs/ui/message-list.md`).
- **Up arrow** in an empty composer is reserved for editing the last message, which isn't
  built yet.
- **Times** ("Today at", day dividers) follow midnight, time zone and locale changes.

## Connection

- A bar under the title bar says when Akari is connecting, reconnecting or offline; quick
  reconnects don't show it. A session closed by an error shows the error with "Reconnect",
  which opens a new session from the token in memory, without reading the Keychain. A
  rejected token returns to the login screen.
- Before the Mac sleeps the session disconnects, and it reconnects after it wakes
  (`NSWorkspace` notifications), since the gateway's own timers don't notice a sleep.
- Akari → Log Out deletes the token from the Keychain first and shows the login screen at
  once; the session is then ended on Discord in the background. If the Keychain item can't be
  deleted, the session stays open and an alert says why. If Discord doesn't confirm the
  logout, the login screen says the session may still be active.

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
app makes itself, each request for an older or newer page, and each time the list keeps the
reader's place while rows change above it ("compensates": the rows removed, inserted and
reloaded above, the height delta, y before and after, and whether a live scroll ran). Numbers
only. Combine both as
`-AkariLog akari_core=info,scroll`; a second `-AkariLog` argument would replace the first.

The app logs its launch milestones once each, in milliseconds since process start, to the
unified log (subsystem `app.akari`, category `launch`): app started, client created, token
loaded, session started, first message load started and finished, first channel rendered,
READY in the app. Xcode's console shows them too. In zsh, `log` is a builtin, so call the
tool by its path:

```sh
/usr/bin/log stream --predicate 'subsystem == "app.akari" AND category == "launch"' --level info
```

Memory and idle CPU of a running Release build, as measured for the PR bodies:

```sh
pid=$(pgrep -x Akari)
vmmap --summary "$pid" | grep -E 'Physical footprint'
top -l 13 -s 5 -pid "$pid" -stats pid,cpu | awk -v p="$pid" '$1==p {n++; if (n>1) {s+=$2; c++}} END {printf "%.2f%%\n", s/c}'
```

## Theme

View → Theme switches between Dark (the default), Light and Match System. The colors are the
tokens of `docs/ui/theme.md`.
