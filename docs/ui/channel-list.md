# Channel list

The column between the server rail and the messages: a server's channels, or the DM list at
home. Numbers are for Discord's redesigned client (2025) at 100% zoom and default density,
measured from the web client on 2026-10-10; colors in [theme.md](theme.md).

Which channels appear and in what order is decided by akari-core (`Store::channel_list`,
[dispatches.md](../protocol/dispatches.md#lists-for-display)); this page is about how they
look and behave.

## Header

The server's name at the top of the column, in semibold `textStrong`, 48 tall like the channel
header next to it. Text sizes of the whole column are in [typography.md](typography.md).
Discord opens a server menu from it; Akari only shows the name for now. At home the same
header says "Direct Messages" (see [Home](#home)).

The header stays in place; only the list under it scrolls, so no row ever slides under the
title bar.

## Categories

- A row 24 tall with 16 of space above it (8 in compact density), 16 padding on the left
  and 8 on the right.
- The name as the server wrote it, in medium weight (14 px in the official client) and
  `channelDefault`. Before the redesign category names were uppercase and 12; now they keep
  their case.
- A small chevron (12) **after** the name, pointing down when open and turned −90° when
  collapsed.
- Clicking the row collapses or expands the category. The state is kept per server.
- A collapsed category still shows, under it:
  - the selected channel;
  - unread channels that aren't muted;
  - channels with mentions or active threads the user joined;
  - voice channels with people in them.

  *Akari: the same, except threads and voice channels with people in them, which it doesn't
  list yet.*
- A muted category's name shows at half opacity. Muting a category collapses it at once, its
  channels still look normal (muting the category doesn't mute them), and while it is muted
  and collapsed only a channel with a mention stays under it (observed 2026-10-14). *Akari:
  collapse is local, as in Discord's clients, which don't sync it, and kept per server. The
  official client collapses a category only when it is muted in that same client; Akari
  never collapses one on its own.*

## Channels

- Each row is inset 8 from the left, has corner radius 8 and padding 4 vertically and 8
  horizontally: a 20 icon, 8 of space, then the name on a 24 line. That makes rows 32 tall,
  2 apart.

| State | Background | Name |
|---|---|---|
| Normal | none | `channelDefault` |
| Hovered | `hoverBackground` | `interactiveTextActive` |
| Selected | `selectedBackground` | `interactiveTextActive` (semibold in light) |
| Unread | none | `interactiveTextActive`, plus a 4 × 8 pill on the column's left edge |
| Muted | none | icon and name at half opacity |

Names are medium weight in every state; the official client changes only their color (observed
2026-10-14 in the dark theme).

### What makes a row unread

Observed in the official desktop client on 2026-10-14:

- A text or announcement channel turns unread when a message newer than the user's read
  position arrives, also in a channel the user never opened.
- **Voice and stage channels never look unread from their text chat:** a message there
  leaves the row as it was, with no bright name and no pill. The gray count the support
  article describes isn't what the client shows. **Unverified:** what a mention there shows.
- **A forum or media channel** looks unread when a new post appears, without a count.
- **Threads the user hasn't joined** aren't listed. Starting one posts a "started a thread"
  message in its parent, which makes the parent unread like any message; messages inside the
  thread leave the parent alone.
- **Deleting messages doesn't clear a row:** with its only unread message deleted, the row
  stays unread, and with a message that mentioned the user deleted, it keeps its count, until
  the channel is read.
- An @silent mention counts like any other; it only skips the notification.

*Akari: an unread row has the bright name and the 4 × 8 pill on the column's left edge (not
while selected), and a row with mentions the red badge at its right end, as on the rail; DM
rows the same, without the pill.*

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
- Muted rows and categories are drawn at half opacity; the unread style comes with unread
  state. A muted channel that is selected keeps full opacity.
- "Hide muted channels" (a server setting) leaves channels muted by their own settings out of
  the list, except the open one. **Unverified:** what the official client does with a muted
  category there; Akari keeps categories.

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
