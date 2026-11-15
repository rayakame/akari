# The emoji picker

The panel that reacts to a message with any emoji the user may use: what opens it, where it
stands, its parts, the keys and the pointer, and what is left out. What the official client does
is read from its code (canary 671549, 2026-11-12: the reactions read and the hover bar read);
nothing here was observed in a running client. The official picker was the guide for Akari's own
(plan decision D166): its look, sizes and symbols are Akari's, in the app's palette
([theme.md](theme.md)). Which emoji are offered, their names, recently used and search come from
the core ([emoji-picker.md](../protocol/emoji-picker.md)).

## What opens it

| Opener | Where the picker stands | A pick reacts from |
|---|---|---|
| Add Reaction in the hover bar | left of the bar, tops aligned | "Message Reaction Picker" |
| The add button at the end of a message's reactions | right of the button, tops aligned | "Message Inline Button" |
| "View More", the last item of Add Reaction's submenu in the message's menu | left of the row's bar; without a bar, where its right edge would be | "Message Reaction Picker" |
| "React to Post" in a forum post's bar, and its add button once the post has reactions | right of the button, tops aligned | "Forum Toolbar" |

- It opens only where the user may react with a new emoji (`react`), on a message that was sent,
  with "Show emoji reactions" on ([reactions.md](reactions.md)); elsewhere none of these
  openers is shown.
- One at a time: opening another closes the first.
- The panel stays 8 inside the window. Where its side has no room it stands on the other side;
  near the window's bottom it is pushed up, and its top no longer meets the opener's.

*Akari:*

- *The official client's `+` on a message with keyboard focus, `+:name:` in the composer and the
  picker in the composer are left out (keyboard focus on messages waits, D165).*
- *A click on Add Reaction itself in the message's menu opens its submenu, as every macOS menu
  item with one does; the official client's opens the picker. "View More" opens it.*

## While it is open

- The panel takes the keyboard. A press anywhere else in the window closes it and does nothing
  else; so do Escape, the window closing, minimizing, entering or leaving full screen, and the app
  going to the background.
- **Its message's row stays lit** with its hover bar, as for its menu: what opened the picker is
  lit, the bar's Add Reaction (also for "View More") or the row's add button, not both; no other
  row hovers, and the pointer over the list changes nothing. The lit add button goes with the
  message when its row scrolls away and back. When the picker closes the row under the pointer
  hovers again. A post's bar isn't a row: nothing is lit for "React to Post".
- It closes itself when its message is deleted or leaves the list, its channel or server goes,
  or "Show emoji reactions" is turned off.

*Akari: the panel stays where it opened when the list scrolls or the window is resized; the
official popout follows its opener.*

## The panel

432 × 440 with a radius of 8, `surfaceHigh` with a `borderSubtle` border and the system's
shadow. Labelled "Reaction Picker" for VoiceOver, as the official dialog.

- **The search field**, across the top in a band of 52 with a `borderMuted` line under it: 32
  high, 12 from the sides, radius 8, `inputBackground` with a `borderSubtle` border, the
  magnifier (`interactiveTextActive`), the official placeholder "Find the perfect emoji" (14,
  `textMuted`) and, once something is typed, an ✕ that empties it. It has the keyboard's focus
  from the start and keeps it: nothing else in the panel takes it.
- **The strip**, the left 48 under the field, on `baseLowest`: one button of 32 per section, 4
  apart and 8 from the top, radius 8. Frequently Used is a clock, each server its icon at 24 with
  a radius of 6 (its initials on `hoverBackground` without one, or when it fails), then a line
  (`borderSubtle`) and Unicode's groups as SF Symbols: `face.smiling`, `hand.wave`, `pawprint`,
  `fork.knife`, `car`, `basketball`, `lightbulb`, `heart`, `flag`. The section at the top of the
  grid is lit (`selectedBackground`, the symbol in `interactiveTextActive`); under the pointer
  `hoverBackground`. A click shows that section's header at the top of the grid and selects its
  first emoji; during a search it ends the search first. The tooltip is the section's name. The
  strip scrolls on its own and brings the lit button into view.
- **The grid**, right of the strip: nine columns of 40, 12 from each side. Each section starts
  with a band of 32 holding its name (12 semibold, `textMuted`, 16 from the left where the first
  emoji's box starts, 12 from the band's top). The section at the top keeps its band pinned over
  the grid, opaque, until the next section's band pushes it up. A search's results have no band.
- **An emoji** stands in a box of 32 in its cell: a custom one is its picture from Discord's CDN
  fitted into the box (`:name:` there if it fails), a Unicode one is drawn in the system's emoji
  font as in a reaction's pill, so that a face fills the box ([reactions.md](reactions.md#the-row)).
  Emoji the user can't use here are dimmed to 35 %.
