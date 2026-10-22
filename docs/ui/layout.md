# Window layout

How the main window is divided and how big each part is. The numbers are for Discord's
current desktop client (after its 2025 redesign) at 100% zoom and default density, measured
from the web client on 2026-10-10. Discord's pixels at 100% zoom map 1:1 to points on macOS.

Sources: Discord's March 2025 changelog
(https://discord.com/blog/discord-update-march-25-2025-changelog), which announced the
redesign, and measurements of the web client.

## Overview

```
┌──────────────────────────────────────────────────────────────────┐
│ ● ● ●             title bar: location, centered                  │ 32
├──────┬──────────────────┬────────────────────────────────────────┤
│      │ server name      │ # channel  │  topic                    │ 48 + 1
│ rail │──────────────────│────────────────────────────────────────│
│      │ channel list     │ messages                               │
│  72  │ 302 (resizable)  │                                        │
│      │                  │                                        │
│ ┌────┴──────────────────┴─┐                                      │
│ │ user panel (floating)   │                                      │
│ └─────────────────────────┘                                      │
└──────────────────────────────────────────────────────────────────┘
```

The member list on the right (264 wide) is not part of Akari yet. A thread can open in a split
view on the right instead ([below](#split-view)).

## Title bar

- 32 tall, in the `frame` color (see [theme.md](theme.md)).
- On macOS it holds the traffic lights, and its own content starts after the rail plus 16.
- A centered title says where the user is: the server's name, or "Direct Messages" at home.
  Measured on 2026-10-14 (dark theme): icon and name together are centered on the whole
  window, not on the area right of the sidebar; the icon is the server's, a 16 rounded square
  with corners of about 4, then 8 of space and the name in `textDefault`, medium weight (size
  in [typography.md](typography.md)), both centered on the bar's height. *Akari: servers
  without a loaded icon (all of them so far) show their initials tile, as in the rail.*
  **Unverified**: the light theme's colors, and what the official client shows while a DM is
  open; Akari then shows the conversation's avatar and name, and "Direct Messages" with the
  rail's home symbol at home without one.
- Nothing scrolls under it or the traffic lights: the rail, the channel or DM list and the
  message list all stop at its lower edge or below.
- Discord also puts back/forward buttons and an Inbox button there. Akari leaves those out
  until it has navigation history and an inbox.

Akari keeps the native macOS title bar and makes it transparent over a 32 pt strip, so
dragging, double-click to zoom, full screen and the traffic lights behave like every other Mac
app. The native title is hidden and Akari draws its own centered one; the window's title is
still set to the same text, so the Window menu, Mission Control and VoiceOver read it.

## Server rail

- 72 wide: 40 icons with 16 on each side.
- 8 between icons, 4 above the first one.
- Details in [server-list.md](server-list.md).

## Sidebar

The rail and the channel list together form the sidebar.

- It is resizable: 264 to 432 wide, 375 by default. Discord also moves it by 10 with the
  arrow keys while its handle has focus.
- The channel list is the sidebar minus the rail and a 1 pt line: 302 by default.
- The rail and the title bar form one L-shaped frame in the `frame` color, and the area inside
  it (channel list and message area) has a rounded top-left corner. A 1 pt `frameBorder` line
  runs along that area's top edge across the whole window and down its left edge, beside the
  rail; its corner's outer radius is about 11.5 (measured 2026-10-14, 11.4). The other edges
  run to the window's edges.
- The line is one color all along: over the message area it's the same `#222225` as over the
  channel list (dark), so Akari lays it over the frame's color there.
- The channel list has the rail's color, so the corner shows only in the line.
  **Unverified** in the light theme.
- At home the channel list's place holds the DM list
  ([channel-list.md](channel-list.md#home)).

## Channel header

- 48 tall, plus a 1 pt `borderSubtle` line at the bottom. The header above the channel list
  (the server's name, or at home the quick switcher's button) has the same height, and the line
  runs on under it: one line at one height across both columns.
- 16 padding on the left, 8 between toolbar buttons on the right.
- A guild channel shows its icon, its name, a short divider and the topic on one line. A DM
  shows the other person's name.
- A thread opened full width shows its channel first: "# new › [thread] t1", the channel's
  symbol and name, a chevron, the thread's symbol and its name (observed 2026-10-17). Clicking
  the channel's name opens the channel. The official client also has Threads, the bell, pins,
  members, "…" and a search field on the right. Its "…" for a thread: Join Thread or Leave
  Thread | Jump To Top, Open in Split View | Copy Link, Copy Thread ID (observed 2026-10-17).
  *Akari: the channel's name in `interactiveText` at the header's size with the pointing hand,
  the chevron 11 semibold `textMuted`; not measured yet. Of the buttons only "…", for threads,
  with Join Thread or Leave Thread (disabled while the thread is archived); the other items
  come later. A join or leave that fails shows Discord's alert: "Cannot join thread" / "This
  thread has reached the maximum possible number of members." when it's full, else "Error" /
  "An error occurred, please try again."*

## Split view

A thread opened from its channel's messages (the box under a message, the thread's name in a
"started a thread" line, or "Open in Split View" in the thread's "…") opens on the right of its
channel, which stays open and selected in the sidebar. Read from Discord's web client (its thread
sidebar, canary 645444, 2026-10-21) and observed 2026-10-17:

```
│ # channel           ╮░│╭ [thread] t1          … ✕ │
│─────────────────────│░││──────────────────────────│
│ messages            │░││ the thread's start,      │
│                     │░││ messages                 │
│ [composer]          │░││ [composer]               │
                       8   480 (450 … page − 315)
```

- **Width:** 480 at first, at least 450, at most the page (everything right of the sidebar)
  less 315, so the channel keeps 307 beside it. Dragging the gap resizes it; Discord keeps one
  width for every thread. *Akari keeps a dragged width until it quits; each launch starts at 480.*
  (Discord's own split, open while the page loads, comes up at 450: its code clamps the width
  before the page is measured.)
- **The gap:** 8 of the frame's color between the channel and the split, which is the drag
  handle: the cursor changes, nothing else, and stays changed for the whole drag, also when the
  pointer passes the width's limits. No keyboard resizing.
- **Borders:** the channel gets a 1 right border that curves into its top with a 12 corner; the
  split has a 1 border on its top and sides in the frame border's color and a 12 top-left corner.
  The top-right corner is square.
- **Too narrow for both** (the page less 315 is 450 or less): the split floats over the
  channel's right side, 450 wide and full height, with a shadow (0 8 40, black at 52%) and its
  border in the chat's color; the channel keeps its full width underneath and there's no handle.
  *Akari's minimum window with the default sidebar is such a page.* Whether the official client
  still acks the channel under a floating split is open (Akari reads both, as when docked; at
  Akari's minimum window, 940, the split leaves 115 of the channel uncovered with the default
  sidebar, 58 with the widest and 226 with the narrowest).
- **Header:** 48 plus its 1 line, like the channel header. 16 padding on the left: the thread
  symbol in a 20 box at 15 (`channelDefault`), 9, then the name in the channel header's style,
  cut off with an 8 fade instead of an ellipsis; clicking the name opens the thread full width.
  On the right, 8 from the edge and 8 apart, 32 square buttons with 20 icons in `textMuted`
  (`interactiveText` hovered), no background: "…" ("More") and ✕ ("Close"). Discord also has a
  notification bell (*Akari: later, with thread settings*). No parent before the name.
- **The "…":** Join Thread or Leave Thread, Open in Full View | Mute Thread, Notification Settings
  | Search, Pins | Copy Link, Copy Thread ID. Joining and leaving there tell Discord "Sidebar
  Overflow". *Akari: Join or Leave Thread and Open in Full View, only the latter when the
  thread's record lacks its state.* The full view's "…" offers "Open in Split View".
- **Inside:** the thread's start, messages, composer and its placeholder (`Message "t1"`) as in
  the full view, same sizes. Opening the split puts the focus in its composer.
- **Per channel:** each channel keeps its own split. Opening another channel hides it; coming
  back shows it again, also after a relaunch. The X, opening the thread full width from its row,
  clicking the open channel's own row, and Escape with nothing left to do close it for good.
  "Open in Full View" and the name only open the thread full width: back in the channel, the
  split is there again. Deleting the thread closes it; closing (archiving), locking or leaving a
  public thread doesn't. Losing the thread closes and forgets it: Discord's split store drops
  an entry the user can no longer view, which for a private thread means being removed from it
  or leaving it (without Manage Threads), and for any thread losing its channel's view. *Akari
  also asks Discord once per launch about a remembered thread READY didn't list as joined (the
  cache keeps those, deleted ones too) and drops it when Discord says it's gone or hidden.
  Leaving a server forgets its splits; an outage keeps them.*
- **Reading:** both lists are read at once, each while it's at its bottom; a split that appears
  on its own (a remembered thread Discord answered for after a relaunch) is read after the next
  click or key press. Escape in either marks both read and jumps both to the present; with
  nothing left in either, it closes the split
  ([message-list.md](message-list.md#reading-the-signal-to-ack), [shortcuts.md](shortcuts.md)).
- No animation: it opens and closes at once. Discord's member list and the split share the
  right side; Akari has no member list yet.

## User panel

- A card that floats over the bottom of the sidebar and spans **both** the rail and the
  channel list.
- 8 from the left, right and bottom edges; 56 tall; corner radius 8; a 1 pt `borderMuted`
  outline; the `panel` color.
- Discord shows the avatar with a status dot, the display name with a status line under it,
  and microphone, headphones and settings buttons 8 apart. The name hides when the panel
  gets narrower than 100.
- The avatar size isn't measured; 32 is our estimate. **Unverified.**
- The lists above the panel scroll underneath it, so their last rows need 72 of space at the
  bottom.

Akari shows the avatar, the display name and the username. It leaves out the status dot
(presence isn't tracked yet) and the voice and settings buttons (no voice or settings yet).

## Message area and composer

- The message list fills the rest; its layout is in [message-list.md](message-list.md).
- The composer at the bottom is 56 tall for one line, with 8 margin on the sides and the
  bottom and corner radius 8, so it lines up with the user panel; a thin status strip sits
  directly above it, over the message list, which runs on behind it down to the composer.
  Details in [composer.md](composer.md).

## Connection bar

Akari shows a bar across the window, right under the title bar, while it isn't connected to
Discord. It is 32 tall and pushes the content below it down.

- Connecting for the first time, or reconnecting after the connection dropped, after a second:
  `panel` background, `textMuted` text and a small spinner ("Connecting to Discord…",
  "Connection lost. Reconnecting…").
- Offline while the Mac sleeps: "Offline".
- Closed for good by an error: `danger` background, white text naming the error, and a
  "Reconnect" button. A rejected login goes back to the login screen instead.

The official desktop client shows no such bar for short drops: messages just stay gray and
then turn red, and a long outage brings back its full-window loading screen (**unverified**).
Its mobile app shows "Connecting…" at the top. Akari's bar is its own design.

## Window size

- Minimum 940 × 500. **Unverified**: the value comes from a 2019 build of the desktop app.

## What the 2025 redesign changed

People may remember the older client, so the differences are worth knowing:

- The channel list now has the same color as the rail; only the message area is lighter.
- The user panel floats across both columns instead of sitting under the channel list.
- Category names are no longer uppercase.
- The sidebar can be resized.
- A title bar runs across the top of the window.
