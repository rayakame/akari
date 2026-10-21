# Akari for macOS

The first Akari app: a SwiftUI shell with an AppKit table for the message list, on the
shared `AkariKit` package (`apps/apple/AkariKit`) and akari-ffi's XCFramework. It needs
macOS 14 or later.

So far it has the login screen (QR code, email or phone and password, two-factor codes,
new-location confirmation) and the main window: a centered title naming the server or DM
(drawn by `TitleBar` over the native title bar, whose own title stays set but hidden), the
server rail (the user's order, with folders, unread pills and mention badges, and the DMs
with mentions under Home), a server's channel list (muted channels dimmed, "hide muted
channels" followed, unread rows bright with their counts, a collapsed category keeping
its unread and mentioned channels, and joined threads under their parent, hung from a spine
with branches), the DM list at home, the channel header (a thread's with its channel's name,
which opens the channel), the user panel, the message list with formatted messages (a thread's
start with its owner, its starter message, the "started a thread" line with its link, and a pin's line, which jumps to the pinned
message), and
the composer (a locked thread's banner). Images, attachments, reactions, replies, the box
under a message with a thread, the split view, voice, notifications and settings come later.

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
- **Times** (today's time, "Yesterday at", day dividers) follow midnight, time zone and locale
  changes.
- **Formatting** (`docs/ui/markdown.md`): `MessageText` builds a message's attributed string
  from `Message.formatted`, once per message, revealed spoilers and relative text, shared by
  the height pass and the cell. `MessageTextView` shows it with TextKit 1, and
  `MessageLayoutManager` says where code boxes, quote bars, pills and spoilers' backgrounds go;
  `MessageDecorations`, kept behind the text view, shows them as layers (see Performance), and
  the layout manager draws only hidden spoilers' covers over the glyphs. A code block's padding
  at the text's top or bottom is the text view's own (TextKit 1 leaves paragraph spacing out
  there). Heights come from `TextMeasurer`, which measures with string drawing: it breaks lines
  as TextKit 1 does at half the cost.
- **Spoilers** are covered until clicked; a revealed one stays revealed until the channel is
  left. Assistive apps get a hidden one as "Spoiler", in the value, the selection, ranges and
  RTF, and as a link element of that name that reveals it; the text view makes its own link
  elements, since NSTextView's carry the hidden text. Look Up finds nothing on a hidden one.
- **Links:** a plain URL opens in the browser; a masked link asks first, in Akari's words, with
  a box to trust its host for the account (`LinkConfirmation`). A link to a channel Akari
  knows, and a channel mention, open that channel, in another server too. Only the schemes the
  parser links open (http, https, discord, steam, mailto, tel, sms).
- **Copy:** Cmd+C copies what the selection shows, as plain text (`bold and @name`). A
  message's context menu has "Copy Text", which copies its markdown.
- **Mentions of the user** tint the row; relative timestamps count down on one timer for
  the list, which waits while the window is hidden or minimized and goes with the list.

## Connection

- A bar under the title bar says when Akari is connecting, reconnecting or offline; quick
  reconnects don't show it. A session closed by an error shows the error with "Reconnect",
  which opens a new session from the token in memory, without reading the Keychain. A
  rejected token returns to the login screen.
- Before the Mac sleeps the session disconnects, and it reconnects after it wakes
  (`NSWorkspace` notifications), since the gateway's own timers don't notice a sleep. A wake
  and a restored launch also open a REST connection ahead of the first message load (a
  `GET /api/v9/gateway` without the token), so that load doesn't pay the TLS handshake.
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

Debug builds and the test host are `app.akari.Akari.dev`, with their own container (settings
and cache) and their own Keychain service, `app.akari.dev.tokens`; a Debug run logs in
separately. Release builds are `app.akari.Akari` with `app.akari.tokens`.

## Tests

```sh
xcodebuild test -project apps/macos/Akari.xcodeproj -scheme Akari -destination 'platform=macOS'
```

The tests run inside the Debug app (a hosted test bundle, `app.akari.Akari.dev`), which
starts nothing under tests: no client, no Keychain and no network. Their settings live in
`MemoryDefaults`, never in the app's defaults or a suite. The logic the iOS app will share
lives in AkariKit and is tested there with `swift test`.

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
only. Combine them as
`-AkariLog akari_core=info,scroll`; a second `-AkariLog` argument would replace the first.

`-AkariLog load` (category `load`) logs one line per history page and one per channel open,
with channel IDs, counts and durations only:

```text
channel 1234 latest page 1: queued 86 ms, limited 0.0 ms, headers 370 ms, connection reused,
  body 1.3 ms, 69755 bytes, decode 0.7 ms, parse 0.2 ms (50 parsed), store 0.1 ms, read 0.1 ms,
  render 44 ms for 50 rows, shown after 524 ms; 50 messages; no limit headers
channel 1234 opened: rows from the network after 544 ms
channel 1234 opened: rows from disk after 24 ms, fresh after 380 ms
```

- **queued:** behind an earlier request for the same channel, or a warm-up still connecting.
- **limited:** waiting for Discord's rate limits (a bucket reset, a 429 pause, the global
  limit); zero when nothing had to wait.
- **headers:** from sending the request until the response headers, over all attempts; with
  "new connection", DNS, TCP and TLS are part of it.
- **body:** reading the body; **bytes** is its size after decompression.
- **decode:** JSON into messages; **store:** into the channel's window, under the store's lock.
- **parse:** the messages' markdown, parsed on Akari's runtime before they enter the window
  (the page's new messages and, on a first page, the disk cache's copy shown before it), with
  how many were parsed; ones parsed before aren't parsed again.
- **read:** AkariKit reading the window and the new messages; **render:** the table update
  that inserted the page's rows (row heights included), with how many; a message arriving
  meanwhile isn't counted. **held … for live scroll:** how long a scroll past the top held the
  page back (it lands when the bounce ends). **shown after:** from the load's start until the
  frame with the rows was committed.
- **remaining, reset:** Discord's `X-RateLimit-Remaining` and `-Reset-After`, or "no limit
  headers".
- A failed page says `failed after N ms:` and the error's kind; Discord's message isn't logged.

akari-core logs the same stages itself at `akari_core::load` (debug, stderr; the parse as
`prepare_ms` and `prepared`), and new REST connections at `akari_core::rest` (debug, with their
connect time). `akari_ffi::content` (debug) logs a line whenever a read had to parse a message a
load had parsed and the cache dropped since (a miss: the channel ID and the count); live
messages parsed when read aren't logged.

### Comparing a page load with the official client

1. Same Mac, same network, same channel, one after the other. Akari runs with `-AkariLog load`.
2. In Chrome, open the official web client logged in as the test account, then DevTools →
   Network, filter `messages?limit`.
3. Scroll up in each client until five older pages have loaded.
4. For each request, Chrome's Timing tab against Akari's line:
   - Queueing and Stalled ≈ `queued` + `limited`;
   - DNS Lookup, Initial connection and SSL ≈ the connect time of a "new connection";
   - Request sent and Waiting for server response ≈ `headers`;
   - Content Download ≈ `body`.
   The Size column shows the bytes on the wire against the page's size (compression), the
   Protocol column `h2` or `h3` (Akari speaks h2; a browser on h3 can connect faster).
5. Write down the medians only. Never export a HAR file: it holds the token.

The app logs its launch milestones once each, in milliseconds since process start, to the
unified log (subsystem `app.akari`, category `launch`): app started, client created, token
loaded, cache restored, session started, first message load started and finished, first
channel rendered, first fresh page rendered (the first page from the network on screen), READY
in the app. Xcode's console shows them too. In zsh, `log` is a builtin, so call the
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

## Performance

What the Performance and cache milestone found, so later changes don't bring it back:

- **Drawn rows cost memory.** The placeholder rows used to draw themselves into a bitmap; one
  of them is at least one and a half views tall, and at launch it was most of the memory peak
  (Release, restored channel: a peak of 112–125 MB, 69 MB of it graphics memory, for about a
  second). They are plain colored layers now (peak 45 MB). A view that overrides
  `draw(_:)` gets a bitmap as large as itself, so large rows shouldn't.
- **Long messages bring in a GPU renderer for a second.** AppKit has views that draw about
  40,000 pt² or more (text fields included: a message of three lines or more at usual widths)
  rendered by Core Animation's Metal renderer. While such views are drawn, as when paging or
  opening a channel, the renderer's working memory of about 64 MB counts in Akari's
  footprint as graphics memory, released about a second after the last one. That is the
  paging peak (recorded: 161–169 MB, 74 MB of it this, 76 MB between pages). Its switch, the
  undocumented `CAEnableMetalWhippet`, makes it worse: the CPU fallback keeps uncompressed
  bitmaps, and in the probe's paging the peak rose from 159 to 175 MB and memory stayed at
  170 MB instead of settling at 73 MB.
- **Buttons are expensive to build.** On current macOS every `NSButton`'s `sizeToFit` runs a
  SwiftUI layout pass. Each message cell built a hidden "Not sent." line with two buttons;
  it's built only for a failed message now, which made building a row about four times
  cheaper (the first page's frame went from 56–60 to 37–41 ms in Release).
- **No fixed height may be required in a cell.** While the table moves row views during an
  update, a cell can sit in a row of another height for a moment; a required height then
  conflicts, and AppKit logs a long message on that frame. Cell heights come from the
  controller (`heightOfRow`); the cells' own height constraints are priority 999 and only
  serve the tests' Auto Layout oracle.
- **What's left in the first page's frame** is mostly Auto Layout of the new rows; only manual
  layout in the cells would cut it.
- **Message formatting draws as little as it can.** A view's bitmap covers what it draws, so a
  code box drawn full width by the text view pushed a code message past the 40,000 pt² above;
  and AppKit keeps text drawn in one color as a one-byte mask, but in full color as soon as a
  second color or a fill is drawn (20 copies of a paragraph: about 1 MB, 12.5 MB with one
  pill). Code boxes, quote bars, pills and spoilers' backgrounds are therefore layers behind
  the text (`MessageDecorations`), and the text view draws only glyphs and hidden spoilers'
  covers. Colored mentions and links still need full color.
- **Formatted rows are built and measured on the main thread**, once per message (shared by
  the height pass and the cell): on fully formatted pages the first page costs about 1.6× a
  plain one. Building a page's text off the main thread is the next step if pages landing in
  a fling ever hitch.
- **A new list waits for its size.** SwiftUI updates a list before it sizes it, while its
  column has AppKit's default width of 100 pt, where formatted rows cost several times as
  much to build and measure; a channel the user returned to paid for every row twice. The
  controller holds that update until the list has a width. A message's text view never
  scrolls the list either: NSTextView scrolls its insertion point into view when layout
  resizes it, which moved a pinned list off its bottom.

`AkariTests/PerformanceProbe.swift` measures the table offline (no account, synthetic
messages, a window on screen) and only runs when asked:

```sh
TEST_RUNNER_AKARI_PROBE=1 xcodebuild test -project apps/macos/Akari.xcodeproj -scheme Akari \
  -destination 'platform=macOS' -only-testing:AkariTests/PerformanceProbe
```

It prints `PROBE` lines: frame times of a fling with pages landing, the first page's time, and
the footprint. `TEST_RUNNER_AKARI_PROBE_TEXT=formatted` shows only formatted messages
(`FormattedSamples`), `=realistic` plain ones with every fourth one of the markdown corpus's
realistic messages (`CorpusSamples`). Swift Testing runs the probe's tests in no fixed order, so
for footprints run one at a time, e.g.
`-only-testing:'AkariTests/PerformanceProbe/flingThroughPagesThatLand()'`.
`TEST_RUNNER_AKARI_PROBE_PAUSE=4` pauses at each phase so `footprint` or `heap` can look at the
test host, e.g.
`footprint --pid "$(pgrep -x Akari)" --sample 1 --sample-duration 40`, which splits the
footprint by kind (malloc, CoreAnimation, graphics).

Recording the real app (Release, signed with an Apple Development identity, which carries the
`get-task-allow` entitlement Instruments needs):

```sh
mkdir -p ~/akari-recordings
footprint -p Akari --sample 0.5 --sample-duration 150 > ~/akari-recordings/footprint.txt
xcrun xctrace record --template 'Animation Hitches' --attach Akari --time-limit 60s \
  --output ~/akari-recordings/hitches.trace
xcrun xctrace record --template 'Allocations' --attach Akari --time-limit 120s \
  --output ~/akari-recordings/alloc.trace
```

The app marks its own work as Points of Interest (`page lands`, `row heights`,
`message cell made`, `page requested`, live scroll start and settle), which Instruments shows
beside its tracks. Delete recordings after use: they come from a real account.

## Disk cache

Each account's state and its recently viewed channels' newest messages are kept in
`~/Library/Containers/app.akari.Akari/Data/Library/Application Support/Akari/Cache` (Debug
builds: in `app.akari.Akari.dev`'s container), one
`<user id>.sqlite` (with `-wal` and `-shm`) per account, excluded from backups. What it holds,
when it's written and when it's deleted: [cache.md](../../docs/protocol/cache.md). Quitting
waits for a last write, at most about a second.

- **Its logs:** `-AkariLog akari_core::cache=info` (counts, sizes and durations only).
- **Its size:** `du -ch ~/Library/Containers/app.akari.Akari/Data/Library/Application\ Support/Akari/Cache/*`.
- **Deleting it by hand:** quit Akari, then delete the directory's files. The next launch
  starts from the network, as without a cache.

## Theme

View → Theme switches between Dark (the default), Light and Match System. The colors are the
tokens of `docs/ui/theme.md`.

Text sizes and weights come from `TypeScale` (`Akari/Theme/TypeScale.swift`), one style per
kind of text at a 16 pt base, as listed in `docs/ui/typography.md`. Views take their fonts
from it, never a size of their own; glyphs inside icons size with their icon.
