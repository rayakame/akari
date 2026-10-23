# Creating threads

How the official client creates a thread, and what Akari does. Read in Discord's web client
(canary 649224, 2026-10-23: the form's module and CSS, its stores, the key handler, the menus) and
observed by Chris on the second account (TF-O1–O5, 2026-10-23). The requests are in
[dispatches.md](../protocol/dispatches.md#threads). Akari's departures are in *italics*.

## Where it starts

- **A message's context menu:** "Create Thread", the last item of the group with Edit Message,
  Reply and Forward, right before the group that starts with "Copy Text". It shows only for a
  sent user message (a plain message, a reply, a command's answer, a few that render as one)
  without a thread, in a text or announcement channel where the user may create public threads
  (Create Public Threads and Read Message History); it's hidden, not disabled, otherwise. A
  timeout takes it away, as every permission but viewing and reading history. *Akari also needs
  Send Messages in Threads for every entry: the first message is required, so without it no
  thread could start with one (the official client would create the thread and fail its
  message).* *Akari has only
  Copy Text from that menu besides it, with the separator between them.*
- **The Threads browser** ([thread-browser.md](thread-browser.md)): the header's "Create" and,
  with no threads, the empty state's "Create Thread", where the user may create public or
  private threads. Both close the browser first.
- **⌘⇧⌥N** ("Create a thread"): the open channel's form, or the open thread's parent's; nothing in
  DMs or where the user may create no thread. It never toggles: it replaces what the slot shows.
  *Akari: File > Create Thread, with the same keys, disabled where it would do nothing.*
- *Later in Akari* (the message-features milestone): the hover bar's "Create Thread" (Shift shows
  it for a message that can be forwarded) and "View Thread", T and Shift+T on a focused message,
  the composer's "+" menu, `/thread`, the nudge under a chain of replies, and attachments and
  stickers in the first message.

Opening goes to the channel first, closes popouts, and moves the channel composer's text into
the form's first message when that is empty.

## The form

It takes the split view's place, the same card at the same width ([message-list.md](message-list.md#threads)):
one per channel, so opening it closes the channel's split and opening a split closes it. It stays
when the user goes to another channel and comes back, with what it said there (a dialog from a
create that finished meanwhile shows on the way back). *Akari keeps it for the session; the
official client keeps it across restarts.* Opened again for another message, it follows that
message, also while its thread is being created; a box clicked before it, whose thread is still
being fetched, doesn't replace it. *Akari:* the form follows its message's edits (the preview
and the name it would get); a deleted message leaves a form for a thread without one, both
texts kept (also when Discord answers that it doesn't know the message, with the name asked
for). Where the user may no longer create the thread (a permission or a timeout), the form
closes, keeping what's typed.

- **The header** (48 and its line): the thread symbol, dimmed to 60%, then "New Thread" (16
  medium, `text-strong`), and the X ("Close") on the right.
- **The fields** sit at the bottom of a scroller, 16 inside it:
  - a 64-point circle (`background-mod-strong`) with the thread symbol (a lock with it while
    private), 16 above and below;
  - **"Thread Name"** (16 medium, 8 above its field): a 40-point field, 12 inside, radius 8, a
    1-point border (`input-border-default`; the brand color while editing; the error color with a
    faint red fill on an error), placeholder "New Thread". Line breaks become spaces, and it
    takes at most 100 characters. From a message the label is "Thread Name (Optional)" and the
    placeholder the name the thread will get.
  - 24 below, **"Private Thread"**: a 20-point checkbox (radius 4; the brand color once checked)
    and "Only people you invite and moderators can see" 12 beside it; checked, "You can invite
    new people by @mentioning them." 4 below (12, `text-subtle`). With both permissions it's the
    user's; with only Create Private Threads it's checked and disabled; without it, and for a
    thread from a message, there is no checkbox.
  - A field's error shows 4 below it: a 16-point exclamation mark and the text (12, the error
    color).
- **From a message**, the message itself shows under the fields, as the list draws it, 24 above
  the composer.
- **The first message's composer**, 8 from the card's edges, with the placeholder "Enter a
  message to start the conversation!" and the character count near the limit. It grows to half
  the form at most, and a tall message above it scrolls (at most 40% of the form), so the name
  and its error stay in view. *Akari's composer as in channels; no attachments, emoji picker or
  stickers yet.*
- **Slowmode**: with a slowmode in the parent, a strip above the composer, on the right: a timer
  and the time left before the next thread, "Slowmode Immune" for a user it doesn't apply to,
  else "Slowmode is enabled." (the tooltip names the interval).

## Creating

Return in the name field or the composer creates it; there is no button.

- The first message is required: "Starter Message is required." above the composer, with a red
  border around it (Chris saw the period; the client's string has none). The name is required
  without a message: "Thread Name is required" under its field. Both show at once and clear as
  the user types; each try starts over.
- From a message, an empty name takes **the message's name for it**: the first embed's title (cut
  at 40 characters with "..."); else the message's first line as text, mentions, custom emoji,
  spoilers and timestamps spelled out and markdown as typed, without leading spaces, `#` and `-`,
  as many whole words as fit in 40 characters, a separator after the last one kept ("das ist ein
  ziemlich langer satz zum " for "das ist ein ziemlich langer satz zum testen von threads",
  TF-O1); a first word longer than 40 is cut there with "...". Lengths count as JavaScript does
  (an emoji is two). Nothing left: "Thread". *Akari leaves out the invisible marks Discord puts
  around mentions, has no poll question yet, and uses the derived name for a name of only spaces
  too.*
- A private thread needs the checkbox; Discord's other kinds follow the channel (an announcement
  channel's threads are announcement threads).
- **After it's created**, the thread opens beside the channel in the split view with its composer
  focused, and the first message is sent there as an ordinary message; if the user went to
  another channel meanwhile, the form just closes. What was typed is gone with it. *Akari: a
  first message the core refuses before sending stays in the thread's composer; a create that
  finishes after the user closed the form, or opened another thread beside the channel, opens
  nothing; one that finishes after the form was opened for another message leaves the form for
  that message without what went out.*
- **Lines and boxes:** a thread from a recent message gets the box under it and no "started a
  thread" line (TF-O1); one from a message older than a day gets the line too (TF-O2); a private
  one gets none (TF-O3). A public one without a message gets the line (Discord's docs; not
  observed separately).
- **Someone else's thread** from the form's message turns the form into that thread, beside the
  channel, and the typed message becomes its composer's text. *The official client skips threads
  the user created elsewhere, and moves the text only when its settings remember the message;
  Akari always does both.*

## Refusals

Discord's dialogs (title, text, one button; *Akari's says "OK"*); everything typed stays.

| What | Shown |
|---|---|
| The server's active threads are full | "Cannot create thread" / "This server has reached the maximum number of active threads. Free up space by archiving a thread." |
| Its announcement threads are full | "Cannot create thread" / "This server has reached the maximum number of active announcement threads. Free up space by archiving a thread." |
| The parent's slowmode for new threads | No dialog: the strip counts down. *Akari asks nothing while it runs; the official client asks and gets the slowmode answer.* |
| Another rate limit | "Cannot create thread" / "You are being rate limited." |
| AutoMod refuses the name | Its reason under the name field, until the next try. *Also for AutoMod's other refusal, where the official client shows nothing.* |
| Discord wants a name | "Thread Name is required" under the field |
| The answer was lost or didn't read as the thread (a timeout or broken connection after sending, a 5xx, an odd answer) | *Akari:* "Thread may exist" / "Discord didn't confirm the thread, but it may have been created: check the channel list before trying again."; the slowmode for new threads starts as if it was |
| Anything else (no permission, a thread already there, the network before sending) | "Error" / "An error occurred, please try again." |

A captcha shows the generic error for now (*the official client shows the captcha and asks
again*).

## Closing

- **Escape** closes it before anything else Escape does (marking read, jumping to the present,
  closing a split), wherever the focus is, and keeps what's typed for the next time. In its fields
  Shift+Escape marks the server read and Cmd+. does nothing, as in the composer.
- **The X** asks first when anything is typed: "Are you sure you want to go?" / "Any changes you
  made will be lost if you exit now." with "Exit" and "Back to editing"; exiting forgets it.
- The open channel's row in the channel list closes it too, as it closes a split, and keeps what's
  typed. Opening the form for another message keeps the message and the checkbox and starts the
  name over.
