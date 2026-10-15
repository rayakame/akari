# Server list

The rail on the far left. Numbers are for Discord's redesigned client (2025) at 100% zoom,
measured from the web client on 2026-10-10; sizes in [layout.md](layout.md), colors in
[theme.md](theme.md).

## Order, top to bottom

1. Home, which opens the DM list.
2. Avatars of DMs with unread messages. *Akari: the DMs and group DMs with mentions (every
   message in an unmuted DM is one), newest first, as initials avatars with the badge and the
   unread pill; clicking one opens it.*
3. Servers the user is only previewing. *Akari: not in READY, so not shown
   ([dispatches.md](../protocol/dispatches.md#open-points)).*
4. A separator: 32 × 1, `frameBorder`.
5. The user's servers and folders in the user's own order (below). Servers no folder entry
   places yet (joined since) come first, the newest join first; a newly created server shows
   at the very top (observed 2026-10-14). *Akari: the order comes from the settings proto
   ([user-settings.md](../protocol/user-settings.md#the-settings-proto)).*
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

## Folders

Observed in the official desktop client on 2026-10-14:

- **Collapsed:** a rounded 40 × 40 tile with its servers as small icons in a 2 × 2 grid (two
  servers fill the top row). A folder without a color is a neutral dark gray tile, not the
  brand color.
- **Expanded:** one rounded column in the folder's color at low opacity, neutral without a
  color, encloses the header and all its servers, with equal padding at top and bottom. Its
  corners are concentric with the server tiles inside (their radius plus the padding), so no
  tile's corner pokes out. The header is only the folder symbol, with no tile behind it, in the folder's color
  (Discord's blue-violet, `brand`, without one). Items keep the rail's gap. The state survives
  a restart; Discord doesn't sync it (each client keeps its own).
- **Unread:** a collapsed folder shows the white pill when a server in it is unread, and a red
  badge with the count at the tile's bottom right for mentions, plus the pill. *Akari: the
  same, adding up the servers' badges; an expanded folder's header shows neither, since its
  servers show their own.*
- The tooltip is the folder's name, or its servers' names joined with ", " when it has none.
- Clicking the tile expands or collapses it.

*Akari:* the collapsed tile shows up to four servers' initials, each on a 16 × 16 rounded
square in `hoverBackground`. A colored folder's tile and column use its color at 30%; one
without a color uses `folder`. Expanded, the header is `folder.fill` (18) in the folder's
color, or `brand`, in the 40 × 40 slot of a rail item; the column is 48 wide with corner
radius 16 (the tiles' 12 plus its 4 of padding), 4 of padding above the header and below the
last server, and items 8 apart. It is drawn behind them and never clips them. Expansion is kept per
account. A collapsed folder holding the open server carries
the selected pill. **Unverified:** that the official client does the same.

## Mention badge

A red (`#d22d39`) badge at the icon's bottom right, cut out of the icon, 16 tall. It is 16
wide for counts under 10, 22 under 100 and 30 above; from 1000 it shows "1k+" up to "9k+".
*Akari: `mentionBadge`, 12 pt bold white text, offset 4 past the icon's corner and cut out by
a 3 wide ring of the rail's color.*

## What makes a server unread

Observed in the official desktop client on 2026-10-14:

- **The pill** shows when one of the server's channels looks unread
  ([channel-list.md](channel-list.md#what-makes-a-row-unread)). So a message in a voice or
  stage channel's chat gives the server no pill, and neither do messages inside a thread the
  user hasn't joined. A new forum post makes the forum unread; **unverified:** the server's
  pill for it.
- **The badge** counts mentions of the user, @everyone and @here, and the user's roles,
  @silent ones included. With the server's "Suppress @everyone and @here" or "Suppress all
  role @mentions", those messages are plain unreads, with no badge.
- **Mutes:** a muted server shows no pill for plain messages, only the badge for mentions,
  and its icon is never dimmed. Plain messages in a muted channel, or in a channel under a
  muted category, give the server no pill.
- **Deleting messages lowers nothing:** the badge stays when a mentioning message is deleted,
  and the pill when the only unread message is, until the channels are read.

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
