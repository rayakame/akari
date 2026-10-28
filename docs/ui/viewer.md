# The media viewer

How a message's images and videos open full size, as the official Discord client shows them
(canary 653948, read 2026-10-28; plan decision D146). The pictures come from Discord's media
proxy at runtime like every image ([images.md](images.md), [cdn.md](../protocol/cdn.md)). Where
Akari differs, the difference is in *italics*.

## What opens it

- A click on an attachment's image or video opens the message's images and videos (not its file
  rows) at the clicked one, in the message's order.
- A click on an embed's image opens the embed's gallery (the images of embeds Discord merged by
  their link) at the clicked one; a single image, a card's thumbnail or a GIFV opens alone.
  Video embeds (YouTube and other players) never open it: their pill opens the link
  ([embeds.md](embeds.md)).
- *A single video attachment opens the viewer too (Discord plays it in the message, D141).*
- *Covered media stays out: a hidden spoiler, flagged media not shown and media waiting for its
  scan are left out of the items, so nothing under a cover shows (Discord shows flagged items
  blurred until zoomed).*
- A file's name still opens the file in the browser.

## Layout

The viewer covers the whole window over a black scrim at 0.92, and looks the same in the dark
and the light theme (Discord forces its Onyx theme on it). Its grid leaves 36 at the top and
bottom and 24 at the sides:

- **The top bar** (40 tall): the message author's avatar (40), with the name (16, medium,
  #dcdcdf) over the time (12, #81828a) beside it, at most 184 wide; the action bar on the right,
  12 before Close. *SF Symbols, never Discord's icons.*
- **The action bar**: 40 tall, Discord's surface (#17181b) with a 1 pt border (#97979f at 16 %),
  corners 12, 3 in; 32 × 32 buttons 4 apart, corners 8, 16 pt symbols in #96979e, #dcdcdf with
  #97979f at 20 % under the pointer. In order: Zoom In / Zoom Out, Save Image / Save Video, Open
  in Browser, More. Each button's tooltip and VoiceOver name are its words.
- **Close**: 40 × 40 in the top right corner, the surface with corners 12, a 20 pt symbol.
- **The arrows** (with more than one item): 40 × 40 like Close, vertically centred, 24 from the
  window's sides.
- **The strip** (with more than one item): a 40 × 40 square per item, 2 apart, centred along the
  bottom row; the ends' outer corners 8, the others 2; every item but the current one dimmed by
  black at 50 %. It scrolls sideways when it's wider than the room between the arrows, and keeps
  the current item in view. Thumbnails never animate.

## The media's size

- The media is centred on the window, fitted into the wider of two boxes and never scaled up:
  the window less 24 at each side (76 with arrows) and 176 in height, or the centre column (the
  window less 544) less 36 at the top and bottom (88 with arrows).
- It's asked at the fitted box's pixels on the window's screen (2× on Retina), at most 4096 a
  side.
- *Zoomed, it's asked at 2.5 × those pixels, never above the original's size or 4096 a side,
  through the media proxy (D132: nothing on screen fetches an original; Discord zooms to the CDN
  original).*
- It follows the window's size.

## Zoom and pan

- A click on an image zooms it 2.5 × at the pointer: the clicked point stays under the pointer.
  A click on zoomed media zooms out, unless the pointer moved 20 points or more (a drag).
- Zoomed, dragging and scrolling pan it, until its edge meets the window's; along an axis where
  it's smaller than the window it stays centred.
- Zoom In / Zoom Out zooms around the middle.
- Moving to another item zooms out. Escape closes, zoomed or not.
- The pointer is the zoom-in or zoom-out cursor over zoomable media (*the arrow before macOS 15,
  which has neither*), the pointing hand over the buttons and the strip, the arrow elsewhere.
- Zoomed, the author, the arrows, the strip and the action bar fade out over 0.15 s; Close
  stays. The action bar stays when its own Zoom button zoomed (Discord keeps it while it has
  the focus).
- *A pinch zooms in at the pointer and out (the Mac's gesture; Discord has none).*
- Videos don't zoom.

## Keys

- → ↓ Space: the next item; ← ↑: the previous; both wrap.
- Escape and ⌘W close.
- While it shows, the app's other shortcuts are off (Discord replaces them with the viewer's).
  *⌘Q and ⌘H still work.*

## Videos and GIFVs

- A video plays at once with sound, once, in the Mac's player (*AVKit's controls, not Discord's
  bar*); its poster shows until it plays. Space and the arrows still move between items.
- A GIFV plays muted and looping over its poster, zoomable as an image. *Only Discord's copy
  plays; without one the poster shows as an image.*
- GIFs and GIFVs play while the viewer is the key window and the app is active.

## Actions

- **Save Image / Save Video** (not for GIFVs): a save panel with the file's name, then the
  original, written whole or not at all.
- **Open in Browser**: the original (a fresh signed link); a video's is the file that plays.
- **More**:
  - **Copy Image** (still images): the original on the pasteboard.
  - **Copy Media Link**: the original's link.
  - **View Details** (attachments): Filename (the title and the extension when Discord gave the
    file a title), Size ("1920x1080 (2 MB)": whole megabytes rounded up, from 1000 MB gigabytes
    with one decimal) and Description (Alt Text) when there is one. A click copies the value.
- *No Forward (Akari doesn't forward yet), no Copy Attachment ID (no developer mode) and no
  toasts after a copy (Akari has none yet).*
- In a media or forum channel's post whose channel hides download options, an attachment keeps
  only Zoom, and a video has no action bar; embeds keep theirs.
- Clicks on the scrim or on the top bar's empty space close the viewer; clicks on the media,
  the buttons, the arrows, the strip or a video don't.

## The top bar

- The message's author (never an embed's), named as the message list names them.
- The time as the message group's header says it ("8:45 AM", "Yesterday at 8:45 AM", the short
  date and time), with the full date as its tooltip.

## Motion, loading and preloading

- It fades in over 0.2 s while the media grows from 0.9; closing fades it out. Moving between
  items has no transition. With Reduce Motion, none of it.
- Loading is as in the message list ([attachments.md](attachments.md)): the placeholder, the
  fade, the spinner after 2 s, "Image failed to load." with *Akari's symbol*; *a click on the
  failure retries (Discord retries on its own)*.
- The previous item is asked for with two items or more, the next with three or more, images
  only, at their fitted size, so moving shows them at once.
- Everything the viewer asked for is let go when it closes or moves on.

## VoiceOver

- The viewer is a dialog named "Media Viewer"; the buttons are named by their words; the strip's
  items are "Item 2 of 5 (current)" and "Item 3 of 5".
