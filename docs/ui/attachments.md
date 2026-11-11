# Attachments

How a message's files show, as the official Discord client shows them (canary 649848, read
2026-10-24 and 2026-10-26; plan decisions D141–D144). The pictures come from Discord's CDN and
media proxy at runtime like every image ([images.md](images.md), [cdn.md](../protocol/cdn.md)).
Where Akari differs, the difference is in *italics*.

## Order

- A message's attachments come right under its text (2 apart), before anything else under it.
- Images and videos first, in Discord's mosaic, then every other file as a file row; each group
  keeps the message's order. Attachments Discord marks as a media post's thumbnail
  (`IS_THUMBNAIL`) aren't shown.
- What counts as media goes by the file name's extension, as the official client decides:
  images png, jpg/jpeg, jfif, webp, gif, heic/heif, dng, avif; videos mp4, webm, mov, qt; with a
  width and height above 0. SVGs are files.
- With "When uploaded directly to Discord" off (`inline_attachment_media`), images and videos
  are file rows too.
- *An image or video without its size, a 1 × 1 image and a sized file that isn't media show as
  file rows (Discord shows the first and nothing for the others).*

## The mosaic

Discord's numbers, from sizes alone, so a row is as tall before its media loads as after:

| Items | Layout |
|---|---|
| 1 | contained in 550 × 350, never scaled up; a video at least 150 × 110 |
| 2 | side by side, 273 × 273 each |
| 3 | one 364 × 350 on the left, two 182 × 173 stacked on the right |
| 4 | 2 × 2, 273 × 173 each |
| 5–10 | a top part (one 550 × 280, or a pair) over rows of three squares |

