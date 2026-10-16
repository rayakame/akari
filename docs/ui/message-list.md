# Message list

The messages of the open channel. Numbers are for Discord's redesigned client (2025) at 100%
zoom, in the default ("cozy") message display and default density, measured from the web
client on 2026-10-10; colors in [theme.md](theme.md).

## Layout

- Messages are listed oldest at the top, newest at the bottom. While the user is at the
  bottom, new messages keep the list at the bottom; when they scrolled up, the list stays
  where it is.
- At its top and bottom the list bounces with the native macOS elastic overscroll, unlike the
  official client, whose Chromium list stops there. While the user scrolls, momentum and
  bounce included, the list never moves on its own, and a position past the bottom counts as
  being at the bottom.
- Each message has 2 of vertical padding and 16 on the right. Its content starts 72 from the
  left: 16 margin, a 40 avatar column, 16 gap.
- Text is 16 on a 22 line height. Sizes of every text style are in
  [typography.md](typography.md).
- Hovering a message tints its **box** with `hoverBackground`: the message with its 2 of
  padding above and below, not the space before a group (see below), and not the 16 at the
  right edge where the official client keeps its scrollbar. Under macOS's legacy scrollers the
  scroller has a gutter of its own, and the box runs up to it. Hovering the space before a group
  or the scrollbar's gutter hovers no message (observed 2026-10-14).

## Groups

Consecutive messages from one author form a group. The first message of a group shows the
author; the rest show only their content.

**The group's first message:**

- 16 of extra space above it, outside its box: hover and the mention highlight leave it out,
  so two highlighted groups in a row show a gap (observed 2026-10-14).
  *Akari: 16, the official setting's default; the official client measured 17 between two
  boxes and 48 for a one-line group start's box, where Akari's is 47 with the name on a 19
  line. Akari keeps its rhythm for now.*
- Right after a day divider the space above it is smaller: its box starts 17.5 below the
  divider's line, its avatar 21.5 (measured 2026-10-14). *Akari: 6 instead of 16 below the
  divider's row; system messages there move up by the same 10.*
- Inside the box, measured at 2 px per point on 2026-10-14: the avatar (40) in the left column
  starts 4 below the box's top, the name's baseline sits 18.5 below it and the first text
  line's 41. *Akari: avatar and name line 4 into its 47 box (name baseline at 19), text
  baseline 41, the avatar 3 above the box's bottom; a group start whose content is shorter than
  the avatar is as tall as a one-line one.*
- The display name in medium `textStrong`. Bots carry a small tag after the name ("APP"
  since 2024). **Unverified** that webhooks get the same tag.
