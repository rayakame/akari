# Embeds

How a message's embeds show, as the official Discord client shows them (canary 649848, read
2026-10-26; plan decisions D143–D145). Their pictures come from Discord's media proxy at
runtime like every image ([images.md](images.md)). Where Akari differs, the difference is in
*italics*.

## Which embeds show

- Embeds come under the attachments, 4 apart, in the message's order.
- Only a user's message shows embeds (default, reply, slash command, context menu command);
  the lines of system messages read their embeds' fields themselves.
- Never drawn: AutoMod, safety and age verification notices, application news, post previews,
  voice channel and poll result embeds, gifts, and the article cards of Discord's own shop and
  game pages (the client draws those as cards of its own).
- Embeds of one link merge: the first keeps everything, the others give only their image, up to
  four, as a gallery. A gallery of bare images shows alone, as one image would.
- "Show embeds and preview website links pasted into chat" (`render_embeds`) off: no embeds, the
  link stays. *Discord's own "Remove all embeds" isn't offered yet.*

## Media alone or a card

- An embed shows its media alone, without a card, when it's a GIFV, or when it isn't a rich
  embed and has neither an author nor a title (a pasted image link, a bare article image).
- Everything else is a card. With "When posted as links to chat" off (`inline_embed_media`), the
  card shows no image, video or thumbnail, and nothing shows alone.
- Media is sized from the embed's numbers alone: fitted inside 400 × 300 (400 × 450 for TikTok
  and portrait YouTube videos), never scaled up, corners 4; in a narrower column it's scaled down
  as a whole. A gallery is two columns 4 apart, 300 tall: two side by side, three as one left and
  two stacked right, four as 2 × 2, each cell cropped to fill. *It's as wide as its first image
  would be alone, or 400 when that's under 300 (Discord stretches a rich card's to 520).*
- When the content is one link and nothing else, and the message's one embed shows an image or
  a GIFV alone for it, the link's text is hidden (both settings on).

## The card

| Part | Look |
|---|---|
| Box | Discord's raised surface, a 1 pt subtle border, corners 4, a 4 pt bar on the left in the embed's color (none or white: the border's color); inside, 2 above, 12 left, 16 right and below |
| Rows | provider, author, title, description, fields, then the footer, each 8 below the one before |
| Provider | 12, regular; a link in the muted interactive color when it has one |
| Author | a round 24 pt icon, 8 from the name, centered on a row as tall as the icon; the name 14 semibold in the strong text color, a link when it has one |
| Title | 16 semibold, strong; with a URL it's a link in the link color |
| Description | 14 on 18 lines; not for image, video and GIFV embeds |
| Fields | up to three side by side in equal columns, 8 apart, 8 between rows; a field that isn't inline has a row of its own; the name 14 semibold, 2 above the value (14 regular) |
| Media | 16 below the text, the card's full width beside a thumbnail |
| Thumbnail | fitted into 80 × 80 on the right, 16 from the text, 8 down, corners 4; on a rich card beside its image, on others only without media |
| Footer | a round 20 pt icon, 8 from the text, on a row as tall as the icon; 12 medium; the text, " • " and the time |

- The card is as wide as its content, at most 516 inside. An image at least 300 wide, or a
  video, makes the card as wide as the media with the card's padding and borders. *Discord's
  card is 32 wider than the media, which shrinks the media by 1; Akari keeps the media's width.*
- Markdown only on rich embeds: titles and field names with styles, emoji and timestamps (no
  code blocks, mentions, lists, headings or line breaks, and no links when the embed has a URL
  of its own, which the title links to); *no quotes either*;
  descriptions and field values as messages, lists and headings only for messages after
  2023-03-22 (by the edit's time when edited) and never a heading in a field. Emoji are 18,
  never jumbo. Other embeds' text is plain, its line breaks kept. A title's or field name's line
  breaks show as spaces. Relative timestamps count down as in the message's text.
- The footer's time in Discord's words: "Today at 3:15 PM", "Yesterday at …", "Tomorrow at …",
  "10/24/2026 3:15 PM" for older days, "10/30/2026" for later ones, in the user's 12 or 24 hour
  clock.

## GIFVs and videos

- A GIFV plays muted and looping while GIFs play by the same rule as attachments (the window
  key, at least 60 % shown and "Play GIFs when Discord is focused" on, or the pointer over it);
  its poster and *Akari's own "GIF" badge* show otherwise.
- A video shows its poster, cropped to fill, at least 150 × 144, with Discord's pill in the
  middle: a black capsule at 60 %, 48 tall, with Play and Open Link (Open Link alone when the
  video can't play). *Both open the link in the browser: Akari has no inline player yet (D145).*
- A click on an image opens the original in the browser, as on attachments.
- *Embed media is asked when its row is laid out, not a screen ahead as attachments' is.*

## Covers

- A link inside a spoiler (`||https://…||`) covers every embed of the message: a card under the
  spoiler color with the "SPOILER" pill, its bar gone, *nothing of its text showing* (Discord
  blurs it 44 px); media alone as attachments are covered. A click reveals it until the channel
  closes. `render_spoilers` applies as to attachments. *A reveal holds for the embed at its place
  with its link, so an edit that brings another embed there starts covered.* A spoiler inside
  part of a link's text (`[||x|| y](url)`) covers nothing; *one around all of it can't be told
  from one around the link, and covers.*
- Flagged embeds (explicit or gore by the embed's own flags) and media still waiting for
  Discord's scan (people's embeds, not bots') take the same covers as attachments
  ([attachments.md](attachments.md)), by the same settings: on a card the whole card is black
  (with the warning, and the eye that shows it and hides it again), *nothing of its text showing*.
  The scan is asked with the message's attachments'.