- **The selection** is a rounded square of the cell (radius 6, `hoverBackground`) behind the
  emoji.
- **The footer**, under the grid, 48 high with a `borderMuted` line over it: the selected emoji
  at 28, its `:name:` (14 semibold, `textStrong`) and under it why it is locked or, for a custom
  one, its server's name (12, `textMuted`).
- **A search that finds nothing** says "No emoji match your search" (14, `textMuted`) in the
  middle of the grid; the footer is empty.

*Akari:*

- *The official picker's sizes and colors weren't measured; the ones above are Akari's.*
- *The field has no focus ring: it holds the focus as long as the picker is open.*
- *Section bands can't be collapsed, and a server's band has no icon (the strip has it).*
- *"No emoji match your search" is Akari's sentence; the official client shows an illustration
  of its own.*

## The sections

In this order: **Frequently Used** (the user's recently used reactions, at most 42, best first),
each of the user's **servers** with its emoji (the channel's own server first, then the rail's
order) and Unicode's nine groups, **Smileys & Emotion** to **Flags**. Which emoji a server's
section lists, which are dimmed and why, and how recently used is counted is the core's
([emoji-picker.md](../protocol/emoji-picker.md)). A section without emoji is left out.

- The sections stay as they were when the picker opened; a pick with Shift doesn't move its
  emoji up in Frequently Used. They are read again when a server's emoji, the user's servers,
  their roles or the user change, when a server the picker lists changes, and when the user's
  permissions in the channel (a thread's: its parent's) change; a DM's new message reads nothing.
  The selected emoji stays selected in its own section (an emoji can also be in Frequently Used),
  else the emoji at its place there, and the grid stays where it is. Nothing changes on screen
  when the lists come back as they were. During a search the search is asked again, and its
  answer keeps the selection the same way.
- Unicode emoji newer than the system's emoji font draws are left out: Emoji 15.0 (what macOS 14
  draws) and each later version whose emoji the font all draws as one picture.

## Searching

- Each keystroke searches at once; the newest answer shows and an older one never replaces it.
  Spaces and colons alone search nothing: the sections show.
- The results stand in one section without a band, the first selected. The strip stays and
  lights nothing; a click on it ends the search and empties the field.
- Search finds emoji by their names: the short names (`thumbsup`, `+1`), Unicode's names and the
  servers' emoji names ([emoji-picker.md](../protocol/emoji-picker.md#search)).

## Keys and the pointer

| Key or click | What it does |
|---|---|
| Typing | searches |
| Up, Down | the emoji above or below, the column kept, across sections; to the last emoji of a shorter row |
| Left, Right | the emoji before or after, across sections |
| Return | reacts with the selected emoji and closes; before the search's answer to what was typed, with the answer's first once it comes (typing more drops it) |
| Shift+Return | reacts with it and stays open |
| Escape | closes, also with a search typed (the field's ✕ empties it) |
| The pointer over an emoji | selects it; the grid doesn't scroll |
| A click on an emoji | reacts with it and closes, on the release over the emoji pressed (dragging off cancels); with Shift at the release it stays open |

- A key that moves the selection scrolls the grid just enough to show it below the pinned band;
  a strip button scrolls the section's band to the top.
- Over an emoji the pointer is the hand; over a dimmed one the not-allowed sign, and a click or
  Return sounds the system's refusal (the footer says why) and reacts with nothing.
- The pinned band takes no pointer: the wheel over it scrolls the grid, and a click on it picks
  nothing.
- A pick adds the user's reaction; an emoji that is the user's reaction already stays theirs
  (the official client's add does nothing then). The pill shows at once, as for any reaction
  ([reactions.md](reactions.md#a-click-on-a-pill)), and the emoji counts as used.
- An animated custom emoji plays while the panel has the keyboard, by `animate_emoji`,
  and the selected one always.
- For VoiceOver each emoji that shows is a button named by its short name without colons
  ("thumbsup", "heart eyes"); a dimmed one is unavailable, with its reason as help. Pressing one
  reacts and closes.

*Akari:*

- *Left and Right move the selection, not the field's caret, as Up and Down do.*
- *Escape closes the picker even with a search typed, as the Threads browser does.*
- *Return before the search's answer waits for it rather than reacting with what showed before:
  what the user typed is what they get.*

## Left out

- *Skin tones: there is no tone selector and no default tone; every emoji is offered in its
  default form.*
- *Favourites, and Discord's synced frecency: Frequently Used is Akari's own count, per account
  on this Mac.*
- *Super reactions (the burst picker).*
- *Nitro's offer on a locked emoji: its click does nothing.*
- *Search by CLDR keywords ("happy" finding 😀).*
- *The picker's other tabs (GIFs, stickers) and its use in the composer.*