- The time, 4 after the name, in medium `chatTextMuted` (12 px in the official client;
  Akari's sizes in [typography.md](typography.md)).

**The other messages:** no avatar and no name. Hovering one shows its time (hours and minutes)
in the left column, right-aligned in a 56 wide space. Their text sits where the first text line
of a group start sits in its line: its baseline 19 below the box's top (derived from the group
start's measurement; *Akari: 3 above the text, 1 below*).

### When a new group starts

This is the rule Akari implements on every platform. Each message is compared with **the
message shown right before it** (sent, pending and failed messages alike). It starts a new
group when any of these holds:

1. It is the first message shown.
2. Its local calendar day is later than the previous message's. A day divider sits between
   them (see [Day dividers](#day-dividers) for a day that goes backwards).
3. It or the previous message is a system message (see below).
4. Its author's user ID differs from the previous message's.
5. Both are webhook messages and their author names differ. One webhook can post under
   several names.
6. It is a reply, or the result of a slash command or a context-menu command.
7. Its timestamp is **7 minutes (420 seconds) or more** after the previous message's. At
   419 seconds it still continues the group.

Otherwise it continues the previous message's group.

Discord also starts a group for a message that has a thread, when ephemeral, scheduled,
blocked or silent state changes, when the posting application changes, and for the first
message of a forum post. Akari applies those once its records carry the fields they need.

## Times

The time in a group's first line:

| When | Shown as (en-US) |
|---|---|
| Today | the time only, "2:05 PM" (observed 2026-10-14) |
| Yesterday | "Yesterday at 11:59 PM" (**unverified** since the today form changed) |
| Earlier, or a date in the future | numeric date and short time, "10/03/2026, 2:05 PM" (**unverified** since then) |

All of them follow the user's locale and time zone, also when those change while the app
runs: at midnight "2:05 PM" becomes "Yesterday at 2:05 PM", and a new time zone or locale
redraws the times and moves day dividers without reopening the channel. Hovering the time
shows the full date and time. Discord writes the older form without the comma; Akari uses the
platform's date format for the locale.

### Locale

Every date and time goes through the platform's locale-aware formats (on Apple platforms
`Date.FormatStyle` fields, the same templates `DateFormatter.setLocalizedDateFormatFromTemplate`
takes); no app has a fixed date pattern. The order, the separators and 12 or 24 hours follow
the user's region and the system's 24-hour setting; the words (month and weekday names,
"Yesterday at") follow the app's language. Akari is English only for now, so on a German Mac
its dates are those of `en_DE`: "14. October 2026", "03.10.2026, 14:05". German words come with
a German localization of Akari, a milestone of its own.

## Day dividers

- One before the first message of each local calendar day in the list.
- Days only move forward: a message stamped earlier than the one before it (a pending message
  from a device whose clock runs ahead or behind) gets no divider of its own and stays under
  the current day. That keeps one divider per day, so a divider's key never repeats.
- A divider stays while any message of its day is shown, also when the day's first message
  goes.
- A 1 pt `borderSubtle` line across the list with the date in the middle, in the locale's
  long form ("October 10, 2026"), semibold `textMuted`, on the list's background.
- The first message under it sits closer than a group usually does: its avatar 21.5 below
  the line (see [Groups](#groups)).

## System messages

Messages Discord writes itself (someone joined, a message was pinned, a boost, a call, a new
thread, a renamed channel, and so on) show as one line:

- a muted icon in the avatar column;
- a sentence naming who did what, in `textMuted`;
- the time.

They never join a group and always end one. Akari writes these sentences in its own words; it
doesn't reuse Discord's texts, such as its random welcome lines.

**Which messages count as system messages:** every message type except the default type,
replies, slash-command and context-menu command results, thread starter messages, and types
Akari doesn't know. A type Akari doesn't know shows its content like an ordinary message.

## Loading history

- The list holds up to 200 messages of a channel. Beyond the loaded messages, while older ones
  exist, gray placeholder rows (an avatar circle and text bars, in a fixed mix of heights)
  fill about one and a half views above them, so a fling carries on into them instead of
  stopping at the top. While the list is detached from the present, the same placeholders sit
  below the newest loaded message. The official client does the same.
- A channel whose messages aren't loaded yet shows only placeholders, filling the area from
  the bottom as if it were at its newest messages, instead of a spinner; the official client
  does the same. The first page takes their place pinned to the bottom, and a channel shorter
  than the view shows its few messages at the bottom. If that page can't be loaded, the
  placeholders say so and offer "Try again".
- Pages of 50 load well before the reader gets there: once the top of the view is within three
  view heights of the oldest loaded message (or the bottom within three of the newest), and
  again right after a page lands while the reader is still that close.
- When a page lands, the real messages take the place of the placeholders and nothing the
  reader looks at moves: the first real message keeps its place on screen, also when only
  placeholders were in view, also while a fling or the bounce at the top is still running.
- Past 200 messages the newest are dropped, and the list no longer reaches the present
  ("detached"); scrolling back down loads newer pages the same way, dropping the oldest.
- If a page can't be loaded, the placeholders next to the loaded messages say so and offer
  "Try again". Nothing retries on its own, so a failing request doesn't repeat on every
  scroll, and a success at the other end doesn't retry it either.
- At the beginning of a channel, once the list holds its first message, a header sits above
  it, left aligned (see [Channel start](#channel-start)).
- Placeholder rows keep their height in every state, so their changes never move the
  messages.

## Channel start

Above the first message of a channel, when the history has reached it. Measured from the
official client on 2026-10-14:

- 16 from the list's left edge, like the avatars: a 68 circle (`startIcon`) with the channel's
  symbol in `textStrong`, about 35 tall.
- The title's baseline 39 below the circle: "Welcome to #general!" in heavy `textStrong`. A
  long name wraps.
- The subtitle's baseline 33.5 below the title's: "This is the start of the #general channel."
  in `textDefault`, the message text's color.
- Then the day divider of the first message, its line about 30 below the subtitle's baseline.

The official client also shows an "Edit Channel" button under the subtitle to those who may
edit the channel. *Akari leaves it out until it has channel settings.*

**DMs** show the other person's avatar (80) in the circle's place, their display name as the
title, their username under it, and "This is the beginning of your direct message history
with Mira." **Group DMs** show the group symbol, the group's name and "Welcome to the
beginning of the Weekend plans group." **Unverified**: the DM and group sizes, spacing and
wording, which no screenshot showed yet; the official DM header also has buttons (add friend,
block) that Akari leaves out.

## Jump to present

- While the list is detached, a bar sits at the bottom of the list, attached above the
  composer: on the left a note that older messages are shown, on the right "Jump to present"
  with a down arrow. It doesn't show after a short scroll up, only when the newest messages
  aren't loaded, as in the official client.
- Clicking it, or pressing Escape ([shortcuts.md](shortcuts.md)), loads the newest messages
  and scrolls to the bottom. At the present, Escape only scrolls to the bottom. Either way,
  Escape also marks the channel read.
- Sending a message also jumps to the present.
- Akari's bar: 32 tall, 16 from the sides, corner radius 8, `brand` with white 14 medium text:
  "You're reading older messages" and "Jump to present".

## Catching up

After a new session (a reconnect that couldn't resume), Akari checks the open channel for
messages it missed; so does the first session after a launch, for a channel shown from the
disk cache. While it does, a small capsule at the top of the list says "Catching up…"
with a spinner, in `textMuted` on `panel`, and it goes as soon as the check is done, also when
it found nothing new. If the check can't join the missed messages to the
list (more than 100 missed, or the request failed), the list becomes detached and the jump
bar says "Some messages may be missing" instead.

## Edits and deletes

- An edited message ends with "(edited)" right after its text, smaller (12) and in
  `chatTextMuted`, like the timestamps. A message without text puts it after its last line,
  and one that ends with a code block puts it on a line of its own.
  The official client's mark has a tooltip with the edit time (**unverified**); Akari adds it
  later.
- An edit that makes a message taller or shorter doesn't move the text the user is reading.
- A deleted message leaves the list without moving the text the user is reading, whether it
  was above, below or at the top of what's visible. At the bottom, the list stays at the
  bottom.

## Formatting and mentions

- Message content is formatted as Discord formats it: styles, code, quotes, lists, headings,
  spoilers, links, mentions and timestamps ([markdown.md](markdown.md)). The text can be
  selected and copied.
- A message that mentions the user (by name, through @everyone or @here, or through one of
  their roles) is tinted with `mentionedBackground` (`#f8a300` at 8% dark, 10% light) and has
  a 2 wide `mentionedBar` at its left edge; hovering darkens it like any row. Suppress settings
  don't change the highlight: the official client still tints a message whose @everyone, @here
  or role mention the server suppresses, and leaves out only its badge (observed 2026-10-14).
- Relative timestamps ("in 5 minutes") count on their own while they're on screen, and a row
  that grows or shrinks with them doesn't move the text being read. Akari stops counting while
  the window is hidden or minimized and catches up when it shows again.

## The new messages line and the unread bar

Observed in the official web client on 2026-10-15 (German UI).

- **An unread channel opens at the bottom**, at the present, not at the first unread message.
- **The line:** a red (`#d22d39`) line with a "NEW" tag at its right end, in the gap above the
  first unread message. It is a line of its own between two messages, not part of either.
- **It also appears while the user reads.** At the bottom, the first of several new messages
  gets the line above it. The ack that follows doesn't remove it. It goes as soon as the user
  sends a message.
- **The bar:** a blue bar at the top of the list, "1 new message since 17:13" (the German UI
  said it in German; the English wording is a translation), with a "Mark As Read" button on
  its right. It showed while the list was scrolled up and the new message was below the
  view. With three unread messages all visible there was no bar.
- **Escape while scrolled up** jumps to the bottom and marks the channel read
  ([shortcuts.md](shortcuts.md)).
- **Not observed yet:** the line next to a day divider, the line after Escape or after
  leaving the channel and coming back, and opening a channel with 80 or more unread messages
  and clicking the bar.

Akari's line (acks: [read-states.md](../protocol/read-states.md#acks-in-the-official-client)):

- It goes before the first loaded message from someone else that is newer than the channel's
  read position when the list opened. Akari marks the channel the user reads read at once, so
  the list keeps that position for as long as it's open. A channel without a read position
  counts as read up to what the list first shows. The user's own messages never start the line.
- Until Akari has read the channel, a new session's READY moves the position forward to
  Discord's, never back. That covers a list opened from the disk cache at launch after the user
  read on another device while Akari was closed. Akari sets no channel as being read until the
  session is online, so READY's positions aren't Akari's own acks; once Akari has read the
  channel, a later READY moves nothing.
- While there is no line, the next message from someone else gets one, also while the user
  reads at the bottom; later messages go below it. Acks, scrolling and reconnects keep it.
  Sending a message, Escape, and "Mark As Read" of the channel or its server remove it, and
  leaving the channel ends it. Its coming and going doesn't move the text the user reads.
- If the first unread message isn't loaded, the line sits before the first one that is, and an
  older page that brings earlier unread messages moves it up.
- Before a day's first message both show: the day divider, then the line (unobserved).
- A 1 pt `newMessages` line from 16 at the left to the tag, which ends 16 from the right:
  "NEW" in bold `newMessagesText` on a 13 tall `newMessages` tag with corners of 3, centered on
  the line. The row is 16 tall with the line in its middle and takes the 16 a group leaves
  above its first message, so under a group start it moves nothing; a follow-up under it keeps
  its group, and the row adds its 16.

Akari's bar:

- It follows the line: it shows while the list has a line that isn't on screen, above the
  visible messages, below them, or before messages that aren't loaded, and that hasn't been on
  screen before. Once the line has been on screen in a window that shows, the bar goes and
  stays gone for that line; a line for a later read position can bring it back. No line, no
  bar. A line on screen when the channel opens never brings it.
- "{n} new messages since {time}", "1 new message since {time}" for one. n counts the loaded
  messages from others from the line on. It reads "{n}+ new messages" when the read position is
  older than every loaded message and the channel's first message isn't loaded, or when the
  list doesn't reach the present. The time is the first unread message's, as message times are
  shown, or the read position's when that message isn't loaded (a channel never read has none,
  so the first loaded unread message's). **Unconfirmed:** the wording is the user's translation
  of the German UI.
- Clicking it jumps to the first unread message: the line goes to the top of the list, as far
  as the list scrolls. When that message isn't loaded, Akari first loads the messages around
  the read position (not compared with 80 or more unread messages yet); a channel never read
  has none, and the jump goes to the first loaded unread message. The jump counts as seeing the
  line, so the bar goes.
- "Mark As Read" on its right marks the channel read like the channel's menu item, which ends
  the line and the bar; it doesn't jump.
- Shift+Page Up does what a click on the bar does, from the list and the composer, whenever
  there is a line, also after the bar has gone ([shortcuts.md](shortcuts.md)).
- It looks like the jump bar: 32 tall, 16 from the sides and 4 from the top, corner radius 8,
  `brand` with white 14 medium text, over the messages and above the "Catching up…" capsule.

### Reading: the signal to ack

What the official client showed (Q12d-f above, 2026-10-15): after a break it acks only on a
click or key press in the client; scrolling back down, focusing the browser or restoring the
window isn't enough. Akari tells the core which channel the user reads
([read-states.md](../protocol/read-states.md#how-akari-acks)) only while all of these hold:

- the session is online, and the list's refresh after READY is in (the list isn't stale);
- the app is active and the window is key and visible (not minimized or covered);
- the list is at the present and scrolled to its bottom;
- the user is engaged: opening the channel (also the one already open), reopening the last
  place at launch, a click or a key press in the window. Scrolling away from the bottom, the
  window losing key, the app going inactive or the window being hidden ends it, and
  scroll-wheel events don't engage. It is stricter than the official client in one place: a
  click while scrolled up reads nothing until the list is back at the bottom, but it
  counts: once the list is there, reading starts without another click.

A list the session opens on its own (the open channel deleted or hidden, its server gone)
isn't read until the user clicks.

## Later

- Emoji images, embeds and images.
- Components V2 layouts ([messages.md](../protocol/messages.md#components-v2)). Until Akari
  renders them, such a message shows one muted placeholder line where its content would be.
