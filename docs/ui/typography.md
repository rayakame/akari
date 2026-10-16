# Typography

Akari's text styles and how their sizes were matched to the official client. The official
client draws its text in gg sans, which Akari doesn't ship; Akari uses the system font (SF Pro
on Apple platforms). The two have different proportions, so a style's size in Akari is the size
at which SF Pro looks as big as the official text, not the official CSS size.

## The type scale

Every text style in an app is one entry of its type scale, kept in one place (on macOS
`TypeScale` in `apps/macos/Akari/Theme/TypeScale.swift`). Each style is given in points at a
**16 pt base** and computed from it, so a font size setting like the official one (which scales
the whole client) starts from the base. Glyphs inside icons (avatar and server initials, SF
Symbols) size with their icon, not with the scale.

The base is fixed for now, and a new base alone wouldn't reach everything: several places make
their fonts once and keep them (on macOS the message formatting's typefaces, the composer's font
and line, every cell's labels, set when the cell is made, and the sizing cells that measure
rows). The setting will also have to rebuild those fonts, drop the message texts built with the
old ones, and remeasure every row while keeping the reader's place.

Message text is the base: 16 regular on a 22 line.

## Measured styles

Measured on 2026-10-14 from two screenshots of the same channel at the same window size, one of
the official macOS client (dark theme, default font size, German UI) and one of Akari, at 2 px
per point.

**Method.** For each style, the same string the official client showed was drawn in SF Pro at
every size from 8 to 34 pt in 0.5 pt steps, with the official text and background colors, and
its ink (the drawn pixels) measured like the official one. The chosen size is the one where
neither the ink's width nor its height is further off than needed (the smallest of the two
relative errors' maximum). The weight is the SF Pro weight whose strokes looked like the
official ones next to each other at that size; it is the official weight's name in every case
(gg sans 500 is SF Pro medium, and so on).

Width alone would shrink short strings of digits too far: gg sans draws its figures much
narrower than SF Pro, so at the width-matched size times would be a point shorter than the
official ones.

| Style | Official text measured | Akari (pt at base 16) | Weight | Was |
|---|---|---|---|---|
| Message text, composer and its placeholder | "test", "Nachricht an #new" | 16 (line 22) | regular | 16 |
| Server name above the channel list | "testserver" | 15 | semibold | 16 semibold |
| Category | "Textkanäle" | 13 | medium | 14 medium |
| Channel name (also DM names) | "slowmode", "chrissi-todo", "test_permissions", "new" | 15 | medium; selected in light: semibold | 16 regular, selected medium |
| Channel header | "new" | 14 | semibold | 16 semibold |
| Author name | "Ray" | 15.5 | medium | 16 semibold |
| Message time | "10:13", "13:14", "13:39" | 11 | medium | 12 medium |
| Day divider | "14. Oktober 2026" | 11 (on the 15 line of the old 12) | semibold | 12 semibold |
| Start header title | "Willkommen bei #new!" | 30 | heavy | – |
| Start header subtitle | "Das ist der Beginn des Kanals #new." | 15.5 | regular | – |
| Title bar | "testserver" | 12.5 | medium | the native title |

- **Message text stays at the base.** By the same measure SF Pro at 16 is 3 to 8% wider than
  the official message text (the best match is 15 to 15.5), but next to each other they read
  as the same size, and message text is the base everything else follows. The composer and its
  placeholder are the size of message text in the official client too (the same x-height), so
  they stay at the base with it.
- **Channel names** are medium in every state; the official client changes only their color
  when one is selected, hovered or unread. Light theme keeps semibold for the selected one
  (**unverified** in the light theme).
- **The divider's line height** stays at the 15 pt of the old 12 pt font, so the smaller text
  doesn't move the list.
- Across the strings of one style the best size varied by up to half a point either way
  (channel names: 14.5 to 15.5); the table gives the middle.

## Not measured yet

These keep the values Akari had; each needs an official screenshot that shows it:

| Style | Akari (pt at base 16) | Why not measured |
|---|---|---|
| User panel: name, username | 14 semibold, 12 regular | the measured account shows its name in a display name style and a nameplate |
| DM start header: username line | 18 medium | no DM in the screenshots |
| Channel topic in the header | 14 regular | the test channel has no topic |
| Time beside a hovered follow-up message | 11 regular | nothing hovered |
| APP tag after a bot's name | 10 semibold | no bot message |
| NEW tag on the new messages line | 10 bold, the scale's smallest caps | seen, not measured yet |
| Notes in the list (attachments, replies, "Not sent.", load errors) | 14 regular | not shown |
| Small notes (edited mark, times of system messages, the composer's strip and counter, "Catching up…", DM member counts) | 12 regular | not shown |
| Jump bar | 14 medium | not shown |
| Connection bar, warning line | 13 medium, 13 regular | Akari's own |
| Message formatting: headings 1–3, subtext, code blocks, jumbo emoji | 24/32, 20/28, 16/22 bold; 13/18; 14/18 mono; 48/56 | not shown |
| Login screen | title 24 semibold, QR title 20 semibold, field label 12 bold, button 16 medium, notes 14 | not shown |
