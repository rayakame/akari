# Pinned messages

The header's pins button with its dot, the popout of a channel's pins, and pinning and unpinning
a message. Read from the official client's code and stylesheets (canary 671549, 2026-11-12);
nothing here was observed in a running client. Sizes are at 100 % zoom and default density,
colors by their token in [theme.md](theme.md). Who may pin is in
[message-actions.md](message-actions.md#what-may-be-done-with-a-message); the requests, the list
the core keeps and the ack are in [messages.md](../protocol/messages.md#pins) and
[read-states.md](../protocol/read-states.md).

## The header's button

- **Where:** in the channel header of a text or announcement channel (after Threads), a thread or
  a forum post open full width, a DM and a group DM. Not for the system DM, a forum's page or a
  voice or stage channel's chat. A pushpin, named "Pinned Messages" (the tooltip and VoiceOver's
  name); no tooltip while its popout is open, and the button is selected then.
- **The dot:** while the channel has a pin newer than the one the user last saw
  (`Store::pins_unread`: never for a muted channel or a thread the user hasn't joined), an
  "unread" badge at the icon's bottom. Opening the popout takes it away, on this device at once
  and on the others through Discord.
- **Behind the age gate** the button is disabled and ⌘P does nothing.

*Akari: the pushpin is the SF Symbol `pin.fill` at 17 in a 32 box, as the Threads button; the dot
is 8 across in `danger` with a 2 ring of the header's color, over the pushpin's bottom right
corner (its middle 3 in from the 20 icon box's corner, so the dot reaches 1 past the corner and
its ring 3). The official badge's size wasn't read.*

## Opening and closing

- A click on the button toggles the popout, and so does **⌘P** (Ctrl+P on Windows) wherever the
  header has the button; the shortcut list calls it "Toggle pins popout".
- **"pinned messages"** in a pin's line ([message-list.md](message-list.md#system-messages))
  toggles the popout of the line's channel, which opens first where it isn't the open one (a
  thread in the split view opens full width): the same toggle as ⌘P, so a second click closes it.
- Escape, a click outside and ⌘P close it; so does opening another channel. The Threads popout
  and the pins popout take each other's place.
- Opening the popout asks for the channel's first page of pins where the list needs one, and
  acks the channel's pins while the dot shows; a pin that arrives while it is open is acked too.
  *Akari: such a pin also asks the core for the list again, which asks Discord when the gateway
  couldn't place the pin (no copy of its message was loaded); the official client shows it after
  the next opening.*

*Akari:*

- *View > Show Pinned Messages (Hide while open) carries ⌘P; the File menu's Print, which would
  hold ⌘P, is gone: Akari prints nothing.*
- *The popout is a key panel attached to the window over a clear shield, as the Threads popout:
  a click outside closes it and goes no further.*

## The popout

- **Frame:** 420 wide, hanging under the button with its right edge on the button's and its top
  9 below the button, past the header's line (as the Threads popout); as tall as what it shows,
  200 at least and 80 % of the window at most. `surfaceHigh`, radius 8, a 1 `shadowBorder` ring
  and the panel's shadow.
- **Header:** 16 around, the pushpin (in `interactiveText`, in a 24 box), 8, "Pinned Messages" at
  20 on a 24 line, semibold, `textStrong`, and a 1 `borderSubtle` line under it: 57 in all. No
  count and no close button.
- **List:** the pinned messages, the newest pin first, 8 below the header and 8 from the left;
  each in a card (`surfaceHigher`, a 1 `borderSubtle` border, radius 8), 4 short of the popout's
  right edge, 8 apart. The card shows the message as the list draws it, in the cozy layout:
  avatar, name, time, the line over a reply, the text, attachments, embeds and stickers, 16
  inside above and below, starting 4 left of the card's padding (the avatar 12 in) and keeping 24
  on its right. The time is the message's; no pin time and no "pinned by".
- **Jump and ✕** show at the card's top right while the pointer is over it, 8 from its top and
  right edges and 8 apart, each a small secondary button 32 high on a `surfaceHigh` backing, so
  the message doesn't show through. **Jump** goes to the message in the channel's list, loading
  around it when needed, and the message flashes; the popout closes unless **Shift** is held, and
  then the list jumps behind it. **✕** ("Unpin Message", only where the user may unpin) asks the
  unpin question unless Shift is held.
- **Paging:** 25 a page. Within 250 of the list's end the next page loads, with a spinner 32 above
  and below; while Discord has more, "Load more messages" (a full-width small secondary button,
  8 above and 16 around) asks for it too. *Akari: the button is centered under the cards, 24 from
  the popout's left as in the stylesheet and 20 from its right where it gives 16.*
- **First page:** a spinner, 100 above and 120 below.
- **Empty:** "This channel doesn't have any\npinned messages... yet." (a DM or a group DM: "This
  direct message doesn't have\nany pinned messages... yet."), 16 medium in `textDefault`,
  centered, the line break kept, 20 under the illustration's 120, 8 above and 32 below; then the
  footer at the popout's bottom: "Protip:" at 14 semibold in `statusPositive` and "Users with the
  'Pin Messages' permission can pin a message from its context menu." (a DM: "You can pin a
  message from its context menu.") at 14, centered, on `hoverBackground` (#97979f at 12 %), 16
  around and 10 more above and below. *Akari: the words in `textMuted`, as the questions' tips;
  their color wasn't read.*

*Akari:*

- *No illustration: Discord's is a bundled picture. A 56 light `pin` symbol in `textMuted` stands
  in its 120 box.*
- *A first page that failed says "Akari couldn't load the pinned messages." with "Try Again"; the
  official client shows such a channel as empty. After a later page failed, "Load more messages"
  stays and asks again (the official client offers nothing more and stops paging), and scrolling
  doesn't ask again.*
- *The cards take no pointer: links, spoilers, images and reactions open from the list, after
  Jump. The official client's rows are as interactive as the list's. The cards show no
  reactions.*
- *The ✕ is busy (a spinner) until Discord answers; the card goes when the gateway says the
  message isn't pinned.*
- *No arrow keys between cards, no Enter or Space to jump and no Page Up or Down in the list:
  Akari's rows have no keyboard focus yet. For VoiceOver, Jump and Unpin Message are the card's
  actions.*
- *Jump shows no loading state while the list loads around the message (the list's own loading
  shows), and asks nothing about blocked or ignored authors: Akari has no blocks yet.*
- *A new session leaves the list shown until its first page comes; the official client empties
  it.*

## Pinning and unpinning

- **Pin Message** and **Unpin Message**: in the Shift-held hover bar (after Mark Unread, the
  pushpin and the slashed pushpin), in the message's menu (after Copy Text), and the popout's ✕,
  where `MessageActions` allows them; never for a failed send.
- **Shift** at the click pins or unpins at once, from the bar, the menu and the ✕.
- **The pin question:** "Pin It. Pin It Good.", "Hey, just double checking that you want to pin
  this message to #channel for posterity and greatness?" (a DM or a group DM: "to the current
  channel"), the message in its box ([edit-delete.md](edit-delete.md#deleting)), Cancel and "Oh
  yeah. Pin it" in the brand's color.
- **The unpin question:** "Unpin Message", "You sure you want to remove this pinned message?",
  the message in its box, "Protip: You can hold down shift when clicking **unpin message** to
  bypass this confirmation entirely." ("Protip:" in `statusPositive` and semibold, the rest at 14
  in `textMuted`), Cancel and "Remove it please!" in red. From every place, the bar included.
- **Nothing changes before the gateway says so:** the menu's and the bar's words flip, the
  popout's list changes and the pin's system line arrives with Discord's updates.
- **A refused pin** shows the official alert:

| Why | Title | Text |
|---|---|---|
| 250 pins already | The Pin Broke | Discord is unable to pin that message. You may have hit the limit of 250 pins in channel #name. (a DM: "in this channel.") |
| No permission | No Access | You do not have permission to pin messages in this channel. |
| Not in this channel | Invalid Channel Type | This message cannot be pinned in this channel. |
| An archived thread | Invalid Thread Archive State | This message cannot be pinned in an archived thread. |
| A system message | Invalid Message Type | This message is a system message and cannot be pinned. |
| The message is gone | Unknown Message | The message you are trying to pin does not exist. |
| Another refusal | The Pin Broke | Discord's message, else "An error occurred, please try again." |
| No answer | Error | An error occurred, please try again. |

- **A refused unpin**, whatever the reason: "The Pin Is Stuck!", "Discord failed to unpin that
  message for some reason. Sorry about that.", Try Again (asks again) and Cancel. One that is gone
  counts as unpinned.

*Akari:*

- *The questions are Akari's modal, as the delete question: a key panel over the dimmed window,
  440 wide, Return answers yes and Escape, Cancel and a press outside no; it stays while another
  app is active and closes when its message is deleted meanwhile. Asked from the popout it stands
  over it, and the popout has the key again afterwards.*
- *The bar's button isn't busy while its request runs: a second click asks again, which Discord
  takes as the same pin (the requests are idempotent).*
- *The alerts' button is macOS's "OK" for the official "Okay"; Try Again isn't red.*
- *The official client's P and Shift+P on a message with keyboard focus are left out with the
  other keys of a focused message ([message-actions.md](message-actions.md#the-hover-bar)).*
- *The thread split's "…" has no "Pins" item yet; the thread opened full width has the button.*
