# Channel list

The column between the server rail and the messages: a server's channels, or the DM list at
home. Numbers are for Discord's redesigned client (2025) at 100% zoom and default density,
measured from the web client on 2026-10-10; colors in [theme.md](theme.md).

Which channels appear and in what order is decided by akari-core (`Store::channel_list`,
[dispatches.md](../protocol/dispatches.md#lists-for-display)); this page is about how they
look and behave.

## Header

The server's name at the top of the column, 48 tall like the channel header next to it.
Discord opens a server menu from it; Akari only shows the name for now. At home the same
header says "Direct Messages" (see [Home](#home)).

The header stays in place; only the list under it scrolls, so no row ever slides under the
title bar.

## Categories

- A row 24 tall with 16 of space above it (8 in compact density), 16 padding on the left
  and 8 on the right.
- The name as the server wrote it, in 14 medium weight and `channelDefault`. Before the
  redesign category names were uppercase and 12; now they keep their case.
- A small chevron (12) **after** the name, pointing down when open and turned −90° when
  collapsed.
- Clicking the row collapses or expands the category. The state is kept per server.
- A collapsed category still shows, under it:
  - the selected channel;
  - unread channels that aren't muted;
  - channels with mentions or active threads the user joined;
  - voice channels with people in them.

  Akari shows only the selected channel until it tracks unread state.
- A muted category's name shows at half opacity.

## Channels

- Each row is inset 8 from the left, has corner radius 8 and padding 4 vertically and 8
  horizontally: a 20 icon, 8 of space, then the name on a 24 line. That makes rows 32 tall,
  2 apart.

| State | Background | Name |
|---|---|---|
| Normal | none | `channelDefault` |
| Hovered | `hoverBackground` | `interactiveTextActive` |
| Selected | `selectedBackground` | `interactiveTextActive`, medium weight (semibold in light) |
| Unread | none | `interactiveTextActive`, medium weight, plus a 4 × 8 pill on the column's left edge |
| Muted | none | icon and name at half opacity |

- Icons by kind: text, voice, announcement, stage, forum, media, rules and thread. Each has a
  private variant (with a lock) and an NSFW variant (with a warning mark).
- Voice channels list the people connected to them underneath.
- Clicking a text or announcement channel opens its messages. Clicking a voice channel joins
  it.

### In Akari

- Icons are SF Symbols. NSFW channels get a small warning badge on their symbol. The private
  variant needs the channel's permission overwrites, which the app doesn't receive yet; rules
  channels need the server's rules channel ID, which it doesn't receive either.
- Voice, stage, forum, media and directory channels are listed but don't open anything; their
  tooltip says the kind isn't supported yet.
- Unread and muted styles come with unread state.

## Home

At home the column holds the DM list instead of a server's channels.

- At the top, in a header as tall as a server's, Discord has a "Find or start a conversation"
  button that opens the quick switcher. *Akari: the header says "Direct Messages" until the
  quick switcher exists; then it can become that button.* The line under it runs on under the
  channel header, as with a server.
- Above the list, Discord has entries for Friends, Nitro and the Shop. *Akari: left out.*
- A "Direct Messages" label, 24 tall, with a + button for a new DM on its right. *Akari: left
  out while the header above says the same; it comes back with the + button or when the
  header becomes the switcher button.* The rows start 8 below the header line.
- One row per conversation: the avatar, the name, and for a group the number of members on a
  second line ("3 Members"). **Unverified**: the exact wording of the member line.
- A conversation's name is the other person's display name. A group shows its own name, or
  the members' names separated by commas when it has none.

### DM order

1. Conversations the user marked as favorites. *Akari: not available yet; favorites are in the
   settings proto.*
2. The rest, the most recent first: by the newest message, or for a conversation without
   messages by when it was created. Ties go by channel ID, newest first.

Discord leaves message requests and conversations marked as spam out of this list. *Akari
keeps them in the list for now*, because the store doesn't keep those two flags yet
([dispatches.md](../protocol/dispatches.md#open-points)).

A new message moves its conversation to the top at once.
