# Images

How the pictures Discord shows reach the screen in Akari: which size each place asks for, how
they're decoded and drawn, and when an animated one moves. The core's side (keys, URLs, the
CDN's sizes, downloads, the files on disk) is in
[cdn.md](../protocol/cdn.md). Numbers are points at 100 % zoom; sources are Discord's web
client (canary 649848, read 2026-10-24) and the plan's decisions D130–D137.

## Sharpness

Every image is asked at the pixels its box covers on screen: the box's points times the
backing scale (2 on Retina), each side rounded up, then up to the next size Discord's CDN
serves. The file is decoded to exactly those pixels (never above the source's own size) and
drawn 1:1 as a layer's contents on whole backing pixels, so nothing is resampled. Avatars and
icons cover their box (the overflow clipped); emoji and stickers fit inside it, the whole
picture centered, a wide one shorter than the box (Discord's `object-fit: contain`). When the
window moves to a display with another scale, every image is asked again at the new size,
and the old one stays until the new one has decoded. *Discord asks twice the drawn size for
some places (a message avatar 80 for 40 points, 160 on Retina); Akari asks the screen's pixels:
half the bytes, as sharp.*

## Identity images

Drawn size per place (the pixels asked on Retina in brackets):

| Place | Size | Animates |
|---|---|---|
| Message header (a group's first message) | 40 (80) | while any message of the group is hovered |
| Reply line | 16 (32) | never |
| DM list row | 32 (64) | while the row is hovered |
| Unread DM on the rail | 40 (80) | while hovered |
| User panel | 32 (64) | never |
| Group DM icon | 32 (64) | never (a still PNG, as Discord asks) |
| Server icon on the rail | 40 (80), lossless | while hovered |
| Folder minis | 16 (32), lossless | never |
| Title bar | 16 (32) | never |
| Channel start of a DM | 80 (160) | never |
| Thread box's newest author | 16 (32) | never |
| Threads browser rows | 16 (32), facepiles 24 (48) | never |
| More Active Threads | 16 (32) | never |
| Custom emoji in the text | 22 (44), jumbo 48 (96) | by `animate_emoji`, visibility, hover ([emoji.md](emoji.md)) |
| Sticker | 160 (320) | by `animate_stickers`, visibility, hover |

- **Users never show initials.** A user without an avatar shows Discord's default avatar,
  fetched from its CDN (`embed/avatars/{index}.png`) like any avatar; the index follows the
  client's rule ([cdn.md](../protocol/cdn.md)). The official client bundles the same six
  pictures; Akari bundles nothing of Discord's (D154).
- **While an avatar loads**, the box shows a disc in `avatarPlaceholder` (Discord's
  `background-mod-subtle`); a failed avatar keeps the disc, and asks again once the session is
  back online. No fade when it lands. A box recycled for another person or server clears at
  once; only the same subject's new picture keeps the old one until the new has decoded.
- **Servers without an icon show their initials' tile** ([server-list.md](server-list.md)). A
  server's icon is drawn alone, nothing filled under it (loading, loaded, hovered or
  selected), so transparent pixels show what's behind; a failed icon falls back to the tile;
  a changed icon swaps only once the new one has decoded. No loaded image anywhere has a fill
  under it: the avatars' disc shows only while loading or after a failure. Group DMs without an icon show the group symbol on a disc
  (Discord's group defaults are its own assets).
- **A member the store doesn't know yet** (a facepile face, a thread's owner) shows the
  loading disc.
- The QR login's "Confirm on your phone" shows the disc with a person symbol: there is no
  account yet to fetch the scanned user's avatar with.

## Animation

Whether an animated image plays is one rule (`MotionPolicy` in AkariKit) over the user's
settings (`gif_auto_play`, `animate_emoji`, `animate_stickers`, from the settings proto's
`text_and_images`), the system's Reduce Motion, the window being key and the app active,
how much of the image shows, and hover:

- Nothing plays while the window isn't key or Akari isn't the active app.
- **Avatars** play only while hovered: a message group's header avatar while any message of
  the group is hovered, a DM row's while the row is, a rail icon's while it is. The user
  panel, replies, facepiles and thread boxes never play.
- **Custom emoji** play with `animate_emoji`, at least 70 % of their message visible and the
  message among the 100 nearest the bottom, or while the message is hovered; **stickers** by
  `animate_stickers` (always, on hover, never) and 70 % ([emoji.md](emoji.md)); **GIFs** (PR D)
  with `gif_auto_play` and 60 %, or while hovered.
- Reduce Motion turns autoplay off (GIFs, emoji) and stickers' "always" into "on hover", as
  the official client does; hover still plays.

Playback is Akari's own: one clock for every animation, each frame decoded at the box's
pixels off the main thread and shown at its delay, loops honoured, the last frame kept when
they're up. A paused animation shows its still, a separate file. Animations are asked as
WebP, or as GIF on a Mac whose ImageIO composites animated WebP wrong (macOS 14): a launch
probe decodes a small animated WebP shipped with AkariKit and checks one frame's pixels.

## Memory

Decoded images live in two tiers bounded in bytes: identity images (avatars, icons, emoji,
stickers) 16 MB and a message's media 48 MB, least recently used first, never an image a box
on screen shows. A memory warning drops everything not on screen; a hidden or minimized
window drops the media tier's. The files themselves stay on disk, 256 MB per account
([cdn.md](../protocol/cdn.md)).

## Rows and the pointer

Every image's size comes from metadata, so a row's height is final before anything loads and
never changes when it arrives. The message list stays the one cursor authority
([cursors.md](cursors.md)): image views draw nothing, track nothing and take no clicks; the
list's own tracking decides the cursor and the hover that animates an avatar.
