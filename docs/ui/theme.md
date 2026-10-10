# Theme colors

Akari's color tokens for its dark theme (the default) and its light theme. The values are
those of Discord's default dark theme and its light theme after the 2025 redesign, measured
from the web client on 2026-10-10. Discord has two more themes, a softer dark one and a black
one; Akari doesn't mirror them.

The token names are Akari's. Each token has one value per theme, and every surface in the apps
uses a token, never a color of its own.

## Tokens

Translucent values are a color plus an opacity; they are drawn over the surface below them.

| Token | Used for | Dark | Light |
|---|---|---|---|
| `frame` | title bar, server rail, channel and DM list, login backdrop | `#121214` | `#f3f3f4` |
| `chat` | message list, channel header | `#1a1a1e` | `#fbfbfb` |
| `panel` | user panel | `#202024` | `#fbfbfb` |
| `card` | login card, sheets | `#28282d` | `#ffffff` |
| `frameBorder` | lines between the columns and under the title bar | `#97979f` at 12% | `#97979f` at 28% |
| `borderSubtle` | the line under the channel header, day dividers | `#97979f` at 12% | `#97979f` at 28% |
| `borderMuted` | the user panel's outline | `#97979f` at 4% | `#97979f` at 20% |
| `textDefault` | message text | `#efeff1` | `#2e2e34` |
| `textStrong` | names, headings, the server rail's pill | `#fbfbfb` | `#28282d` |
| `textMuted` | day dividers, system messages, secondary text | `#96979e` | `#6c6d76` |
| `chatTextMuted` | message times | `#81828a` | `#70717a` |
| `textLink` | links | `#4d96ee` | `#006dd4` |
| `textError` | errors next to form fields | `#f87e7a` | `#b92733` |
| `channelDefault` | channel and category names | `#81828a` | `#666770` |
| `interactiveText` | icons and buttons at rest | `#abacb2` | `#595a63` |
| `interactiveTextActive` | hovered and selected names | `#fbfbfb` | `#28282d` |
| `hoverBackground` | hovered rows and messages, the background of server initials | `#97979f` at 12% | `#97979f` at 12% |
| `selectedBackground` | the selected row | `#97979f` at 20% | `#97979f` at 24% |
| `inputBackground` | text fields | `#000000` at 12% | `#000000` at 2% |
| `danger` | error messages, e.g. above the login button | `#f23f43` | `#da373c` |
| `brand` | Home when selected, a hovered or selected server, primary buttons | `#5865f2` | `#5865f2` |
| `composer` | the composer card | `#222327` | `#ffffff` |

On the dark message list (`#1a1a1e`), `hoverBackground` comes out at about `#29292d` and
`selectedBackground` at about `#333338`.

## Avatar colors

Users without a picture get a colored circle with their initials. The colors are Akari's own,
picked by the user ID modulo 6, and the same in both themes:

| Index | Color |
|---|---|
| 0 | `#5865f2` |
| 1 | `#3e8e7e` |
| 2 | `#c06c2b` |
| 3 | `#a352b5` |
| 4 | `#c2445a` |
| 5 | `#4f7fba` |

## For later

Values Akari doesn't use yet:

| Use | Dark | Light |
|---|---|---|
| Tooltips and menus | `#242429` | `#ffffff` |
| Mention badges, the new-messages line | `#d22d39` | `#d22d39` |
| Status: online, idle, do not disturb, offline | `#45a366`, `#ffc04e`, `#da3e44`, `#84858d` | same |

## Before the redesign

People who remember the older dark client: its message list was `#313338`, the channel list
`#2b2d31` and the rail `#1e1f22`. Akari follows the current colors.
