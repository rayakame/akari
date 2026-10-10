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

The member list on the right (264 wide) is not part of Akari yet.

## Title bar

- 32 tall, in the `frame` color (see [theme.md](theme.md)).
- On macOS it holds the traffic lights, and its own content starts after the rail plus 16.
- A centered title says where the user is: the server's name, or "Direct Messages" at home.
- Nothing scrolls under it or the traffic lights: the rail, the channel or DM list and the
  message list all stop at its lower edge or below.
- Discord also puts back/forward buttons and an Inbox button there. Akari leaves those out
  until it has navigation history and an inbox.

Akari keeps the native macOS title bar and makes it transparent over a 32 pt strip, so
dragging, double-click to zoom and the traffic lights behave like every other Mac app.

## Server rail

- 72 wide: 40 icons with 16 on each side.
- 8 between icons, 4 above the first one.
- Details in [server-list.md](server-list.md).

## Sidebar

The rail and the channel list together form the sidebar.

- It is resizable: 264 to 432 wide, 375 by default. Discord also moves it by 10 with the
  arrow keys while its handle has focus.
- The channel list is the sidebar minus the rail and a 1 pt line: 302 by default.
- The channel list has the rail's color. A 1 pt `frameBorder` line runs along its left
  and top edges.
- At home the channel list's place holds the DM list
  ([channel-list.md](channel-list.md#home)).

## Channel header

- 48 tall, plus a 1 pt `borderSubtle` line at the bottom. The header above the channel list
  (the server's name, or "Direct Messages" at home) has the same height, and the line runs on
  under it: one line at one height across both columns.
- 16 padding on the left, 8 between toolbar buttons on the right.
- A guild channel shows its icon, its name, a short divider and the topic on one line. A DM
  shows the other person's name.

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
  directly above it. Details in [composer.md](composer.md).

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
