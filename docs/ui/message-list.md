# Message list

The messages of the open channel. Numbers are for Discord's redesigned client (2025) at 100%
zoom, in the default ("cozy") message display and default density, measured from the web
client on 2026-10-10; colors in [theme.md](theme.md).

## Layout

- Messages are listed oldest at the top, newest at the bottom. While the user is at the
  bottom (within 2 of it), new messages keep the list at the bottom; when they scrolled up,
  the list stays where it is.
- At its top and bottom the list bounces with the native macOS elastic overscroll, unlike the
  official client, whose Chromium list stops there. While the user scrolls, momentum and
  bounce included, the list never moves on its own, and a position past the bottom counts as
  being at the bottom. A message that arrives during a gesture at the bottom keeps the list
  there: it scrolls to the new bottom once the gesture ends, unless the user scrolled up in
  the meantime.
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
- The author's name in medium `textStrong`: Discord shows the server nickname, else the global
  name, else the username. Bots carry a small tag after the name ("APP" since 2024).
  **Unverified** that webhooks get the same tag. *Akari: the global name, else the username;
  server nicknames come with the member milestone.*
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
7. It has a thread started from it (observed 2026-10-19: Ray's 17:22 message with a thread
   started a group 6 minutes after his 17:16 one). The message after it doesn't start one for
   that: it continues the group under the thread's box, without a header (observed 2026-10-20:
   Ray's "danach" seconds after his "tb14").
8. It or the previous message is a thread's starter ([Threads](#threads)).
9. Its timestamp is **7 minutes (420 seconds) or more** after the previous message's. At
   419 seconds it still continues the group.

Otherwise it continues the previous message's group.

Discord also starts a group when ephemeral, scheduled, blocked or silent state changes, when
the posting application changes, and for the first message of a forum post. Akari applies
those once its records carry the fields they need.

## Times

The time in a group's first line:

| When | Shown as (en-US) |
|---|---|
| Today | the time only, "2:05 PM" (observed 2026-10-14) |
| Yesterday | "Yesterday at 11:59 PM" (**unverified** since the today form changed) |
| Earlier, or a date in the future | the locale's short date and short time, "10/17/26, 4:02 PM"; German "17.10.26, 16:02" (observed 2026-10-19) |

All of them follow the user's locale and time zone, also when those change while the app
runs: at midnight "2:05 PM" becomes "Yesterday at 2:05 PM", and a new time zone or locale
redraws the times and moves day dividers without reopening the channel. Hovering the time
shows the full date and time. Akari uses the platform's short date and time styles for the
locale, which include the user's own date format and 24-hour settings.

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

