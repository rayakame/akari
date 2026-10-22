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
Discord opens a server menu from it; Akari only shows the name for now. At home the header
holds Discord's "Find or start a conversation" button (see [Home](#home)).

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
  - channels with mentions;
  - a channel with joined threads that are unread or have mentions, with only those threads
    ([Threads](#threads));
  - voice channels with people in them.

  *Akari: the same, except voice channels with people in them, which it doesn't list yet.*
- A muted category's name shows at half opacity. Muting a category collapses it at once, its
  channels still look normal (muting the category doesn't mute them), and while it is muted
  and collapsed only a channel with a mention stays under it (observed 2026-10-14). Its threads
  aren't affected: under a collapsed muted category an unread thread stays with its parent, with
  or without a mention, and expanded by hand the category shows all its channels' threads
  (observed 2026-10-17). *Akari:
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
- **Threads the user joined** are listed under their parent channel, each with its own unread
  state and count (observed 2026-10-15). The parent's row doesn't change for them; the server's
  pill and badge do ([Threads](#threads)).
- **Threads the user hasn't joined** aren't listed. Starting one posts a "started a thread"
  message in its parent, which makes the parent unread like any message; messages inside the
  thread leave the parent alone.
- **Deleting messages doesn't clear a row:** with its only unread message deleted, the row
  stays unread, and with a message that mentioned the user deleted, it keeps its count, until
  the channel is read.
- An @silent mention counts like any other; it only skips the notification.
- **In community servers with "Channels & Roles"** (from the official client's code): a
  channel whose newest message is over a week old shows neither unread nor a count, and a
  channel the user didn't pick shows unread only when it has mentions.
- **Server Guide resource channels** never look unread and never show a count, in any server
  (from the official client's code; observed 2026-10-15 on a channel Discord reports unread
  with a mention). The official client does list them in the normal channel list (observed
  2026-10-15); only the badge is left out. Akari lists them too.

*Akari: an unread row has the bright name and the 4 × 8 pill on the column's left edge (not
while selected), and a row with mentions the red badge at its right end, as on the rail.*

- Icons by kind: text, voice, announcement, stage, forum, media, rules and thread. Each has a
  private variant (with a lock) and an NSFW variant (with a warning mark).
- Voice channels list the people connected to them underneath.
- Clicking a text or announcement channel opens its messages. Clicking a voice channel joins
  it.

### Context menu

The first item of a channel row's menu is "Mark As Read" (observed 2026-10-15 in the German
client as "Als gelesen markieren"; the English label is unconfirmed). The official client
disables the DM row's item instead of hiding it when nothing is unread ([below](#home)); the channel
row's was not observed. *Akari: disables both when nothing is unread.*

### In Akari

- Icons are SF Symbols at 18 points in the 20 box: Discord's # is ink 17 by 17 with strokes 1.5
  thick (measured at 2x on 2026-10-18); SF's `number` at 18 is 16 by 17, a little lighter (its
  strokes about 12% heavier than at 16, Discord's about 19%). Medium weight would be about a
  third heavier, so the symbols keep the regular weight. Every kind takes the same size; the
  widest (forum 25.5, group 24, media 22) reach past the 20 box without moving the name.
  NSFW channels get a small warning badge on their symbol. The private
  variant needs the channel's permission overwrites, which the app doesn't receive yet; rules
  channels need the server's rules channel ID, which it doesn't receive either.
- Voice, stage, forum, media and directory channels are listed but don't open anything; their
  tooltip says the kind isn't supported yet.
- Muted rows and categories are drawn at half opacity, except a row's mention count, which
  stays full red. A muted channel that is selected keeps full opacity. VoiceOver reads a row's
  mute, unread state and count after its name ("muted, unread, 2 mentions"), and a thread row as
  "name, thread in channel".
- "Hide muted channels" (a server setting) leaves channels muted by their own settings out of
  the list, except the open one and one with unread or mentioned threads
  ([Threads](#threads)). **Unverified:** what the official client does with a muted category
  there; Akari keeps categories.

## Threads

Observed in the official web client on 2026-10-17, with the second account and a test server.
A text or announcement channel with active threads the list doesn't show offers them in "More
Active Threads" when the pointer rests on its row ([thread-browser.md](thread-browser.md#more-active-threads)).

- **Which threads:** those the user joined, under their parent channel, while they aren't
  closed and are inside their "Hide After Inactivity" window, or are unread or have a mention.
  A thread past its window that the user hasn't read stays listed, also after a reload; once
  read, it goes. Threads the user hasn't joined aren't listed. akari-core decides this
  ([dispatches.md](../protocol/dispatches.md#threads)).
- **Order:** the last joined first, by the user's own `join_timestamp` (the time the user last
  joined the thread); equal times go by thread ID, newest first. New messages don't move a
  thread; joining or rejoining moves it to the top. Observed 2026-10-19 with the second account:
  t7, t3, t1, t2, thread-test, exactly their join times newest first, while creation order would
  swap t1 and t2 (t1 was created first but joined later). The day before, t6, joined after t3,
  led the list.
- **Look:**
  - no icon, only the name, in rows as tall as channel rows;
  - joined to the parent by a vertical line, the spine, with a short branch to each row (├,
    and └ for the last); its geometry, measured at 2x on 2026-10-18, is [below](#the-spine);
  - read in gray; unread in white with the pill on the column's left edge; mentions with the red
    badge at the right; muted dimmed like a muted channel; selected with the channel rows'
    selected background.
- **The parent's row** doesn't change for its threads' unread state or mentions.
- **Collapsed category:** the parent stays with its unread and mentioned threads; read threads
  hide. Under a muted category too, where a channel without threads stays only with a mention.
- **Muted parent:** neither a muted channel nor a muted category changes its threads' unread
  state, dots or badges. While a muted channel isn't the open one, only its unread and
  mentioned threads show; opening it shows them all again.
- **Hide muted channels:** a muted channel it would hide stays while it has unread or mentioned
  threads, with only those threads.
- **A muted thread** is dimmed like a muted channel. A plain message changes nothing visible; a
  mention adds the red badge. "Only @mentions" still shows plain messages as unread.
- **The open thread** (opened full width) always has a row. When it stops being listed (the user
  left or closed it, or read it past its window), the row moves to the top of its parent's
  threads and goes when the user opens something else. An unlisted thread opened full width,
  such as one the user hasn't joined, gets that row too, selected. When its parent leaves the
  list (the user can't see it any more, nor its threads), the thread gives way as a hidden
  channel does: the server's last or first channel opens.
- **In the split view** (a thread beside its parent, [layout.md](layout.md#split-view)) the
  thread has no row of its own and the parent stays selected, whether the thread is joined or
  not; a joined thread keeps its ordinary listed row, never highlighted. Leaving the thread
  there removes its row at once (observed 2026-10-17). A thread's row opens it full width and
  closes its channel's split for good, and so does clicking the open channel's own row
  (Discord's web client, read 2026-10-21; the second inferred from its router).
- **The row's menu:** Mark As Read (disabled when read) | Leave Thread, Copy Link |
  Mute Thread ›, Notification Settings › | Copy Thread ID. The official client's thread menu
  offers Join Thread instead for a thread the user isn't in, such as the open one's row.
  *Akari: Mark As Read | Join Thread or Leave Thread (disabled while archived); the other items
  later.*

### The spine

Measured side by side at 2x in the dark theme (Discord on the #121214 sidebar), in points from
the column's left edge; Akari draws the same:

- **Width and color:** 2 wide, opaque `#474850` (`threadSpine`; the light theme's `#aaabb3`
  keeps the same 2.06:1 contrast on its sidebar, **unverified** against Discord's light theme).
- **Where:** centered 26 from the edge, under the center of the parent's symbol.
- **Top:** at the first thread row's top edge, with a round cap reaching 1 into the 2 between
  rows, 8.5 below the ink of the parent's # (the parent row ends 1 above it).
- **Between rows:** each row's piece runs to the row's bottom and the next starts 2 above its
  top, so consecutive pieces meet without a seam or overlap.
- **Branch:** 2 thick at the row's vertical center, with a round cap; its right end, cap
  included, is 9 right of the spine's center (35 from the edge). The row's background starts at
  36, the name at 44 (19 right of the spine's center), under the parent's name.
- **Joint (middle rows):** fillets of 2 radius above and below where the branch leaves the
  spine, so it reads as a smooth T.
- **Last row:** no spine below the branch; the spine bends into it with a radius of 4 at its
  center line (5 outside, 3 inside), starting 4 above the row's center.

*Akari draws each row's piece as one filled outline, so the anti-aliased edges never add up.*

## Home

At home the column holds the DM list instead of a server's channels.

- At the top, in a header as tall as a server's, Discord has a "Find or start a conversation"
  button that opens the quick switcher. *Akari: the header stays empty until the quick switcher
  exists.* The line under it runs on under the channel header, as with a server.
- Above the list, Discord has entries for Friends, Nitro and the Shop. *Akari: left out.*
- A "Direct Messages" label in a category's style (small, `channelDefault`, not the server
  header's `textStrong`), 24 tall, with a + button for a new DM on its right (observed
  2026-10-17). *Akari: the label without the + button until new DMs can be created; it starts
  8 below the header line and stays in place while the rows, 2 below it, scroll.*
- One row per conversation: the avatar, the name, and for a group the number of members on a
  second line ("3 Members"). **Unverified**: the exact wording of the member line.
- An unread row looks like an unread channel row: the bright name and the pill on the column's
  left edge. **No count in the list:** a DM's mention count shows only on its avatar on the
  rail (observed 2026-10-17). *Akari: the same; VoiceOver still reads the count with the row.*
- A conversation's name is the other person's display name. A group shows its own name, or
  the members' names separated by commas when it has none.
- The first item of a row's context menu is "Mark As Read" (German: "Als gelesen markieren";
  the English label is unconfirmed). With nothing unread it is disabled, not hidden (observed
  2026-10-15). *Akari: the same.*

### DM order

1. Conversations the user marked as favorites. *Akari: not available yet; favorites are in the
   settings proto.*
2. The rest, the most recent first: by the newest message, or for a conversation without
   messages by when it was created. Ties go by channel ID, newest first.

Discord leaves message requests and conversations marked as spam out of this list, and so does
Akari.

A new message moves its conversation to the top at once.
