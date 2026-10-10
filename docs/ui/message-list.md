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
- Text is 16 on a 22 line height.
- Hovering a message tints its background with `hoverBackground`.

## Groups

Consecutive messages from one author form a group. The first message of a group shows the
author; the rest show only their content.

**The group's first message:**

- 16 of extra space above it, and at least 44 tall. **Unverified** that 16 is the default
  group spacing.
- The avatar (40) in the left column, 2 above the name's line.
- The display name in semibold `textStrong`. Bots carry a small tag after the name ("APP"
  since 2024). **Unverified** that webhooks get the same tag.
- The time, 4 after the name, in 12 medium `chatTextMuted`.

**The other messages:** no avatar and no name. Hovering one shows its time (hours and minutes)
in the left column, right-aligned in a 56 wide space.

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
| Today | "Today at 2:05 PM" |
| Yesterday | "Yesterday at 11:59 PM" |
| Earlier, or a date in the future | numeric date and short time, "10/03/2026, 2:05 PM" |

All of them follow the user's locale and time zone, also when those change while the app
runs: at midnight "Today at" becomes "Yesterday at", and a new time zone or locale redraws the
times and moves day dividers without reopening the channel. Hovering the time shows the full
date and time. Discord writes the older form without the comma; Akari uses the platform's date format
for the locale.

## Day dividers

- One before the first message of each local calendar day in the list.
- Days only move forward: a message stamped earlier than the one before it (a pending message
  from a device whose clock runs ahead or behind) gets no divider of its own and stays under
  the current day. That keeps one divider per day, so a divider's key never repeats.
- A divider stays while any message of its day is shown, also when the day's first message
  goes.
- A 1 pt `borderSubtle` line across the list with the date in the middle, in the locale's
  long form ("October 10, 2026"), 12 semibold `textMuted`, on the list's background.

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
- At the beginning of a channel a single row says "This is the beginning of #general." (a DM:
  "This is the beginning of your conversation with Mira."), in Akari's words.
- Placeholder rows keep their height in every state, so their changes never move the
  messages.

## Jump to present

- While the list is detached, a bar sits at the bottom of the list, attached above the
  composer: on the left a note that older messages are shown, on the right "Jump to present"
  with a down arrow. It doesn't show after a short scroll up, only when the newest messages
  aren't loaded, as in the official client.
- Clicking it, or pressing Escape ([shortcuts.md](shortcuts.md)), loads the newest messages
  and scrolls to the bottom. At the present, Escape only scrolls to the bottom.
- Sending a message also jumps to the present.
- Akari's bar: 32 tall, 16 from the sides, corner radius 8, `brand` with white 14 medium text:
  "You're reading older messages" and "Jump to present".

## Catching up

After a new session (a reconnect that couldn't resume), Akari checks the open channel for
messages it missed. While it does, a small capsule at the top of the list says "Catching up…"
with a spinner, in `textMuted` on `panel`. If the check can't join the missed messages to the
list (more than 100 missed, or the request failed), the list becomes detached and the jump
bar says "Some messages may be missing" instead.

## Edits and deletes

- An edited message ends with "(edited)" right after its text, smaller (12) and in
  `chatTextMuted`, like the timestamps. A message without text puts it after its last line.
  The official client's mark has a tooltip with the edit time (**unverified**); Akari adds it
  later.
- An edit that makes a message taller or shorter doesn't move the text the user is reading.
- A deleted message leaves the list without moving the text the user is reading, whether it
  was above, below or at the top of what's visible. At the bottom, the list stays at the
  bottom.

## Later

- A red (`#d22d39`) "new messages" line with a "NEW" tag at its right end, at the first unread
  message.
- Messages that mention the user, tinted with `#f8a300` at 8% and a 2 wide bar on the left.
- Markdown, mentions, emoji, embeds and images. Until markdown rendering exists, Akari shows
  message content as plain text: a mention looks like `<@123>` and markdown symbols show as
  typed.
- Components V2 layouts ([messages.md](../protocol/messages.md#components-v2)). Until Akari
  renders them, such a message shows one muted placeholder line where its content would be.