- 4 apart, the mosaic's corners 8 and its cells' 2; one item alone has 8.
- Cells are covered by their media (cropped, never stretched); one item alone shows it whole.
- *In a column narrower than 550 the whole mosaic is scaled down to fit (Discord's CSS shrinks
  it too; the exact steps weren't read).*
- Each item is asked at its cell's pixels on the window's screen (2× on Retina), through the
  media proxy, and kept in the media tier's memory ([images.md](images.md)).

## While it loads

- Discord's placeholder (the thumbhash it sends with the file) lies under the box, cropped as the
  image will be; without one, the box is black at 4 %.
- The image fades in over 0.2 s, or 0.05 s when it came within 200 ms; one already decoded shows
  at once.
- After 2 s a spinner shows in the top right corner, 4 in: a 24 pt black disc at 48 % with a
  turning white arc, fading in over 0.8 s.
- *The media of rows within a screen above and below what shows is asked early, so scrolling
  meets it decoded.*
- A box that leaves the list (a reused row, a closed channel) drops its request.

## Failure

- "Image failed to load." (Discord's words, 14, medium, muted) under *Akari's symbol* (Discord
  draws an illustration of its own).
- *A click on a failed box asks again; boxes that failed while offline ask again when the
  session is back (Discord has no retry).*

## GIFs and videos

- A GIF (or an animated WebP or AVIF) plays while the window is key, at least 60 % of it shows
  and "Play GIFs when Discord is focused" (`gif_auto_play`) is on, or while the pointer is over
  it. Reduce Motion stops the setting's part; the pointer still plays it. *Akari's own "GIF"
  badge* sits top left, 6 in, while it doesn't play.
- A video shows its poster (the proxy's still) with Discord's Play button in the middle: a black
  48 pt circle at 60 %, its symbol nudged 1 right.
- *Until the media viewer, a click on an image or a video opens the original in the browser
  (a fresh signed link). Discord opens its viewer for images and plays a single video in place.*

## File rows

- 432 wide at most (narrower in a narrow column), 74 tall: 16 in, a 1 pt border, corners 8,
  Discord's raised surface and light shadow.
- *Akari's symbol for the type* (a page, an archive, a sound wave, film, a photo, code, a
  table), where Discord draws its own file icons.
- The name in the link color, cut with an ellipsis, underlined under the pointer; a click opens
  the file in the browser (a fresh link), as Discord's web client does. When Discord gave the
  file a title, the name is the title and the extension.
- The size in Discord's words: base 1024, "512 bytes" (no singular), then two decimals
  ("1.50 MB"), a comma for thousands.
- *No Download button on hover yet: the name's link downloads, and Save comes with the viewer.*
- Audio files and voice messages are file rows until Akari has a player.
- Three files, 8 apart; the first 12 under the mosaic, or 10 under the text.

## Alt text

- With "Show image descriptions" (`view_image_descriptions`), an image with a description shows
  Discord's ALT button bottom left (4.5 in, 4 up): white "ALT" on black at 60 %, corners 4.
  A click shows "IMAGE DESCRIPTION (ALT TEXT)" over the description beside it.
- Never over a cover. VoiceOver reads the description either way.

## Spoilers

- An attachment marked a spoiler is covered: Discord's placeholder (else the spoiler color) and
  an uppercase "SPOILER" pill in the middle (15 semibold, 0.5 spacing, 8 and 12 around, round
  ends, black at 60 %, 88 % and white text under the pointer). Each cell of a mosaic has its own
  cover, with 8 pt corners.
- A click reveals that item until the channel closes; nothing hides it again.
- *Nothing of the image loads while it's covered, so hovering it or the viewer can't show it
  early (Discord blurs the loaded image).*
- `render_spoilers`: *Always* shows them uncovered, *If moderator* does in channels where the
  user has Manage Messages, *On click* (the default) covers them. The same setting applies to
  spoilers in the text.

## Sensitive media

Discord scans media and flags what may be explicit or gory; the user's settings say where that is
covered (Settings > Content & Social).

- **Where**: a guild channel or thread counts as a server, a DM or group DM as a friend's or a
  stranger's by the message's author. The user's own messages are never covered.
- **Unset settings**, as the official client resolves them: explicit media in servers shows; in
  a friend's DM it's covered only with the legacy filter at "all"; in a stranger's unless the
  filter is off. Gore shows in servers and friends' DMs and is covered in strangers'. When
  Discord knows the user is under 18, both are covered everywhere. Self-harm covers nothing
  (the client ignores that setting).
- **The cover**: black, a white warning symbol and, on an item alone that is at least 146 × 212,
  "May contain sensitive content". The eye button (bottom right, 4 in, 32 × 32, black at 60 %,
  corners 8) shows the media and hides it again. A click on the cover does nothing. Blur and
  Block look the same here (Discord's server enforces Block when sending).
- **Waiting for a scan**: media Discord hasn't scanned yet (in a place the settings cover) is a
  black box for up to 3 s while akari-core asks for the scan; then it shows, or takes its cover
  if Discord flagged it.
- *The spoiler setting never lifts these covers (Discord's "Always" does in this client).*
- *An item alone keeps its size under the cover (Discord makes the box at least 146 × 212 while
  covered), so the row doesn't move when it's shown.*
- *No "Why we hide some media" line under the message yet.*

## The age gate

- A guild channel marked age-restricted (or a thread in one), or every channel of a server whose
  owner marked it explicit or age-restricted, shows the gate in place of its messages and
  composer; the header stays. DMs never.
- "Age-Restricted Channel" ("Age-Restricted Server") over Discord's sentence, with Go Back and
  Continue; Continue opens every age-restricted channel of that server, now and later. *Akari
  keeps the agreement with the account (Discord keeps it in the browser for every account).*
- When Discord knows the user is under 18: "Sorry, you're not old enough to view this
  age-restricted channel" with Go Back only. *Discord also asks for a date of birth when it
  doesn't know the age; Akari has no such form and offers Continue.*
- Go Back opens the server's first channel that isn't gated, else Home.
- Nothing of a gated channel is loaded or marked read.