- an icon in the avatar column, centered on the first line, whose shape, color and box size
  follow the kind (Discord's system-message renderer, read 2026-10-21; Chris's screenshots
  showed the pin and the poll): a green arrow right for joins, additions, follows,
  subscriptions, purchases and accepted join requests, a red arrow left for removals and
  refused or withdrawn requests (16); a green handset for a call, grey and mirrored for a missed
  one (16); a pencil for renames and icon changes (16); a filled pushpin leaning right for pins
  (18); a pink gem for boosts (16); a red X, a green check or an amber warning for Discovery
  (16); the thread symbol for threads (18); stage lines with a person and broadcast arcs, green
  for a start, muted otherwise, a microphone or a raised hand (24); a ticket for an app upgrade
  (16); left-aligned bars for a poll result (16); a trash can, a clock with a warning, a person
  with a minus, a hammer or a check for reports (20); a green speaker for a voice session (24);
  a green person with a check for an accepted friend request (18). Unless colored, icons are in
  `channelDefault`; a poll's in `interactiveText`, stage lines' in `textMuted`. *Akari: SF
  Symbols fitted into Discord's box sizes; threads keep Akari's thread symbol and boosts a
  plain hexagon (Discord's are its own drawings); AutoMod alerts, which Discord draws as a
  message from "AutoMod", keep the info symbol until that block comes;*
- a sentence naming who did what, in `chatTextMuted` (#818289), its actor in `textStrong`
  (#FBFBFB) and medium (observed 2026-10-20 on the thread and pin lines; Akari draws every
  kind's actor so);
- the time, one space after the sentence and on its baseline (observed 2026-10-19); hovering
  it, and only it, shows the full date and time, as Discord does.

**A new thread** posts "Ray started a thread: t2. See all threads." in the channel (observed
2026-10-17 and 2026-10-19): "Ray", "t2" and "threads" in `textStrong` (#FBFBFB) and medium, the
rest in `chatTextMuted` (#818289), all three links with the pointing hand and an underline while
the pointer is over them. The thread's name opens the thread beside the channel, in the split
view ([layout.md](layout.md#split-view)), "threads" the channel's Threads popover, and "Ray"
the profile (**unverified**). *Akari: the name opens the split view.
A thread Akari doesn't have, such as one archived since, is fetched first
([dispatches.md](../protocol/dispatches.md#threads)); if Discord refuses it or it's gone, the
name stays strong text without the link. "Ray" and "threads" are drawn the same but aren't
links yet: Akari has no profiles and no Threads popover.*

**A pinned message** posts "Ray pinned a message to this channel. See all pinned messages."
(observed 2026-10-20, in German "Ray hat eine Nachricht an diesen Kanal angepinnt. Alle
angepinnten Nachrichten anzeigen."): "Ray", "a message" and "pinned messages" strong and
medium, the rest gray. "a message" jumps to the pinned message, the message the line refers
to, with a brief highlight, and "pinned messages" opens the channel's pins popout (Discord's
client, read 2026-10-20). A line that doesn't refer to its message has "a message" in plain
gray text. *Akari: "a message" has the
pointing hand and the underline and jumps as the unread bar does: it loads the messages around
the pinned one unless they're loaded and nothing else is loading a place for the list, and the
message goes to the vertical middle of what the list shows, below a locked thread's banner and
the unread bar (36 each) while either can show and above the strip and the jump bar at its
bottom, as far as the list scrolls (the official client centers it, observed 2026-10-20); one
deleted since leaves the next message there. The user's last request places the list, as for the unread bar's jump: a click while
another jump or the jump to the present loads takes their place, and a second click on the same
message while it loads asks once. The message flashes ([below](#the-flash)). "pinned messages"
isn't a link yet: Akari has no pins popout.*

**Every other kind** is worded as Discord's client words it (its en-US strings and its
system-message renderer, read 2026-10-20), from what the message carries and what the store
knows. **Bold** below is strong text, as names are:

| Type | The line |
|---|---|
| 1 added | "Ray added **Mira** to the thread." in a thread, "… to the group." elsewhere; Mira is the message's first mention, "???" (thread) or "unknown-user" without one |
| 2 removed | in a thread "Ray removed **Mira** from the thread."; elsewhere "Ray removed **Mira** from the group.", or "Ray left the group." when the mention is the author or missing |
| 3 call | while it runs "Ray started a call. — **Join the call**"; then "Ray started a call that lasted {length}."; "You missed a call from Ray that lasted {length}." when the reader isn't a participant; without an end, no length |
| 4 renamed | "Ray changed the channel name: **name**" (also in threads); a forum post "… changed the post title: **name**"; a group "Ray changed the group name: **name**. **Edit Group**" or "Ray removed the custom group name. **Edit Group**" |
| 5 icon | "Ray changed the group icon. **Edit Group**" |
| 7 join | one of Discord's 13 welcome lines, by the message's time: its ID's milliseconds since 1970 (through a double, as JavaScript's `Number` takes the ID) mod 13: "Ray joined the party.", "Ray is here.", "Welcome, Ray. We hope you brought pizza.", "A wild Ray appeared.", … "Yay you made it, Ray!"; a bot's join "Mira has added **Akari Bot** to the server." (from the message's `actor`, undocumented) or "Akari Bot was added to the server." |
| 8–11 boost | "Ray just boosted the server!", or "… **3** times!" for a count above one; at a level, with the server known, "… Lab has achieved **Level 2!**" |
| 12 followed | "Ray has added **Lab #news** to this channel. Its most important updates will show up here." |
| 14–17 Discovery | "**Lab** has been removed from Server Discovery because it no longer passes all the requirements. Check **Server Settings** for more details." and its relisted and warning lines; "this server" without the name |
| 24 AutoMod | "**AutoMod** has blocked a message in **general**" (or "has flagged" unless the alert says blocked); "unknown-channel" when the store doesn't have it |
| 25 subscription | "Ray joined **Gold** and has been a subscriber of **Lab** for 3 months!", "Ray renewed …", and for no months "Ray joined **Gold** as a subscriber of **Lab**!" or "Ray renewed **Gold** in their **Lab** membership!" |
| 27–31 stage | "Ray started **topic**", "Ray ended **topic**", "Ray is now a speaker.", "Ray requested to speak.", "Ray changed the Stage topic: **topic**" |
| 32 app upgrade | "Ray upgraded Akari Bot to premium for this server! 🎉", or "… a deleted application …" |
| 36–39 security | "Ray enabled security actions until {date}.", "Ray disabled security actions.", "Ray reported a raid in Lab.", "Ray resolved an Activity Alert." |
| 44 purchase | "Ray has purchased **Badge**!" |
| 46 poll | "Ray's poll **Lunch?** has closed."; a title over 50 UTF-16 units keeps 49 code points without trailing spaces and dots, then "…" |
| 52–54 join request | "This application was approved! Welcome!", "… was rejected.", "… has been withdrawn." (Discord's words when the client doesn't have the request) |
| 58–62 report | "Ray banned **Mira**", "Ray kicked **Mira**", "Ray timed out **Mira**", "Ray deleted the message", "Ray resolved this flag" (no periods) |
| 65 voice | "Ray started **a voice hangout**."; once it ended "Ray was hanging out for an hour.", "Ray and **Mira** were …", "Ray, **Mira**, and **Noa** were …", "Ray, **Mira**, and **2 others** were …" |
| 67 friend | in a DM only: "You accepted **Ray**'s friend request." (plus " They said: **note**") or "**Ray** accepted your friend request." (plus " You said: **note**") |

Call lengths are moment's English `humanize()` as Discord's client uses it: under 44.5 s "a few
seconds", then "a minute", "2 minutes" from 90 s, "an hour" from 44.5 minutes, "2 hours" from
90 minutes, "a day" from 21.5 hours, "2 days" from 36 hours, "a month" from 25.5 days, and so on
(JavaScript's rounding, halves up).

*Akari: a call runs while its message has no end; Discord asks its call state. Every name a line
shows resolves as a message's header does ([Groups](#groups)). A voice session names the
participants the store knows, as Discord's client does with its own store; Akari's knows DM
recipients. Akari can't join calls, so "Join the call" is strong text.*

**Links:** a part links only where Akari has its target: a thread's name, "a message" and a
poll's title (they jump), an AutoMod alert's channel while the user can see it, and a Discovery
notice's server. Discord's other links are strong text without the hand until Akari has their
targets: profiles (every actor and named person), "threads", "pinned messages", "Join the call",
"Edit Group", a followed channel's server popout, "Server Settings", a subscription's server
page, a product, and "a voice hangout".

**Not drawn:** Discord's client shows no line for invite reminders (22), revive and gaming
prompts (40, 42), HD streaming (55), wallpapers and emoji (56, 57, 63) and a purchase that isn't
of a server product; it makes 48, 50, 51 and 66 itself; 35 and 64 are cards; a channel linked to
a lobby (49) shows only when the client knows the app. *Akari leaves all of them out of the
list, and the messages around them group as if they weren't there. Not drawn yet either: the
poll result's card, the AutoMod alert's message block, the "Wave to say hi!" button, a bot's APP
tag, a subscription's welcome badge and sticker buttons, a purchase's thank-you card.*

Discord's client draws 26, 41, 47 and 68 as messages people write, and so does Akari (without
41's gift card and 68's time chip).

They never join a group and always end one.

**Which messages count as system messages:** every type except Discord's message set (the
default type, replies, slash-command and context-menu command results, 26, 41, 47, 68), thread
starter messages, and types Akari doesn't know, which show their content like an ordinary
message; of those, 56, 57 and 63 show nothing. A thread's starter that can't be loaded shows as
one ([Threads](#threads)).

### The flash

When a jump lands on a message (a pin's "a message", a poll's title; in Discord also a reply's
quote and search results), Discord's client highlights it: `messageHighlight`, #5865F2 at 41/255
in both themes, over the message's box (not the space above its group), its trailing corners
rounded 4. It fades in over 0.2 s (CSS `ease`), holds until 2 s after the jump and fades out over
0.2 s; with reduced motion it comes and goes at once. A new jump restarts it, and only one
message flashes at a time. Read from its flash
component (a 2 s timer) and its CSS (a 0.2 s `background-color` transition), 2026-10-20. *Akari:
the same, with macOS's Reduce Motion. A jump to a message deleted since flashes nothing, as in
Discord; the unread bar's jump doesn't flash.*

## Selecting text

Discord lets the user select a message's text, a system message's line and the channel start's
text; names, times, day dividers and the reply line can't be selected. *Akari: the same. The
selectable texts are all text views like a message's, so they select alike, and a click, double
click or drag never changes their font, color or baseline (a selectable label's field editor
would).*

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

## Threads

A thread opens like a channel, with its own start, starter and composer. Observed in the
official web client on 2026-10-17 (German and English UI) and from the user's screenshots on
2026-10-19.

- **The start** of every thread: the round icon with the thread symbol, the thread's name as the
  title ("t1"), and "Started by Ray" with the owner's name in `textStrong` and semibold, the rest
  in the subtitle's `textDefault`. Then the day divider and the messages. *Akari: the channel
  start's sizes; the owner's name from the store or from a loaded message of theirs. While no
  name is known, the line is left out (**unverified** what the official client shows then).*
- **The starter.** A thread started from a message begins with that message, drawn as it is in
  the channel: its author, its time and its content, not as a reply. It is a group of its own,
  and the next message starts a new one with a full header, whatever the time between them.
- **A starter that can't be loaded** (deleted, or not readable): after the day divider, a
  system line with the thread symbol, "Sorry, we couldn't load the first message in this
  thread", and its time; then the other messages.
- *Akari:* when Discord sends the starter without the message it points at, Akari fetches that
  message once ([messages.md](../protocol/messages.md#a-threads-starter-message)); until it's
  there the starter has no row, and the new messages line waits on the next one, or stays last.
  A starter whose fetch failed, or a cached one whose page failed, shows the "couldn't load" line
  until a later page of the thread fetches it again.

### The thread box

A message a thread started from has a box under it, and so does the "started a thread" line of
a thread started without a message (observed 2026-10-17, Q21, Q38; the rules and measures from
Discord's web client, read 2026-10-21, and the user's screenshots of 2026-10-19). A thread
started from an older message has its box under that message only; its line has none. The box
shows once Akari has the thread: from the message itself (its page, MESSAGE_CREATE or the
MESSAGE_UPDATE that starts it), THREAD_LIST_SYNC or THREAD_CREATE, joined or not, archived or
not. *Akari keeps the thread of a cached message's box in its cache, so a relaunch shows the
box at once.*

- **First line:** the thread's name, 14 semibold `textStrong`, then 8 and the count in the link
  color: "1 Message ›", "16 Messages ›", "See Thread ›" at none, "50+ Messages ›" for a thread
  from before 2022-07-01 with 50 or more (Discord stopped counting there), "100k+ Messages ›".
  The count starts from Discord's `message_count` and moves with every message and delete in
  the thread, live.
- **Second line**, in this order:
  - an archived (closed) thread: "There are no recent messages in this thread.", no time;
  - a thread with no messages: "There are no messages in this thread yet.", no time;
  - no newest message known: "There are no recent messages in this thread." and the time;
  - else the newest message: a 16 avatar, 8, the author's name (medium, `textStrong` at 64%),
    4, the text as inline markdown on one line, cut with an ellipsis, links not clickable,
    hidden spoilers covered; an empty one says "Click to see attachment", "Click to see
    sticker", "Click to see command" or "Click to see message" (components), a pin line
    "Message pinned.", in italics; then 8 and the time.
  - The time is the thread's last message's, else its creation's: "1m ago" under two minutes,
    "{n}m ago", "{n}h ago", "{n}d ago" below 30 days, ">30d ago" below 360 days, then the long
    date ("January 20, 2026"), in `textMuted`. *Akari moves it on with the list's timer;
    Discord redraws it only with the box.*
- **Measures:** 1 border (#323237 dark), fill (#242429 dark), radius 4, 8 padding inside the
  border, the two 18-tall lines 2 apart: 56 tall. As wide as its content, at most 480. Its
  left edge on the text column (72). Its top border 15.5 below the message's last baseline,
  9.5 below a line's (measured at 2x on the official client, 2026-10-21); its bottom is the
  message's.
- **The connector:** from 5 under the avatar's (or the line's icon's) center, 2 wide in the
  thread spine's color, down and into the box's middle with an 8 outer corner radius, ending 3
  before the box.
- **Clicks:** the whole box is one button with the pointing hand; only the count underlines
  while hovered, the background doesn't change; the author's avatar and name do nothing.
  Clicking opens the thread beside the channel, in the split view
  ([layout.md](layout.md#split-view)); another box replaces the thread there, and the box of
  the thread already there keeps it.
- *Not built yet:* the content's trailing icons, poll questions as content, voice messages'
  placeholder, blocked and ignored authors, server nicknames and role colors for the author.

## Jump to present

- While the list is detached, a bar sits at the bottom of the list, attached above the
  composer: on the left a note that older messages are shown, on the right "Jump to present"
  with a down arrow. It doesn't show after a short scroll up, only when the newest messages
  aren't loaded, as in the official client.
- Clicking it, or pressing Escape ([shortcuts.md](shortcuts.md)), loads the newest messages
  and scrolls to the bottom. At the present, Escape only scrolls to the bottom. Either way,
  Escape also marks the channel read. Both cancel a jump to the first unread message that is
  still loading ([below](#the-new-messages-line-and-the-unread-bar)).
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
  as the list scrolls, and in a locked thread below the banner. When that message isn't loaded, Akari first loads the messages around
  the read position (not compared with 80 or more unread messages yet); a channel never read
  has none, and the jump goes to the first loaded unread message. The jump counts as seeing the
  line once those messages are in the list, so the bar goes then; if they can't be loaded, the
  bar stays for another click.
- The user's last request places the list. Escape, Shift+Escape, "Jump to present", "Mark As
  Read" (the bar's, or the channel's, its server's or its folder's menu item), sending a
  message and switching to another channel cancel a jump that is still loading, and the list
  goes where that request takes it; back in the channel after a switch, it opens at the present
  with the bar. The cancelled jump doesn't count as seeing the line. A second click while the
  jump loads starts no second load; one after such a request starts a new jump.
  A jump clicked while the list is still loading the present takes that load's place; if the
  jump then fails or a mark as read cancels it, the list still goes to the present.
- "Mark As Read" on its right marks the channel read like the channel's menu item, which ends
  the line and the bar; it doesn't jump.
- Shift+Page Up does what a click on the bar does, from the list and the composer, whenever
  there is a line, also after the bar has gone ([shortcuts.md](shortcuts.md)).
- It looks like the jump bar: 32 tall, 16 from the sides and 4 from the top, corner radius 8,
  `brand` with white 14 medium text, over the messages and above the "Catching up…" capsule.
- When it appears while Akari is the active app, VoiceOver announces its text once; a new
  count doesn't repeat it, and a bar that appears while another app is in front isn't
  announced (it is still on screen for VoiceOver to read).

### Reading: the signal to ack

What the official client showed (Q12d-f above, 2026-10-15): after a break it acks only on a
click or key press in the client; scrolling back down, focusing the browser or restoring the
window isn't enough. In the split view it reads both lists at once, whichever was clicked last
(observed 2026-10-17). Akari tells the core which channels the user reads
([read-states.md](../protocol/read-states.md#how-akari-acks)): the open channel, and the split
thread beside it, each only while all of these hold:

- the session is online, and the list's refresh after READY is in (the list isn't stale).
  The core stops reading whenever the connection drops, a resume included, and Akari sets the
  channel again once both hold; the user stays engaged across the drop;
- the app is active and the window is key and visible (not minimized or covered);
- the list is at the present and scrolled to its bottom;
- the user is engaged: opening the channel (also the one already open), reopening the last
  place at launch, a click or a key press in the window; scroll-wheel events don't engage.
  Scrolling clearly away from the bottom ends it for that list (the other one in the split
  view keeps reading): the user's own scrolls toward older
  messages, counted from the bottom, add up to more than 48 or a quarter of the list's visible
  height, whichever is larger. That is looser than the 2 that keep the list at the bottom
  ([Layout](#layout)): a shorter scroll or a trackpad's bounce only pauses reading until the
  list is back at the bottom, and the list's own scrolls (the unread bar's jump, the jump to
  the present, following new messages) never end it; the jump's own move doesn't count toward
  it. The window losing key, the app going inactive, the window being hidden, the Mac going to
  sleep and fast user switching to another account end it too.
  It is stricter than the official client in one place: a click while scrolled up reads
  nothing until the list is back at the bottom, but it counts: once the list is there, reading
  starts without another click.

A list the session opens on its own (the open channel or an open thread's parent deleted or
hidden, its server gone), or a split thread it shows on its own after READY (a remembered
thread Discord answered for after a relaunch, a batch bringing it), isn't read until the user
clicks or presses a key. After the Mac sleeps, reading
resumes with the first click or key press after it wakes. Clicks and key presses count only in the session's window; when
it closes, Akari stops watching for them, and watches again once it is back, key and visible.
Akari tells the core only when the channels change or the connection dropped, and what a
list reports while it scrolls reaches the core on the next turn of the main loop, once, with
the last value.

## Later

- Emoji images, embeds and images.
- Components V2 layouts ([messages.md](../protocol/messages.md#components-v2)). Until Akari
  renders them, such a message shows one muted placeholder line where its content would be.
