# Cursors

Which cursor the pointer shows where, and what decides it. Each region has one authority, or
leaves the cursor to AppKit; none sets a cursor over another's region. Audited 2026-10-23 after
Chris's tests of #29, where views behind a popout and neighbouring message rows fought over it.
Anything clickable shows the pointing hand (Chris, 2026-10-23).

`CursorArea` sets its cursor from its own tracking (active in the active app, key window or not:
cursor rects only work in a key window and went stale in a panel that took the key before its
content was laid out). Leaving one, the pointer shows what lies there now: another area's
cursor, the message list's choice, the I-beam over editable text, else the arrow, whichever
tracking event comes first.

| Region | Cursor | Authority |
|---|---|---|
| The message list (each of the channel's and the split's): rows, a message's text, links, mentions, hidden spoilers and code blocks, thread boxes, Retry and Delete, Try again; the bars over it (unread, jump, locked, status) | I-beam over text, pointing hand over what clicks (the unread bar, the jump bar's button too), arrow elsewhere | The list's tracking area (`MessageTableController`): it hit-tests on every move, scroll, update and key change, while its window is key and on top at the pointer; over a bar it takes the bar's `CursorArea`. Text views and boxes track nothing themselves. |
| The composer | I-beam | AppKit (NSTextView's own) |
| The channel header: the "# parent ›" link, the Threads button, the thread's "…" | Pointing hand | `CursorArea` |
| The split's header: its title, "…", the X | Pointing hand | `CursorArea` |
| The split's gap and the sidebar's edge | Resize left and right, held through a drag | `ResizeCursor`: one push on hover or a drag's start, one pop at the end or when the view goes |
| The Threads browser's panel (key): cards, the modal's X, the field's clear button | Pointing hand | `CursorArea` |
| The browser's search field | I-beam | AppKit (the field is AppKit's own, in the key panel) |
| The browser's Create and the empty state's Create Thread | Pointing hand | `CursorArea` |
| The New Thread form: the X, the private checkbox and its label | Pointing hand (the arrow over the disabled checkbox) | `CursorArea` |
| The form's name field and composer | I-beam | AppKit (an NSTextField and the composer's NSTextView) |
| The form's source message | Arrow | AppKit's default: no list tracks it |
| The shield under the browser | Arrow | Its own tracking area (`ShieldWindow`) |
| More Active Threads (never key): its rows and "See All" | Pointing hand | `CursorArea` |
| Everything else: the server rail, the channel and DM lists, the user panel, the title bar, menus, alerts, the login screen (its fields: AppKit's I-beam) | Arrow | AppKit's default |

Discord also shows the pointer over the channel and DM rows and the server icons; Akari's show the
arrow there, as macOS lists do.
