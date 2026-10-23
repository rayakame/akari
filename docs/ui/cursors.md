# Cursors

Which cursor the pointer shows where, and what decides it. Each region has one authority, or
leaves the cursor to AppKit; none sets a cursor over another's region. Audited 2026-10-23 after
Chris's tests of #29, where views behind a popout and neighbouring message rows fought over it.

| Region | Cursor | Authority |
|---|---|---|
| The message list (each of the channel's and the split's): rows, a message's text, links, mentions, hidden spoilers and code blocks, thread boxes, Retry and Delete, Try again; the bars over it (unread, jump, locked, status) | I-beam over text, pointing hand over what clicks, arrow elsewhere and over the bars | The list's tracking area (`MessageTableController`): it hit-tests on every move, scroll, update and key change, while its window is key and on top at the pointer. Text views and boxes track nothing themselves. |
| The composer | I-beam | AppKit (NSTextView's own) |
| The channel header: the "# parent ›" link, the Threads button, the thread's "…" | Pointing hand | `CursorArea` cursor rects |
| The split's header: its title, "…", the X | Pointing hand | `CursorArea` cursor rects |
| The split's gap and the sidebar's edge | Resize left and right, held through a drag | `ResizeCursor`: one push on hover or a drag's start, one pop at the end or when the view goes |
| The Threads browser's panel (key): cards, the modal's X, the field's clear button | Pointing hand | `CursorArea` cursor rects |
| The browser's search field | I-beam | AppKit (the field is AppKit's own, in the key panel) |
| The shield under the browser | Arrow | Its own tracking area (`ShieldWindow`) |
| More Active Threads (never key) | Arrow | AppKit's default; a window that's never key gets no cursor rects |
| Everything else: the server rail, the channel and DM lists, the user panel, the title bar, menus, alerts, the login screen (its fields: AppKit's I-beam) | Arrow | AppKit's default |

Discord shows the pointer over every clickable thing (rows, server icons, the bars over the
list); Akari shows it only where the table says, the robust choice over the exact one (D126).
