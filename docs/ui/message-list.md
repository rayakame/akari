# Message list

The messages of the open channel. Numbers are for Discord's redesigned client (2025) at 100%
zoom, in the default ("cozy") message display and default density, measured from the web
client on 2026-10-10; colors in [theme.md](theme.md).

## Layout

- Messages are listed oldest at the top, newest at the bottom. While the user is at the
  bottom, new messages keep the list at the bottom; when they scrolled up, the list stays
  where it is.
- Scrolling stops at the top and the bottom of the list: no elastic bounce past either end.
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

All of them follow the user's locale and time zone. Hovering the time shows the full date and
time. Discord writes the older form without the comma; Akari uses the platform's date format
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

## Later

- A red (`#d22d39`) "new messages" line with a "NEW" tag at its right end, at the first unread
  message.
- Messages that mention the user, tinted with `#f8a300` at 8% and a 2 wide bar on the left.
- An "(edited)" mark after edited messages.
- Markdown, mentions, emoji, embeds and images. Until markdown rendering exists, Akari shows
  message content as plain text: a mention looks like `<@123>` and markdown symbols show as
  typed.
- Components V2 layouts ([messages.md](../protocol/messages.md#components-v2)). Until Akari
  renders them, such a message shows one muted placeholder line where its content would be.
