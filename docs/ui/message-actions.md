# Message actions

What the official client lets a user do with a message, and what Akari does: the rules, the
hover bar, the message menu, Mark Unread. Everything here is from the code read of canary 671549
(2026-11-12: the hover bar's props and stylesheets, the message menu, the reply, edit, pin and
reaction predicates, the permission and message-type tables); nothing was observed live.
Akari's departures are marked *Akari:*.

## What may be done with a message

The core answers in one read, `Store::message_actions(channel, message)`, a `MessageActions` of
booleans; the hover bar, the message menu and every shortcut ask it and decide nothing
themselves. `Store::message_actions_of(&message)` answers the same for a message the caller
holds and the store may not: a pinned message of the pins list, a reply's target, a row a jump
trimmed away. The answer changes with the message, its channel (a thread archived or locked),
the user's roles and the end of a timeout, so it is read when the bar or the menu opens, not
kept.

### The terms

- **User message:** types 0, 19, 20, 23, 26, 41, 45, 47 and 68 (a plain message, a reply, a
  command's answer and a few that render as one). Everything else is a system message.
- **Replyable:** types 0, 7, 19, 20, 23, 24, 25, 32 to 42, 45, 46 and 69. Joins, AutoMod's
  actions and role-subscription purchases take replies; recipient changes, calls, renames, pin
  lines, boosts, "started a thread" lines and thread starters don't.
- **Undeletable:** types 1 to 5 (recipient changes, calls, a group's name and icon), 21 (a
  thread's starter), 35, 56, 57, 64 and 68.
- **Private channel:** a DM or a group DM. The client asks no permission there: replying,
  reacting and pinning are everyone's, and nobody may delete another's message or remove
  another's reactions, a group's owner neither. Akari's core counts a private channel as
  granting View Channel, Send Messages, Read Message History, Add Reactions, Use External Emojis
  and Pin Messages, and nothing else; this set is internal, and `Store::permissions` still
  answers nothing for a private channel. *From the code read: what the client's permission store
  answers for Manage Messages in a private channel wasn't traced; "own messages only" is what
  Akari takes.*
- **System DM:** the DM with a system user. No reply, edit, pin or reaction there.
- **Writable:** not a thread, or an active thread, or an archived thread that isn't locked and
  that the user's next message would reopen (they may send in it). An archived and locked thread
  is writable for nobody, moderators included, until it is reopened.
- **Silenced:** timed out, or quarantined by AutoMod. Both leave a member only View Channel and
  Read Message History (the quarantine also Change Nickname), so everything below that asks for
  a permission is gone with them. The owner and administrators are exempt.

### The rules

A message that is still **sending** offers nothing. A **failed** send offers Delete alone, which
only removes the row (no request); Retry beside it is the composer's. An **ephemeral** message
offers nothing: the client gives it no bar and no menu. For every other message:

| Action | Offered when |
|---|---|
| `reply` | not the system DM; Send Messages and Read Message History (a private channel grants both); a replyable type. In a thread Send Messages already says whether the user may write there: an archived thread they could reopen takes a reply, a locked one only from a moderator (Manage Threads) |
| `reply_pings` | `reply`, and the message isn't the user's own: the reply starts with the ping on. Shift at the click turns it off (the UI's part) |
| `mention_toggle` | `reply`, not the user's own message, not a private channel: the reply bar has the "@" switch. Webhooks and bots are no exception |
| `edit` | not the system DM; the user's own; a user message other than type 68; not a sticker alone (a sticker with text is edited); not a voice message; not a forward; writable; Send Messages, which in a locked thread only a moderator has. A message of attachments or embeds alone is edited |
| `delete` | the user's own, or Manage Messages; writable; not an undeletable type; an AutoMod action (24) only with Manage Messages. A locked thread that is still active doesn't stop it |
| `pin` / `unpin` | not the system DM; a user message; Pin Messages (bit 51) and Read Message History, both a private channel's; writable; not a voice or stage channel's chat; not a media channel's post. `pin` for a message that isn't pinned, `unpin` for one that is; never both. Manage Messages no longer pins |
| `react` | a new emoji on the message: not the system DM; Add Reactions and Read Message History; writable; not a media channel's post. System messages take reactions like any other |
| `toggle_reaction` | joining or leaving an emoji already on the message: not the system DM; Read Message History; writable; not a media channel's post; not silenced. Add Reactions isn't asked: "Members can still react using reactions already added to messages without this permission." |
| `remove_reactions` | others' reactions (one user's in the reactors list, one emoji's, all of them): Manage Messages; writable |
| `mark_unread` | writable |
| `copy_link` | always |
| `create_thread` | `Store::can_start_thread`'s answer ([thread-create.md](thread-create.md)) |
| `view_thread` | the thread that has the message's ID is known and the user may view it. A forum post's first message, which has its own post's ID, views nothing |

What isn't in the answer, because it isn't the message's, the channel's or the permissions' to
say:

- Whether an emoji is already on the message (which of `react` and `toggle_reaction` a click
  needs) and whether the message has reactions at all (View Reactions, the remove items): the
  message's reactions say so.
- The 20 distinct reactions a message takes: the server's limit (30010), unknown to the client.
- Which emoji the picker offers (Use External Emojis, availability, Nitro): the picker's.
- The user's own row in the reactors list always has its "×".
- The setting "Show emoji reactions" (`text_and_images.render_reactions`), which hides the rows
  and every way to react: a display setting, which the apps read beside the answer
  ([reactions.md](reactions.md#show-emoji-reactions)).

*Akari:*

- *Sending and ephemeral rows offer nothing at all. The official client's menu still opens on a
  sending row (Mark Unread, Copy Message Link, on a provisional ID), and its pin check and key
  handler don't look at the ephemeral flag.*
- *`edit` needs Send Messages. The official client offers Edit without it and opens an editor
  that is disabled.*
- *`react` and `toggle_reaction` need Read Message History. The official client's reaction
  checks don't look at it, and Discord's server refuses the reaction without it.*
- *The owner and administrators count as not silenced, as in the permission computation. The
  official client's reply and reaction checks read the member's timeout alone.*
- *`view_thread` doesn't wait for the message's "has a thread" flag: a thread the store knows
  under the message's ID is enough, so Create Thread and View Thread never both miss while the
  message's update is on its way.*
- *A channel the user can't view offers nothing.*
- *Not looked at, because Akari doesn't keep them: polls (the client doesn't edit one), the
  message of a user-installed app's command (the user who ran it may delete it), a referral's
  trial offer, moderator-report channels, the server's verification gate ("can chat in guild",
  which the client asks before a reaction) and lurking. Discord refuses what such a row
  offers.*
- *Forward, Publish, Bookmark, Report and the join line's Configure are not offered.*

## A thread's starter row

A thread started from a message begins with a row of type 21 that points back at it, and the
official client renders that row as the original message itself (the bar and the menu get the
original, which lives in the parent channel). So does the core: asked with the thread's ID
and the row's ID, `Store::message_actions` and `Store::reactions` both answer for the original
once it is loaded (`Store::starter_message` is `Loaded`): its type, flags and author, and the
parent channel's permissions. The thread's own state doesn't count: an archived or locked
thread withholds nothing from the message it started from. `view_thread` is false on that
row, since the thread it would open is the one the row is in. While the original is loading,
gone or failed, both answer for the row itself, a type-21 system message of the thread.

*Akari: `mark_unread` is the one answer that stays the thread's. The row is the thread's first
item, so a mark from it marks the thread unread from its start, and the thread says whether it
may be marked (not an archived, locked one); the original's channel isn't touched. What the
official client marks from this row wasn't read.*

*Akari: the row offers no Reply. A reply written in the thread can't name a message of the
parent channel (the composer only builds references into its own channel), so `reply`,
`reply_pings` and `mention_toggle` are false on the row whatever the original allows. What the
official client's Reply does on this row wasn't read.*

Requests for the shown message never take the row's IDs. `Store::shown_message(channel, row)`
gives the channel and ID of the message a row shows: the original's for such a starter row,
the row's own for every other; a reaction, a pin or a delete from the row goes there, and Copy
Message Link names them. Mark Unread alone takes the row's own:
`Account::mark_unread(thread, row)`. While the row's message isn't resolved (loading, gone or
failed), Copy Message Link names the row itself, in the thread: `Store::message_link` falls
back to the row's own IDs.

So the row's bar and menu have Copy Text (the original's text), Copy Message Link (the
original's, in the parent channel; the row's own until the original is loaded) and Mark Unread
(the thread's); never Create Thread, since the original has its thread, nor View Thread, nor
Reply. *Akari: the apps ask the core with the row's own IDs and take the shown message's from
the `starter_message` they already hold for the row.*

## The hover bar

A row of small buttons over the hovered message's top right corner. From the code read of canary
671549 (the bar's component and its stylesheets, 2026-11-12), not observed.

### Where and how it shows

- **Size:** 34 high: a 1 border, 2 of padding, buttons of 28 × 28 side by side with no gap, each
  with a 20 × 20 icon in its middle. Its width is 28 per button plus 6, and 9 more for the line
  after the quick reactions. Corner radius 8, a button's 6.
- **Place:** its right edge 14 inside the message's box, its top 25 above the box's top, so it
  lies over the row above (y −25 to +9). Over the first message of a group it sits lower, 16
  above the box's top (−16 to +18), unless that message is a reply or a system line. *Akari: the
  bar never leaves the list's visible part: a row at the list's top edge gets its bar 4 below that
  edge, over its own text, where the official client cuts the bar off.*
- **Look:** `surfaceHigh` with a 1 `borderMuted` border and a soft shadow (black at 14 %, 8 % in
  the light theme, 1 down, blurred 4). With the pointer over the bar the border is `borderSubtle`
  and the shadow larger (4 down, blurred 8, 16 % and 8 %), changing over 0.1 s.
- **Buttons:** the icon in `interactiveText`. Under the pointer the button has `hoverBackground`
  and the icon `interactiveTextActive`; pressed, `interactiveBackgroundActive` (16 %); "More"
  while its menu is open, `selectedBackground`. A dangerous button (Delete) is `textError` in every
  state; a disabled one at half opacity. The pointing hand shows over every button. *Akari: SF
  Symbols for every icon; the official client also grows the hovered icon by a tenth, which Akari
  leaves out.*
- **Tooltips:** each button's tooltip is its label and nothing else; none names a key. *Akari:
  macOS's own tooltip, after its usual delay and under the pointer, where the official client
  shows its own at once above the button.*
- It shows and hides at once, without a fade, with the row's hover: only in the key window,
  and not while another window lies over the list at the pointer. The pointer over the bar,
  the part over the row above included, keeps the bar's own row hovered. Clicks and right
  clicks on the bar never reach the row under it.
- A message that is still **sending** and an **ephemeral** one have no bar, and neither has a
  row that allows nothing. A **failed** send's bar is Retry and Delete (Delete not red, no
  question asked; in the bar as in the menu only where the core offers it, which it does for
  every failed send); the "Not sent." line under the message keeps its own Retry and Delete.
- The row stays lit, with its bar, while a menu of it is open (a right click, the More button),
  whatever the pointer does meanwhile; no other row hovers then. When the menu closes the list
  goes back to where the pointer is. With a menu open Shift doesn't change the bar.
- For VoiceOver the bar is a group named "Message Actions" and each button carries its label.

*Akari: one bar for the whole list, a view of the list's controller laid over the rows; the
controller's pointer authority lights its buttons, and the rows track nothing
([message-list.md](message-list.md#selecting-text)). What the bar lists is read from the core when
the hovered row or Shift changes and when the row's content changes, not on every move.*

*Akari: the bar exists only over a hovered row, so VoiceOver's navigation doesn't reach it; the
row's menu has the same actions.*

### The buttons, in order

Each button shows only where `MessageActions` allows it. Without Shift:

| # | Button | When | In Akari |
|---|---|---|---|
| 1 | up to 3 quick reactions, then a separator | `react` | yes: 💯, 😆 and 💖 ([reactions.md](reactions.md#quick-reactions)) |
| 2 | Add Reaction | `react` | yes: opens the quick reactions as a menu until Akari has its emoji picker |
| 3 | Reply | `reply`, and not `edit` (others' messages, and the user's own that can't be edited) | yes: starts a reply ([replies.md](replies.md)); Shift at the click starts it without the ping |
| 4 | Edit | `edit` | with editing |
| 5 | Forward | | left out |
| 6 | Create Thread | `create_thread` | yes |
| 7 | View Thread | `view_thread` | yes |
| 8 | Publish | | left out |
| 9 | More | always | yes: opens the message's menu |

With **Shift held**, the bar changes at once, the pointer still, and back when Shift is let go:

| # | Button | When | In Akari |
|---|---|---|---|
| 1 | Copy Message ID | developer mode | once Akari has that setting |
| 2 | Copy Link | `copy_link` | yes |
| 3 | Configure (a join line, for Manage Server) | | left out |
| 4 | Mark Unread | `mark_unread` | yes |
| 5 | Bookmark Message | | left out |
| 6 | Pin Message or Unpin Message | `pin`, `unpin`; no question asked | with pins |
| 7 | Reply | `reply` and `edit`: the user's own editable message; the reply starts without the ping | yes |
| 8 | Add Reaction, Reply, Edit, Create Thread, View Thread | as without Shift; no quick reactions | as above |
| 9 | Delete | `delete`; no question asked | with deleting |
| 10 | More | only where Delete isn't | yes |

The Shift key counts as it is at the click: a click on a button of the longer bar, and on a
menu item, reads it then.

*Akari:*

- *Forward (and with it the rule that moves Create Thread into the Shift row for a message that
  can be forwarded), Publish, Bookmark, Configure and the "official message" mark are left out.*
- *The order above is one table in AkariKit (`MessageActionsModel`): a later feature turns its
  action on there and its button and menu item appear in their places.*
- *The official client's letter keys on a message with keyboard focus (E, R, P, Backspace, +, F,
  T, Alt+Enter) are left out: Akari's rows take no keyboard focus yet. The bar's actions are
  not VoiceOver custom actions of the row either; the rows have none.*

## The message menu

A right click or Control-click anywhere in a message's box, on a system line too, and the bar's
More button open the same menu; More's opens under the button. Groups are divided by separators
and an empty group leaves none. From the code read of canary 671549, not observed. The official
client's order, with what Akari has in bold:

| Group | Items |
|---|---|
| Reactions | **Add Reaction** (its submenu: the quick reactions), **View Reactions** (a message with reactions) |
| Writing | Edit Message, **Reply**, Forward, **Create Thread**, ***View Thread*** |
| The message | **Copy Text** (a message with text), Pin Message or Unpin Message, **Resend message** (a failed send), **Mark Unread**, **Copy Message Link** |
| Removing | **Remove Reactions**, **Remove All Reactions** (red, for those who may remove reactions, on a message with some; [reactions.md](reactions.md#removing-reactions)), **Delete Message** (red; today a failed send's only) |
| Developer | Copy Message ID (developer mode) |

*Akari:*

- *"View Thread" is in the menu, in Create Thread's place: the official client has it in the bar
  only.*
- *A message that is still sending, and an ephemeral one, keep "Copy Text" alone: the official
  client's sending row has more, its ephemeral row no menu at all.*
- *What a thread's starter row has is in [A thread's starter row](#a-threads-starter-row).*
- *The quick reactions are Add Reaction's submenu, not a row over the menu
  ([reactions.md](reactions.md#quick-reactions)).*
- *Left out: Forward, Translate, Bookmark and reminders, Apps, Speak
  Message, polls' and interactions' items, Report Message, the items for a clicked image, link,
  emoji or sticker (the media viewer has Copy Image and Copy Media Link), and Copy and Search
  with Google for a text selection (a selection's menu ends with macOS's "Copy"). Copy Message ID
  waits for developer mode.*

**Copy Message Link** gives `https://discord.com/channels/{server}/{channel}/{message}`, with
`@me` for the server in a DM or a group. The bar's "Copy Link" gives the same.

## Mark Unread

"Mark Unread" in the menu and in the Shift bar, and **Option-click** (Alt+click in the official
client) on a message: the channel is unread from that message on. The request and the rule that
keeps the channel unread while it stays open are in
[read-states.md](../protocol/read-states.md#mark-unread).

- An Option-click counts anywhere in the message's box: on its text, a link or an emoji too,
  which then aren't followed or opened, and no text is selected. From the code read: the
  official client's row handler takes every Alt+click and stops the click's default.
- *Akari: not on the row's buttons (media, a thread's box, Retry and Delete, the hover bar, the
  text and the spine of a reply's line), which keep their click, and only with Option alone.
  Where the message can't be marked (an archived, locked thread, a message still sending) the
  click isn't taken: it is an ordinary click with Option held. A click that is taken takes its
  drag and its release too.*
- On a thread's starter row the thread is marked, from its start
  ([A thread's starter row](#a-threads-starter-row)).
- The open list's new messages line goes over the chosen message, the user's own too, and
  the channel isn't read again while it stays open: leaving it (another channel, the split
  closed), marking it read (Escape) or writing there ends that. With a reply pending the first
  Escape only cancels the reply ([replies.md](replies.md#escape)); the next one marks read.
  Scrolling, another window, the Mac asleep or a lost connection don't: the channel is still
  open. Coming back to a channel that was left reads it as always.
- *Akari: the badge and the new messages line change at the click; the official client waits
  for Discord's answer. If Discord refuses, they go back and one alert says so ("Couldn't Mark as
  Unread"). A mark that something later replaced before it counted (Escape right after, the
  channel read or marked on another device, a new session with another position) takes its line
  back without an alert: nothing failed. Back is where Discord has the line: after two marks in
  a row, at the first one's message only if Discord took that mark, else where the line was
  before both.*
- *Akari: the official client's Alt+Enter on a message with keyboard focus is left out with
  the other keys of a focused message.*
