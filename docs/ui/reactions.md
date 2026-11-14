# Reactions

The pills under a message, what a click on one does, the quick reactions in the hover bar and
the menu, the window that lists who reacted, and a forum post's reactions. Read from the
official client's code and stylesheets (canary 671549, 2026-11-12); nothing here was observed in
a running client, and the requests' shapes are code-read, not observed
([reactions.md](../protocol/reactions.md)). Sizes are at 100 % zoom and default density, colors
by their token in [theme.md](theme.md). Who may react is in
[message-actions.md](message-actions.md#what-may-be-done-with-a-message).

## The row

- **Where:** under the message's text, attachments, embeds and stickers, above the box of its
  thread. A system line (a join, a pin, a boost) takes reactions like any message and has the
  row under its line.
- **Only with reactions:** a message without any has no row and no add button under it; the
  hover bar and the menu are the ways to the first reaction.
- **Order:** Discord's, which is the order the emoji were first used in; a new emoji comes
  last, and one whose last reaction went and came back comes last again.
- **Layout:** pills side by side, 4 apart, wrapping onto further lines 4 below each other. The
  row starts 1 above its first pills and ends 2 under its last: 34 per line, less 1. It stands 4
  under an attachment, embed or sticker and 2 under the text.
- **A pill:** 30 high with a radius of 8 and a border of 1; inside, 6 from each side, the emoji
  in a box of 20 and, 6 after it, the count: 14 semibold, each digit in 12, so a pill is 52 wide
  with one digit and 12 more for each further one.
  - At rest: `hoverBackground` (12 %), no border, the count in `interactiveText`.
  - Under the pointer: `interactiveBackgroundActive` (16 %), a `borderMuted` border, the count
    in `interactiveTextActive`. Pressed: `selectedBackground` and a border of the same gray at
    44 % (52 % in the light theme).
  - **The user's own:** the brand's color at 24 % with a border in the brand's color, the count
    #dfe4ff (#273478 in the light theme). It doesn't change under the pointer or pressed.
  - Fill and border change over 0.1 s.
- **The emoji:** a custom emoji is its picture from Discord's CDN, fitted into the box, with
  `:name:` in its place if it fails; an animated one plays by the same rule as the message's
  other custom emoji ([emoji.md](emoji.md#custom-emoji)). A Unicode emoji is drawn in the
  system's emoji font so that a face fills the box.
- **The count's roll:** when a count changes while the window is key, the old number leaves
  upward and the new one comes from below for a higher count, the other way for a lower one,
  over 0.3 s with a slight overshoot. In a window that isn't key, and with Reduce Motion, the
  number is swapped.
- **The add button** stands at the row's end, always visible, wherever the user may react with
  a new emoji (`react`): 34 × 30, radius 8, the chat's own color with a smiling face in
  `interactiveText`; under the pointer the panel's color, a white border at 20 % and the face in
  `interactiveTextActive`. Its tooltip is "Add Reaction".
- For VoiceOver the row is a group, each pill a button that reads "👍, 3 reactions, press to
  react" or "…, press to remove your reaction" (a custom emoji by its name, underscores as
  spaces), and the add button "Add Reaction".

*Akari:*

- *Super reactions show as plain reactions: one pill per emoji with both counts added, the
  user's own when either reaction is theirs. The official client draws a second, glowing pill
  with its own count and effects.*
- *The pills are views of the row; the list's one pointer authority lights the pill under the
  pointer and sets the cursor, and the rows track nothing.*
- *A reaction that changes rebuilds only the row's pills: the message's text, media and embeds
  stay as they are, and a count that changed keeps its pill. The row's height follows the lines
  of pills without the reader's place moving, and a list at its bottom stays there.*
- *SF Symbols' smiling face stands for the official client's face with a plus.*
- *The first reaction appears without a transition, as in the official client; so does the
  row's last one going.*

## A click on a pill

- A pill that isn't the user's adds their reaction; one that is takes it back. The pill changes
  at once and Discord is asked afterwards; a refusal puts it back
  ([reactions.md](../protocol/reactions.md)).
- Where the user may not toggle reactions (`toggle_reaction`: timed out, an archived thread
  they can't reopen, the system DM) the pills only show: the cursor over them is the
  not-allowed sign and a click does nothing.
- Joining a reaction that is already there doesn't need the Add Reactions permission; a new
  emoji does.
- **What a refusal shows:**
  - too many different reactions on the message: an alert, "We appreciate the enthusiasm,
    but..." / "Your reaction was not added because there are too many reactions on this
    message.";
  - blocked by the message's author: the official client shakes its window for 0.2 s;
  - anything else (a missing permission, an unknown emoji, the network): nothing; the pill
    goes back.

*Akari:*

- *A pill that is the user's only as a super reaction takes the super reaction back with one
  click, as any other. Akari never adds a super reaction.*
- *The system's alert sound stands in for the shake, and is also played when Discord refuses
  for a missing permission, where the official client stays silent.*
- *The official client's states for a lurker and a member who hasn't accepted a server's rules
  (a popout, the verification screen) aren't built: the core says what such a user may do, and
  Discord refuses the rest.*

## A pill's tooltip

- It names up to three of the users who reacted and counts the rest: "👍 reacted by Ann",
  "… by Ann and Bo", "… by Ann, Bo, and Cy", "… by Ann and 1 other", "… by Ann, Bo, and 3
  others", "… by Ann, Bo, Cy, and 2 others". While the names aren't there: "👍 reacted by 4
  people" ("1 person").
- The names are asked once per message and emoji, three of them, when the pointer has rested
  on the pill for half a second, and kept until that message's reactions change or a new
  session starts.

*Akari:*

- *macOS's own tooltip, under the pointer after its usual delay, where the official client
  shows a card of its own with the emoji at 32 that can be clicked to open the reactions window
  ("View Reactions" in the menu opens it).*
- *A Unicode emoji is named by itself, a custom one as `:name:`. The official client names
  Unicode emoji by its own short names (`:thumbsup:`), which Akari doesn't have.*
- *Names are the users' display names; server nicknames come with the member milestone. A pill
  with super reactions fills its three names up from them.*
- *The names are dropped when the message's reactions change; the official client keeps its
  list current from the events instead.*

## Quick reactions

- **In the hover bar,** where `react` allows a new emoji and Shift isn't held: three emoji
  before the bar's buttons, each in a button of 28 with the emoji at 20, then a line of 1 × 24
  (`borderSubtle`) with 4 on each side, then Add Reaction and the rest
  ([message-actions.md](message-actions.md#the-buttons-in-order)). The bar is 28 wider per
  emoji and 9 for the line.
- A click adds the reaction; on an emoji the user already reacted with it takes it back. The
  tooltip is the emoji's name over "Click to react" or "Click to remove".
- **Which three:** the official client takes the user's most used reaction emoji and fills up
  with 💯, 😆 and 💖.
- **In the menu:** the official client has a row of up to four quick reactions over the menu,
  and under "Add Reaction" a submenu of up to twelve with "View More" for the picker.

*Akari:*

- *The three are always 💯, 😆 and 💖: Akari doesn't read the user's frequently used emoji yet.*
- *"Add Reaction" (the bar's button, the row's add button, a forum post's "React to Post")
  opens a menu of the three quick reactions under its button, with a checkmark on those that
  are the user's already. The emoji picker takes its place when Akari has one.*
- *In the message's menu the quick reactions are "Add Reaction"'s submenu; there is no row of
  emoji over the menu.*
- *The official client's `+` on a message with keyboard focus and `+:name:` in the composer
  are left out.*

## The reactions window

"View Reactions" in the message's menu, where the message has reactions.

- A modal titled "Reactions" over a dimmed window, 600 wide; the X, Escape or a press outside
  closes it, and it closes itself when the message has no reactions left.
- **Left:** the message's emoji in a column 24 from the edge, 98 wide with 16 before a divider
  of 1: one tab per emoji, the most reactions first, 32 high and 8 apart with a radius of 8,
  the emoji at 24 and its count (14 semibold). The chosen tab is filled
  (`interactiveBackgroundActive`), the others under the pointer (`hoverBackground`). The window
  opens on the first tab; when the chosen emoji leaves the message the first tab is chosen.
- **Right:** the users who reacted with the chosen emoji, rows of 52: the avatar at 32, the
  display name (16 medium) over the username (12, `textMuted`). A hundred are asked at a time,
  the next hundred when the list's end comes on screen; a spinner stands there meanwhile.
- **The ✕** at a row's right, shown while the pointer is over the row, removes that user's
  reaction: on every row for those who may remove reactions (`remove_reactions`), and always on
  the user's own. The row goes at once.

*Akari:*

- *One tab per emoji, plain and super reactions together: the users of the plain reactions,
  then those of the super reactions. The official client has a tab for each kind.*
- *When the chosen emoji's reactions change, its first hundred are asked again and the list
  stays as it is until they come; users loaded past them are kept. The official client changes
  its list by the events' user IDs.*
- *A page that didn't come says "Couldn't load who reacted." with "Retry".*
- *A removal Discord refuses puts the row back, with the alert sound for a missing permission.*
- *A click on a user and the user's menu aren't built.*

## Removing reactions

In the message's menu, for those who may remove others' reactions, where the message has some:

- **Remove Reactions** lists the message's emoji; a choice asks "Remove Reactions for Emoji" /
  "Are you sure you want to remove all of the reactions for this emoji from this message?".
- **Remove All Reactions** asks "Remove All Reactions" / "Are you sure you want to remove all
  reactions from this message?".
- Both answer with "Yes" and "No"; Shift at the click skips the question. The pills go when
  Discord says so.

*Akari: a refusal says so in one alert ("Couldn't Remove Reactions").*

## "Show emoji reactions"

The user's setting (`text_and_images.render_reactions`, on by default). Off, no message shows
its reactions, the hover bar has no quick reactions and no Add Reaction, the menu has none of
the reaction items, and forum cards and a post's bar show none either. Akari reads the setting
and follows its changes; it has no window to change it yet.

## Forum posts

- **A post's first message** doesn't show its reactions under it; they are at the left of the
  post's bar ([forum.md](forum.md#a-post)): as many pills as `floor((width − 280) / 58)`,
  then "+N" for the rest, which opens the reactions window, then the button that adds one,
  labelled "React to Post" while the post has no reactions. While it has none and the forum has
  a default reaction, that emoji stands first as a pill without a number; a click reacts with
  it.
- **A card** shows one reaction before its count (after it on a gallery card): the first
  message's reaction with the most users, or, while it has none, the forum's default reaction
  without a number where the user may react. A click reacts, or takes the user's reaction back.

*Akari:*

- *The card's reaction is a chip of Akari's own, 20 high (the emoji at 14 and the count, the
  user's own in the brand's colors); the official client's wasn't measured.*
- *A forum's custom default reaction is known by its ID alone, so a reaction with it is asked
  without the emoji's name; whether Discord takes that isn't known.*
- *"+N" is a pill of the count's width without an emoji.*
