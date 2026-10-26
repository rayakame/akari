# Emoji and stickers

How custom emoji and stickers show in a message, as the official Discord client shows them
(canary 649848, read 2026-10-24; plan decisions D139 and D140). Unicode emoji are text in the
system's emoji font ([markdown.md](markdown.md)); reactions aren't in Akari yet. The pictures
come from Discord's CDN at runtime like every image ([images.md](images.md),
[cdn.md](../protocol/cdn.md)).

## Custom emoji

- **In the text**: `<:name:id>` and `<a:name:id>` are a square box in the line, 1.375 × the
  line's font size (22 in the body, 18 in subtext, 33 in a large heading), its bottom on the
  line's bottom; the whole image fits inside it (contain), so a wide emoji is shorter and
  centered, never cut. The box is a text attachment that only makes room
  (its width, never taller than the font's ascent, so no line grows for it); the layer with the
  image sits on it, above the text, placed by the same layout that places pills and spoiler
  backgrounds. Text wraps around it as around a word; selecting across it works as across text,
  and a selected emoji stays visible over the selection.
- **Jumbo**: a message of 1 to 30 emoji (Unicode or custom) and whitespace, nothing else, shows
  them large: Unicode emoji at 48 on a 56 line, custom emoji in a 48 box centered on that line.
  Never in a reply's preview. *Discord's limit is 30 (`MAX_EMOJI_TO_BE_JUMBO`); akari-markdown
  had 27 from an older client.*
- **The name**: copying a selection that crosses it gives `:name:`; VoiceOver reads `:name:`.
  Hovering it shows its hover card and a click its popout (below).
- **Animation**: an `<a:…>` emoji plays while the window is key and the app active, and
  `animate_emoji` is on, at least 70 % of its message shows and the message is among the 100
  nearest the bottom, or while its message is hovered. Reduce Motion turns the setting's part
  off; hover still plays. *Akari counts the message's visibility for every emoji in it, and
  the message's hover for every emoji in it (Discord tracks the emoji's own hover too).*
- **Failure**: an emoji that can't load shows `:name:` in its box, small and muted, the text
  around it unmoved. A hidden spoiler's emoji stay under the cover and aren't asked for.
- **Elsewhere**: a thread box's second line and the Threads browser's rows read a custom emoji
  as `:name:` (one line of small text, no image layers); the New Thread form's preview shows
  the message as the list does, emoji and stickers included.

## Stickers

- **Where**: a message's stickers come under its text (and under its files), above its thread
  box, in a row 8 apart from the text's left edge; each is a 160 × 160 box, asked at 160 × the
  backing scale (320 on Retina, Discord's `size × min(2, DPR)`). A user's message has one
  sticker; a bot's up to three.
- **Formats**: PNG and APNG stickers come as PNG, GIF stickers as GIF (through Discord's media
  proxy); the still of an animated one first, the animated form when it plays.
- **The box** fits the whole sticker inside it (contain), as the emoji's does: a wide one is
  shorter, centered, never cut; the box's size is known before the image loads.
- **Animation**: by `animate_stickers`: *always* plays while the window is key and at least
  70 % of the message shows; *on interaction* plays while the message is hovered; *never* keeps
  the still. With Reduce Motion, *always* acts as *on interaction*.
- **Lottie** (format 3): no renderer in this milestone. The box shows a face symbol and the
  sticker's name on the code background. (Backlog: weigh ThorVG or rlottie against their
  size.)
- **Failure**: a warning symbol and "Uh Oh!", Discord's words, in the same box.
- A click on a sticker opens its popout (below); the pointer is the hand over it, and a
  right-click opens the message's menu. A thread box's second line says "Click to see sticker"
  for a newest message that is only a sticker.

## Hover card

Over a custom or Unicode emoji in a message's text, or a sticker under it (not in a channel's start,
a notice, a file's line or the New Thread form's preview), the pointer is the hand (the message
list's cursor authority), and after 300 ms a card shows above it, its arrow 3 above the item (the
part of it the list shows); it goes at once when the pointer leaves the item, on a click and while
the popout is open, and takes no clicks. *Akari: at the top of the screen it shows below the item,
and pushed in from a screen edge its arrow still points at the item.* An animated emoji plays in it
while the window is key.

- Emoji (custom and Unicode): the emoji at 48, then `:name:` (14 medium, one line) over "Click
  to learn more" (14, muted), 8 apart; padding 8 × 12, at most 272 wide; `surfaceHigh`, radius
  8, a 1 px `borderSubtle` edge, a 16 × 10 arrow. A Unicode emoji's name is gemoji's
  (`:grinning:`, `:thumbsup_tone3:`, `:people_holding_hands_tone1_tone2:`, `:flag_de:`,
  `:regional_indicator_a:`, else Unicode's name), as Discord's; what Discord's parser leaves as
  text isn't an emoji there: ©, ®, ™, a symbol without its variation selector (✔, ♥), emoji in
  code or a hidden spoiler.
- Sticker: a sticker symbol (16) and its name (14 semibold), "Click to learn more" under them,
  centered, at most 240 wide. *Discord's is 200 with gg sans; the system font is wider, so names
  that fit there fit here.*

## Popout

A click opens a popout to the right of the item, centered on it (to the left near the window's
edge), 288 wide and as tall as its content; a press outside or Escape closes it.

- **Custom emoji**: the emoji at 48, `:name:` (16 semibold), one line of what it is, then on a
  darker band the server it's from: "This emoji is from" (a server the user is in) or "Get this
  emoji from", the server's icon (32, rounded square), its name and a line: "{n} Online ·
  Discoverable Server", "{n} Online" or "Invite-only server". A click on a server the user is
  in opens it. A discoverable server the user isn't in shows "Show More Emoji": up to 12 of its
  other emoji at 32. The line follows Discord's matrix: this server or one of the user's,
  Nitro or not ("You can use it everywhere." / "Type its name in the chat bar to use it." /
  "Get Nitro to use it everywhere."), a discoverable server ("Want to use this emoji
  everywhere? …join the server."), an app ("This emoji is from the {app} app"), or neither
  ("…a server that is either invite-only or unavailable."). Where it's from: the user's own
  servers from the store (READY's guild emoji), others from `GET /emojis/{id}/source`, which
  answers only discoverable servers, given 5 s.
- **Sticker**: a server's sticker shows itself at 48, its name and the same states in sticker
  words ("This sticker is from", "Get this sticker from", "Show More Stickers"); Nitro for
  stickers is Nitro or Basic, not Classic. A pack's sticker shows its name, "This is a
  **{pack}** sticker." and up to three others from the pack at 80. One Discord can't place shows
  the invite-only line. (`GET /stickers/{id}`, `/stickers/{id}/guild`, `/sticker-packs/{id}`.)
- **Unicode emoji**: the emoji at 48, `:name:` and "A default emoji. You can use this emoji
  everywhere on Discord." (`:pizza:` gets " (without pineapple)" in English).
- "Show More" grows the popout around the item, inside the window. A sticker is a button for
  VoiceOver ("Sticker, {name}"); an emoji in text opens its popout only with the pointer.
- *Akari leaves out what it can't do yet: the "Join Server", "Get Nitro", "Subscribe", "Change
  Tier" and "View More Stickers" buttons, the "Join Server" link and the "Learn more" links;
  the server row of a server the user isn't in has no click (Discord opens a preview); an unsold
  pack's "They are no longer available." (it needs the shop's pack list).*
