# Emoji and stickers

How custom emoji and stickers show in a message, as the official Discord client shows them
(canary 649848, read 2026-10-24; plan decisions D139 and D140). Unicode emoji are text in the
system's emoji font ([markdown.md](markdown.md)); reactions aren't in Akari yet. The pictures
come from Discord's CDN at runtime like every image ([images.md](images.md),
[cdn.md](../protocol/cdn.md)).

## Custom emoji

- **In the text**: `<:name:id>` and `<a:name:id>` are a square box in the line, 1.375 × the body
  size (22 at 16), its bottom on the line's bottom; the image is drawn to fit it. The box is a
  text attachment that draws nothing of its own; the layer with the image sits on it, placed by
  the same layout that places pills and spoiler backgrounds. Text wraps around it as around a
  word; selecting across it works as across text.
- **Jumbo**: a message of 1 to 30 emoji (Unicode or custom) and whitespace, nothing else, shows
  them large: Unicode emoji at 48 on a 56 line, custom emoji in a 48 box centered on that line.
  Never in a reply's preview. *Discord's limit is 30 (`MAX_EMOJI_TO_BE_JUMBO`); akari-markdown
  had 27 from an older client.*
- **The name**: hovering the emoji shows `:name:` as a tooltip. Copying a selection that
  crosses it gives `:name:`; VoiceOver reads `:name:`.
- **Animation**: an `<a:…>` emoji plays while the window is key and the app active, and
  `animate_emoji` is on, at least 70 % of its message shows and the message is among the 100
  nearest the bottom, or while its message is hovered. Reduce Motion turns the setting's part
  off; hover still plays. *Akari counts the message's visibility for every emoji in it, and
  the message's hover for every emoji in it (Discord tracks the emoji's own hover too).*
- **Failure**: an emoji that can't load shows `:name:` in its box, small and muted, the text
  around it unmoved. A hidden spoiler's emoji stay under the cover and aren't asked for.

## Stickers

- **Where**: a message's stickers come under its text (and under its files), above its thread
  box, in a row 8 apart from the text's left edge; each is a 160 × 160 box, asked at 160 × the
  backing scale (320 on Retina, Discord's `size × min(2, DPR)`). A user's message has one
  sticker; a bot's up to three.
- **Formats**: PNG and APNG stickers come as PNG, GIF stickers as GIF (through Discord's media
  proxy); the still of an animated one first, the animated form when it plays.
- **Animation**: by `animate_stickers`: *always* plays while the window is key and at least
  70 % of the message shows; *on interaction* while the message is hovered; *never* doesn't.
  Reduce Motion turns *always* into *on interaction*.
- **Lottie** (format 3): no renderer in this milestone. The box shows a face symbol and the
  sticker's name on the code background. (Backlog: weigh ThorVG or rlottie against their
  size.)
- **Failure**: a warning symbol and "Uh Oh!", Discord's words, in the same box.
- Nothing in the row takes clicks yet (Discord opens the sticker's pack); the pointer is the
  arrow and a right-click opens the message's menu. A thread box's second line says "Click to
  see sticker" for a newest message that is only a sticker.
