# Composer

The field at the bottom of the message area where the user writes. Numbers are for Discord's
redesigned client (2025) at 100% zoom and default density; colors in [theme.md](theme.md).
Facts about the official client marked **unverified** come from community descriptions, not
from Discord's documentation.

## Layout

- A card at the bottom of the message area, 8 from its sides and its bottom, in the `composer`
  color, with corner radius 8. Its bottom edge lines up with the user panel's in the sidebar,
  and a one-line card is as tall as the panel (56).
- One line is 56 tall: text at 16 on a 22 line, 17 above and below, 16 inset on the left and
  right.
- Directly above the card runs a thin strip (24) over the bottom of the message area, as in the
  official client: typing indicators on its left (later in Akari; send errors and notices
  there now) and the slowmode note or countdown with a stopwatch on its right. Nothing sits
  below the card.
- Discord puts buttons for attachments, gifts, GIFs, stickers and emoji into the card. Akari
  has none of them yet.

## Growing

The card grows with its text, line by line, up to half the message area, then the text
scrolls inside it. The official client grows to about half the window (**unverified**).
Sending shrinks it back to one line.

## Placeholder

Akari's own wording, in `textMuted`:

- a guild channel: "Write a message in #general";
- a DM or group DM: "Write a message to Mira".

## Keys

- **Return** sends.
- **Shift+Return**, **Option+Return** and **Ctrl+Return** start a new line.
- **Return while an input method is composing** (Japanese, Chinese and other input sources)
  commits the composition; it never sends.
- **Up arrow in an empty composer** edits the user's last message in the official client.
  Akari reserves the key for that and does nothing yet.
- **Escape** see [shortcuts.md](shortcuts.md).

Pasted text arrives as plain text. Smart quotes, smart dashes, text replacement and automatic
spelling correction are off, since they would change markdown and code; the official client
doesn't do them either. Misspelled words are still underlined. Undo works within the draft and
never reaches back past a send or into another channel's draft.

## Sending

- The text is trimmed of spaces and newlines at both ends. A blank message sends nothing.
- The card clears as soon as the message is queued, and the message shows in the list as
  pending, dimmed (`textMuted`). The official client shows pending messages in a muted gray
  (**unverified**). A message refused before it's queued (too long, or Akari isn't connected)
  leaves the text exactly as it was, with the reason under the card.
- After a reconnect, messages that were still pending or had failed come back as the draft of
  their channel, after any text already there, so nothing typed is lost.
- Sending while the list shows older messages jumps to the present first.

## Failed messages

- A message Discord refused or that couldn't reach Discord stays in the list in red
  (`danger`). Under it a line says "Not sent." with two buttons, "Retry" and "Delete", in
  `textLink`. The official client also turns failed messages red and lets the user resend or
  delete them.
- The reason shows on the left of the strip under the card, in `danger` at 12, on one line.
  Akari words its reasons itself: being too fast (with how long to wait), missing
  permission, a person who doesn't accept the user's messages, a message that is too long, no
  connection, a server problem, a captcha Akari can't show.

## Without permission

When the user can't send in a channel, the card stays where it is, dimmed and disabled, and
says "You can't send messages in this channel." The official client keeps a dimmed composer
with a similar note (**unverified**). The same applies while the user is timed out.

## Slowmode

A channel's slowmode lets each member send one message every so many seconds (up to six
hours). Its rules:

- It applies in guild text, announcement, voice, stage, forum and media channels and in
  threads; never in DMs or group DMs. A thread has its own slowmode, copied from its parent
  when the thread is created.
- Since 2026-02-23 only the **Bypass Slowmode** permission exempts a member. Manage Messages,
  Manage Channels and Manage Threads no longer do. The owner and administrators have every
  permission and are exempt.
- The cooldown starts when the user sends, not when Discord confirms the message
  (**unverified** for the official client; Akari does the same). A message the user sent from
  another device starts it too.

What Akari shows on the right of the strip above the card:

- with slowmode and no cooldown running: a stopwatch symbol and "Slowmode: one message every
  10 seconds";
- for an exempt user: "Slowmode is on, but it doesn't apply to you";
- during a cooldown: the stopwatch and the time left, as m:ss (h:mm:ss past an hour).

Return during a cooldown sends nothing and keeps the text. The official client also shakes
the card; Akari doesn't.

## Message length

- The limit is 2,000 characters, or 4,000 with Nitro. Nitro Basic stays at 2,000, and so
  does Nitro Classic as far as its perk list says.
- Discord counts Unicode code points: an emoji with a skin tone counts as 2, a family emoji
  as 5, a custom emoji as its full `<:name:id>` text. Akari's core counts the same way.
- A counter appears inside the card at its right edge once 200 or fewer characters are left. It
  counts down and turns negative and red (`danger`) past the limit, like the official
  client's.
- Return past the limit sends nothing; the strip says by how much the message is too long.
  The official client offers to send such a message as a text file instead; Akari can't
  upload files yet.
- If Discord itself refuses a message as too long, Akari shows the same error with
  Discord's limit, so a counting difference never shows as a generic error.
