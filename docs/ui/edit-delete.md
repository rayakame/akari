# Editing and deleting messages

How the user edits a message in its row, what "(edited)" says, and how a message is deleted.
Read from the official client's code and stylesheets (canary 671549, 2026-11-12); nothing here was
observed in a running client. Sizes are at 100 % zoom and default density, colors by their token
in [theme.md](theme.md). Who may edit or delete is in
[message-actions.md](message-actions.md#what-may-be-done-with-a-message); the requests and what
the core does with Discord's answers are in
[messages.md](../protocol/messages.md#editing-and-deleting).

## Starting an edit

- **Edit** in the hover bar (a pencil, after Reply) and **Edit Message** in the message's menu,
  first in its writing group, where `edit` allows it. For a message the user can edit, the bar's
  Reply moves into the Shift row.
- **Up arrow in an empty composer** edits the newest loaded message the user may edit
  (`Store::newest_editable_message`), wherever the list is scrolled, skipping others' messages,
  system lines, stickers alone, pending and failed sends. With nothing to edit the key does
  nothing and never beeps.
- **One edit per channel.** A second edit in the same channel replaces the first, and its text
  goes without a question. A thread or a post has its own. The edit stays, with what was typed,
  while the user looks at other channels, and comes back with the channel.
- Edit, Edit Message or Up arrow on the message already being edited keeps what was typed: the
  row scrolls into view and the editor takes the focus, its caret where it was.
- *Akari: the official client's E on a message with keyboard focus is left out with the other
  keys of a focused message ([message-actions.md](message-actions.md#the-hover-bar)).*

## The editor

The row keeps everything but its text: the avatar and the header, the line over a reply, the
attachments, embeds, stickers, reactions and the thread's box. The text gives way to the editor:

- **The box** is the composer's: the text at 16 on a 22 line, 16 in from the sides and 17 from
  the top and bottom (56 for one line), `composer` with a 1 border, `borderMuted` and
  `borderSubtle` while it has the focus, radius 8. It starts 8 under the header, as wide as the
  text column, and grows with the text, line by line; the row grows with it.
- **The line under it:** "escape to cancel • enter to save" at 12 in `textDefault`, 7 above and
  below; "cancel" and "save" are links in `textLink`, underlined under the pointer, that do what
  the keys do.
- **The counter**, once a tenth of the limit or fewer characters are left (200 of 2,000, 400 of
  4,000 with Nitro): the number left at 14 semibold in `textDefault`, `danger` past the limit,
  14 from the box's right edge on its last line, the text making room for it. Its tooltip is
  "N characters remaining", "Message is at max length" or "Message is too long", the official
  words.
- The row stays lit with the hover tint while it is edited, and opens no message menu (the text
  has its own). The hover bar still shows over it.
- The editor takes the focus when it opens, with the caret at the end, also on a row the list
  first scrolls to, and keeps it through the row's updates and the list's scrolling; the composer
  takes it back when the edit ends. The list scrolls the row into view with 64 around it, and
  keeps the caret's line in view as the text grows; a row above the reader grows without moving
  what they read.

*Akari:*

- *The editor holds the message's text as Discord keeps it: a mention is `<@id>`, a channel
  `<#id>`, a custom emoji `<:name:id>`. The official client turns them back into names while
  editing; Akari has no autocomplete to turn them into IDs again yet, and these round-trip
  unchanged.*
- *No emoji button, autocomplete or text commands (`/shrug`, `/me`): the composer has none
  either.*
- *The counter follows the official editor's rule, a tenth of the limit; the composer's still
  starts at 200.*
- *The scroll that keeps the caret in view keeps 8 between its line and the list's edge.*

## Keys

- **Return** saves; **Shift+Return** and **Option+Return** start a new line; Return while an
  input method composes commits the composition.
- **Escape** cancels the edit: the message stays as it was. **Shift+Escape** marks the server
  read, as from the composer.
- The arrows move the caret; Up in an empty editor does nothing.
- Escape from the composer or the list ends an open edit first, before a pending reply and before
  marking anything read. In the split view each pane is done whole before the other: the pane's
  own edit, its own reply, then the other pane's edit and reply, so Escape never throws away
  what was typed in the pane the user isn't in ([shortcuts.md](shortcuts.md)).

## Saving

The text is trimmed of spaces and line breaks at both ends, then:

- **Unchanged:** nothing is sent and the editor closes.
- **Emptied:** for a message without attachments or stickers the editor closes and the delete
  question opens, without the Shift tip; Cancel leaves the message as it was. A message with
  attachments or stickers is saved without text.
- **Past the limit:** nothing is sent, the editor stays open with the text, and a line under the
  box says by how much: "This message is N characters too long. The limit is 2,000." It goes
  once the text fits again. *Akari's words; the official client opens an alert, or for a user
  without Nitro its offer of Nitro.*
- **Otherwise** the editor closes at once and the message is saved.

*Akari (D162):*

- *The row shows the new text at once, dimmed (`textMuted`) as a message being sent, until
  Discord takes it. The official client shows the old text until the gateway's update arrives.*
- *When Discord refuses the edit, the row shows Discord's text again and the editor opens with
  what was saved and one line saying why, in `danger` at 12 under the box: AutoMod's own reason
  (or the official words "This message can't be edited because it contains content blocked by
  this community. The message may also be viewed by the community owners."), the announcement
  limit in the official words ("To keep announcements speedy, published messages can only be
  edited 3 times per hour. Please try again in N minutes."), or Akari's words for a missing
  permission, a rate limit, a lost connection or a server problem. The official client loses
  the text on every refusal but AutoMod's, and shows nothing.*
- *A refusal because the message was deleted opens nothing: there is nothing left to save to,
  and the row goes with the gateway's delete.*
- *A refusal waits while another edit is open in the channel and opens once that one ends; one
  that comes while the channel isn't shown opens when it is. That holds too for a refusal
  before asking (no connection), of which the core keeps no record.*
- *The official client's AutoMod notice under the message ("(reverted)", "Learn more") is left
  out: the editor's line says it.*
- *A published announcement's "Are you sure?" before an edit is left out.*

## What ends an edit

Escape, "cancel", a save, another edit in the channel, and the message's deletion, also in a bulk
delete and in a channel that isn't open; a thread's starter row's edit ends when the message it
shows goes. A delete nothing told of (the channel's messages were let go meanwhile, or it came
while offline) ends the edit once the channel's loaded messages would show it and don't. The
composer takes the focus back after Escape, "cancel" and a save; after a deletion only when the
editor had it. *Akari: the official client misses bulk deletes.*

## "(edited)"

An edited message says so, in one of three places:

- **after its text**, one space after it, on the text's baseline;
- **on a line of its own** under a code block that ends the text;
- **after its attachments, embeds and stickers**, before its reactions, for a message without
  text.

"(edited)" is 10 regular in `chatTextMuted`. Under the pointer its tooltip is the edit's full
date and time, as a timestamp's. The line over a reply ends with it too, without the tooltip. While
the message is edited, the mark after its text goes with the text.

*Akari: macOS's own tooltip, after its usual delay; the official one opens after 750 ms.*

## Deleting

- **Delete** in the hover bar while Shift is held, in More's place, red
  ([message-actions.md](message-actions.md#the-buttons-in-order)); **Delete Message** in the
  menu, red, last in its group; and an emptied edit.
- **Shift at the click** deletes at once, from the bar and the menu. A click on the bar's Delete
  with Shift let go asks without the tip.
- **A failed send** goes at once from its bar, its menu and its "Not sent." line, without a
  question and without a request to Discord.

### The question

A modal over the dimmed window:

- "Delete Message", then "Are you sure you want to delete this message?";
- the message as its row shows it (the header, the line over a reply, the text, attachments and
  embeds), in a box of `panel` with radius 3, a 1 ring (`previewRing`) and a soft shadow, 10
  above and below the row, which keeps its own insets; nothing in it takes the pointer, and a
  tall message scrolls inside the box;
- from the menu only, the tip "You can hold down shift when clicking **delete message** to
  bypass this confirmation entirely.";
- **Cancel** and **Delete**.

Delete and Return delete; Cancel, Escape and a press outside don't. The question stays when
Akari stops being the active app, as the official modal does; it closes when its message is
deleted meanwhile (for a thread's starter row, the message it shows).

*Akari: the modal is Akari's (the reactions window's surface, ring and radius, 440 wide, 24 in,
the official modal's buttons: Cancel gray, Delete red, 40 high); its words and the message's box
are the official client's. The official client's report checkbox for another's message is left
out, and so is the announcement's wording for a published message.*

### After the answer

- *Akari (D162): the row dims to half until Discord answers, then goes from every place that
  shows the message. The official client changes nothing until Discord answers.*
- *A refusal brings the row back and says so in an alert, "Couldn't Delete Message", in Akari's
  words. The official client shows nothing.*
- *The official client's Backspace on a message with keyboard focus is left out.*
