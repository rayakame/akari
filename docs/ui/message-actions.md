# Message actions

What the official client lets a user do with a message, and what Akari does. Everything here is
from the code read of canary 671549 (2026-11-12: the hover bar's props, the message menu, the
reply, edit, pin and reaction predicates, the permission and message-type tables); nothing was
observed live. Akari's departures are marked *Akari:*.

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
| `react` | a new emoji on the message: not the system DM; Add Reactions; writable; not a media channel's post. System messages take reactions like any other |
| `toggle_reaction` | joining or leaving an emoji already on the message: not the system DM; writable; not a media channel's post; not silenced. No permission is asked: "Members can still react using reactions already added to messages without this permission." |
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
  and every way to react: a display setting, which Akari doesn't read yet.

*Akari:*

- *Sending and ephemeral rows offer nothing at all. The official client's menu still opens on a
  sending row (Mark Unread, Copy Message Link, on a provisional ID), and its pin check and key
  handler don't look at the ephemeral flag.*
- *`edit` needs Send Messages. The official client offers Edit without it and opens an editor
  that is disabled.*
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
