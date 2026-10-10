# Server list

The rail on the far left. Numbers are for Discord's redesigned client (2025) at 100% zoom,
measured from the web client on 2026-10-10; sizes in [layout.md](layout.md), colors in
[theme.md](theme.md).

## Order, top to bottom

1. Home, which opens the DM list.
2. Avatars of DMs with unread messages. *Akari: later, with unread state.*
3. Servers the user is only previewing. *Akari: not in READY, so not shown
   ([dispatches.md](../protocol/dispatches.md#open-points)).*
4. A separator: 32 × 1, `frameBorder`.
5. The user's servers and folders in the user's own order. *Akari: the order is in the
   settings proto, which isn't read yet ([user-settings.md](../protocol/user-settings.md));
   until then the most recently joined server comes first.*
6. Servers blocked in the user's region, then one "unavailable servers" item whose tooltip
   gives their number. Discord's item links to its status page.
7. Add a Server and Discover. *Akari: left out.*

## Icons

- 40 × 40 with a fixed rounded-square mask, about 12 corner radius. Before the redesign an
  icon was a circle that turned into a rounded square on hover; that animation is gone.
- 8 between icons.
- Servers without an icon show initials (below).

## The pill

A small bar on the rail's left edge, next to an icon, in `textStrong`:

| State | Height |
|---|---|
| Unread | 8 |
| Hovered | 20 |
| Selected | 40 |

It is 4 wide where it shows (an 8 wide shape pushed halfway past the edge), rounded on its
right side, and changes height over 0.2 s with ease-out.

## Mention badge

A red (`#d22d39`) badge at the icon's bottom right, cut out of the icon, 16 tall. It is 16
wide for counts under 10, 22 under 100 and 30 above; from 1000 it shows "1k+" up to "9k+".
*Akari: later, with unread state.*

## Initials

A server without an icon shows letters made from its name:

1. "'s " becomes a space, so "Akari's Lab" reads "Akari Lab".
2. Every word becomes its first character.
3. Characters that aren't part of a word, such as emoji and punctuation, stay as they are.
4. Whitespace is removed.

There is no length limit. The font size shrinks with the length: 18 for 1–2 characters, 16
for 3–4, 14 for 5, 12 for 6, and 10 for 7 or more, in medium weight. The background is
`hoverBackground`; when hovered or selected it turns `brand` with white letters.

Examples: "Rust Programming Language" → "RPL", "🌸 Garden" → "🌸G",
"The 'Best' Server" → "T'B'S".

**Where Akari differs on purpose:** Discord's word rule only knows the letters A–Z, digits
and the underscore, so "Café" gives "Cé" and a name in Arabic or Japanese stays whole.
Akari counts any letter or digit of any script, and the underscore, as part of a word:
"Café Crème" → "CC".

## Tooltips

Hovering an icon shows the server's name to its right. Akari uses the native tooltip, a
place where the platform convention wins over Discord's custom bubble.

## Clicking

- Home opens the DM list and the conversation last opened there.
- A server opens the channel last opened in it, else its first text channel.
- The unavailable item does nothing in Akari.
