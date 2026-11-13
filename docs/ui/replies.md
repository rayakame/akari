# Replies

How a reply starts, the bar over the composer while one is pending, the line over a reply in
the list, and the jump to the message it answers. Read from the official client's code and
stylesheets (canary 671549, 2026-11-12); nothing here was observed in a running client. Sizes
are at 100 % zoom and default density, colors by their token in [theme.md](theme.md). What a
reply sends is in [messages.md](../protocol/messages.md); who may reply is in
[message-actions.md](message-actions.md#what-may-be-done-with-a-message).

## Starting a reply

- **Reply** in the hover bar and in the message's menu
  ([message-actions.md](message-actions.md#the-hover-bar)), where `reply` allows it. For a
  message the user can edit the button is in the Shift row only.
- The next message sent in that channel answers the message. The composer takes the focus.
- **The ping.** A reply mentions the answered message's author unless the message is the user's
  own or Shift was held at the click: **Shift+click on Reply** starts it without the ping. The
  bar's switch changes it afterwards, where it shows.
- **One pending reply per channel.** A second Reply in the same channel replaces the first; a
  thread or a post has its own. It stays while the user looks at other channels.
- It ends when the send starts (not when Discord answers: a failed send doesn't bring the bar
  back), with the bar's ✕, with Escape, and when the answered message is deleted; what was
  typed stays.

*Akari:*

- *The pending reply lives as long as the session. The official client also keeps it across a
  restart.*
- *A bulk delete that takes the answered message ends the reply too; the official client's
  store has no handler for it.*
- *A reply Discord refuses because its message is gone (a delete that raced the send) fails like
  any message, and its row says why: "Not sent: the message it replies to was deleted." The
  official client shows its general "could not be delivered" notice. If the core refuses the
  reply before it is queued (the message left the loaded ones), the bar goes, the text stays and
  the strip under the composer says so.*
- *The official client's R and Shift+R on a message with keyboard focus are left out: Akari's
  rows take no keyboard focus yet.*

## The reply bar

A band inside the composer's card, as its top part. The card keeps its radius of 8 on all four
corners; the band takes the top two and nothing is squared off.

- **Height:** 3 kept clear at the top, then a row of 34 with the switch or 32 without it: 37 or
  35, and a line of 1 (`borderSubtle`) between the band and the text.
- **Background:** `surfaceHigher`.
- **Label:** "Replying to Name", 16 from the left, 14 on an 18 line in `textDefault`, the name
  semibold; one line, cut with an ellipsis before the buttons.
- **On the right,** ending 4 from the edge:
  - the switch, where it shows: an "@" of 16, 4, then "ON" or "OFF" at 14 semibold, with 12
    beside and 8 above and below; `textLink` when on, `textMuted` when off. Its tooltip is
    "Click to disable pinging the original author." when on and "Click to enable pinging the
    original author." when off;
  - a separator of 1 × 20 in `borderSubtle`;
  - ✕ in a circle, 16, with 12 beside and 8 above and below, `interactiveText`, brighter
    (`interactiveTextActive`) under the pointer; "Cancel reply".
  Neither has a hover background.
- **The switch shows** unless the channel is a DM or a group DM or the message is the user's
  own. A DM's reply pings (unless started with Shift) and can't be changed in the bar; a reply
  to oneself never pings. Webhooks and bots are no exception.
- **A click on the band** goes to the answered message; the pointer is the hand over all of it.
  The switch and ✕ take their own clicks.
- **The answered message's row** is marked while the bar is up: `replyingBackground` (the brand
  color at 16 %) over its box, 10 % while it is hovered, and a bar of 2 in `brand` on its left
  edge. The tint takes the place of the mention highlight and of the hover tint.

*Akari:*

- *The name has no role color: Akari doesn't know members' roles yet.*
- *The band's click jumps like a click on a reply's line, with the flash. The official client
  navigates to the message's link there.*
- *The bar comes and goes at once; how the official client moves it in wasn't read.*
- *The list gives the band its room from its bottom edge: what a reader scrolled up sees doesn't
  move, and a list at the present stays at its newest message.*
- *The official client's nudge under the bar ("Want to make it easier to follow this chain of N
  replies?" with Create Thread) is left out.*
- *SF Symbols `at` and `xmark.circle.fill` stand in for the official glyphs.*

## The line over a reply

A reply (type 19) shows, over its header, one line about the message it answers. A reply
always starts a group, so the line is always over a header with its avatar.

### Layout

From the row's left edge and the line's top:

- The line is 18 high and starts at 72, the text column; 4 lie between it and the header.
- A **small avatar**, 16, round, never animated ([images.md](images.md#identity-images)); 4 to
  its right.
- The **name**, 14 medium in `replyName` (`textStrong` at 64 %), with "@" before it when the
  reply mentions the answered message's author (the ping, or naming them in the text); it never
  shrinks; 4 to its right.
- The **text**, 14 in `textDefault`, on one line, cut with an ellipsis at the text column's
  right edge. Custom emoji are 18 (1.25 × the text, rounded to whole points). An edited message
  ends with "(edited)".
- A **symbol** of 16 may follow the text, 4 after it, or lead it, 4 before; it keeps its size
  when the text is cut.
- The **spine**: a stroke of 2 in `spine` from the line's bottom edge, 4 above the header
  avatar, up to the line's middle, round the corner (outer radius 6) and along the line's middle
  to 4 before the small avatar. Its vertical part is centered on the header avatar's center (36
  from the row's edge), its horizontal part on the line's middle (9).

### The three states

| The answered message | Left | Name | Text | Click |
|---|---|---|---|---|
| loaded | its author's avatar | yes | its text on one line, or the words below | jumps |
| not sent along and not loaded | a badge | no | "Message could not be loaded", italic | jumps (loads around it) |
| deleted | a badge | no | "Original message was deleted", italic | none |

The badge is a 16 circle in `baseLowest` with a reply arrow in `textDefault`. The line follows
the message live: an edit shows the new text with "(edited)", a delete turns it into the deleted
state, and a message that any load brings turns "could not be loaded" into its preview. None of
this changes a row's height.

### What the text says

For a loaded message, the first that applies:

| The message | Text | Symbol after it |
|---|---|---|
| a "pinned a message" line | "Message pinned." | none |
| has text | the text | a sticker's, a command's, a voice message's, else a picture's if it has attachments or embeds |
| no text, a sticker | "Click to see sticker", italic | sticker |
| no text, a command's result | "Click to see command", italic | command |
| no text, a voice message | "Click to see voice message", italic | microphone |
| no text, otherwise | "Click to see attachment", italic | picture |

A forwarded message has the forward arrow before its text.

The text is the message's markdown by the official client's one-line rules
(`akari_markdown::parse_inline_reply`): styles, inline code, spoilers, links, mentions,
timestamps and emoji stay; a quote's marker is text; a code block shows as inline code; line
breaks are spaces; headings, subtext and list items give their text without markers; emoji are
never jumbo.

### Under the pointer

Over the text or the spine the pointer is the hand, the text turns `textStrong` (mentions, links
and code keep their colors) and the spine `interactiveText`, within 50 ms; the two light
together. A click on either jumps. A deleted message's line does neither. A right click on the
line opens the message's menu.

*Akari:*

- *SF Symbols stand in for the official glyphs: `arrowshape.turn.up.left.fill` in the badge,
  `photo`, `face.smiling`, `terminal`, `mic.fill` after the text and
  `arrowshape.turn.up.right` before it, in boxes of 16 (the official trailing glyphs are 20).*
- *The spine ends at the line's bottom, 4 above the header avatar. The official sheet's box
  ends 2 lower; Akari's header starts at the avatar's top.*
- *The name has no role color, and the name and the small avatar aren't links yet: no user
  popout, no user menu, no Shift+click to mention. The name isn't underlined on hover.*
- *A context-menu command's result (type 23) shows its line as a reply does, with the spine and
  the avatar; the official client draws a chat-bubble badge and no spine there.*
- *Spoilers in the line stay covered whatever the user's "show spoiler content" setting says,
  and a click on the line jumps rather than reveal them.*
- *A blocked or ignored author's message shows as any other ("Blocked message", "Ignored
  message" and the refusal to jump are left out: Akari doesn't know relationships yet), and so
  does a likely spammer's.*
- *A join line, a boost or another system message that someone answered reads "Click to see
  message", with its author's avatar and name; the official client shows its system wording
  with a glyph of its own and no name. A poll without text reads "Click to see attachment"; the
  official client shows the poll's question.*
- *Compact mode has no line of its own yet.*
- *The line isn't a VoiceOver element of its own beyond its text button, labelled "Replying to
  Name: text"; "Jump To Reply" is its help.*

## The jump

A click on the line's text or its spine, or on the reply bar, goes to the answered message:

- **Among the loaded messages:** the list scrolls it to the middle and it flashes
  ([message-list.md](message-list.md#the-flash)).
- **Not loaded:** Akari loads the messages around it (`around`), shows them and does the same.
  If the message turns out to be gone, the list lands beside its place and nothing flashes.
- **The way back.** When the jump left the present, the bar over the list's bottom reads "You
  Are Viewing a Reply." with the button "Jump To Original Message" instead of "You're reading
  older messages" and "Jump to present". The button jumps back to the reply and flashes it. Any
  other jump, the jump to the present and Escape end it; a jump that stayed among the loaded
  messages at the present shows no bar.
- **A thread's first row** shows the message the thread started from with that message's line;
  its click goes to the answered message in the parent channel: in the open list that shows the
  parent, else the parent opens and jumps.

*Akari:*

- *A message that an `around` load finds gone keeps "Message could not be loaded" on its replies:
  only a delete Discord reports turns the line into "Original message was deleted".*
- *The official client's questions before a jump to a blocked, ignored or likely spamming
  author's message are left out.*

## Escape

Escape's order ([shortcuts.md](shortcuts.md#notes)): an open New Thread form closes first; then
a pending reply is cancelled, the one of the pane the key was pressed in before the other
pane's; only a further Escape marks read and jumps to the present. Cancelling a reply marks
nothing, scrolls nothing and keeps what was typed. A channel the user marked unread
([message-actions.md](message-actions.md#mark-unread)) stays unread through that first Escape,
its new messages line where it is; the pending reply and the mark don't touch each other, in
either order.

## Highlight

A reply that pings the user is highlighted like any message that mentions them
([message-list.md](message-list.md)); a reply sent without the ping isn't.
