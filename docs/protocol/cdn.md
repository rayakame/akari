# Images: the CDN and the media proxy

How `akari_core::media` gets images: which URL each image has, which size and form it asks
for, how signed attachment links are renewed, and what it keeps on disk. Sources:
[CDN Formatting](https://docs.discord.food/reference#cdn-formatting), the
[message](https://docs.discord.food/resources/message) and
[sticker](https://docs.discord.food/resources/sticker) pages, and Discord's web client (canary
649848; module IDs in brackets, read on 2026-10-24). Decoding and drawing are each app's.

## Keys, not URLs

An image is named by Discord's IDs (`ImageKey`): an avatar (user, hash), a member's guild
avatar, a default avatar (0–5), a server, group DM or role icon, an emoji, a sticker, an
attachment (channel, message, attachment), or an embed's image, thumbnail or icon (channel,
message, the embed's place, the slot). The core builds the URL; signed links never leave it.
A new avatar or icon has a new hash, so a new key.

## Hosts

| Host | What |
|---|---|
| `cdn.discordapp.com` | avatars, icons, emoji, default avatars, attachments' originals |
| `media.discordapp.net` | attachments and videos' posters at a size, stickers |
| `images-ext-1.discordapp.net`, `images-ext-2.discordapp.net` | embeds' external images (`external/{signature}/{scheme}/{host}/{path}`) |

Akari fetches from these hosts only, over HTTPS on 443 (an `Endpoints` override for tests
names the first two). An attachment or embed whose link points elsewhere is `Unsupported`.

## Sizes

Every request asks at least the pixels its box covers on screen: the box's points times the
backing scale (2 on Retina), rounded up. The host passes those pixels (`ImageRequest.width`,
`height`); the core rounds them up to what Discord serves.

- **The CDN's sizes** [776231]: 16, 20, 22, 24, 28, 32, 40, 44, 48, 56, 60, 64, 80, 96, 100,
  128, 160, 240, 256, 300, 320, 480, 512, 600, 640, 1024, 1280, 1536, 2048, 3072, 4096. An
  avatar's box of 40 points asks `size=80` on Retina. *Discord asks twice its drawn size for
  some places (a message avatar 80 for 40 points, so 160 on Retina); Akari asks the screen's
  pixels: half the bytes, as sharp.*
- **The proxy's sizes**: the box covered (a cover cell needs the larger ratio), the longest
  side rounded up the same ladder from 128 and the other side following the source's aspect,
  never above the source (then no size is asked) and at most 4096. So resizing a window
  rarely asks again, and nearby boxes share one file. *Discord asks the drawn size exactly; an
  experiment of its own snaps like this.* Whether Discord's client asks a small image above its
  size, and the viewer's requests, are open (Chris's observation O1).
- The CDN never answers larger than the source; a smaller answer is the source, and answers
  any later size.

## Forms

| Image | URL | Notes |
|---|---|---|
| avatar | `avatars/{user}/{hash}.webp?size=N` | `a_` hash animating: `&animated=true`, or `.gif?size=N` |
| member avatar | `guilds/{guild}/users/{user}/avatars/{hash}.webp?size=N` | as an avatar |
| server icon | `icons/{guild}/{hash}.webp?size=N&quality=lossless` | `&animated=true` after `quality` |
| role icon | `role-icons/{role}/{hash}.webp?size=N&quality=lossless` | the client's path; the docs differ |
| group DM icon | `channel-icons/{channel}/{hash}.png?size=N` | always a still PNG, as the client asks |
| emoji | `emojis/{id}.webp?size=N` | animated: `&animated=true`, or `.gif?size=N` |
| default avatar | `embed/avatars/{0–5}.png` | 256 × 256 PNGs; no query (size is ignored); 6 is a 404 |
| sticker | `media…/stickers/{id}.{ext}?size=N` | PNG → `.webp` + `quality=lossless`; APNG `.png` (still: `&passthrough=false`); GIF `.gif` (still: `.webp`); Lottie unsupported |
| attachment | its `proxy_url` with the signature, then `format=webp`, `quality=lossless` for `.png` and `.avif`, `width`, `height` | GIF animating: no `format`; animated WebP or AVIF (`IS_ANIMATED`): `format=webp&animated=true`; a video's poster: `format=webp` |
| embed image, thumbnail | its `proxy_url`, sized as an attachment | |
| embed author or footer icon | its `proxy_icon_url` as Discord gives it | |

Stills are WebP. Animations are WebP, or GIF on a Mac whose ImageIO composites animated WebP
wrong (macOS 14; the host picks per request with `AnimatedFormat`). The query order is
Discord's: size, quality, animated. An attachment's kind comes from its file name's extension,
as the client decides [127038]: images png, jpg, jpeg, jfif, webp, gif, heic, heif, dng, avif
(SVGs are files); videos mp4, webm, mov, qt. An attachment without a width and height isn't an
image here.

Originals (`Account::original_url`, `save_original`): an attachment's CDN `url`, renewed when
due; an embed's own `url` (saved from the proxy's copy, since Akari fetches only from Discord);
identity images as PNG, or GIF when animated, at `size=4096`.

## Downloads

- One HTTP client for images, shared by every account: the browser user agent the REST client
  sends, Chromium's `Accept` for images, no `Authorization`, none of Discord's headers
  (`X-Super-Properties`, `X-Discord-Locale`), no cookies, redirects refused, HTTP/2, the same
  rustls configuration as REST. Its connections are its own.
- Only `image/*` answers whose bytes are PNG (APNG), GIF, WebP or JPEG are kept; the bytes give
  the size and whether it animates. At most 16 MB and 64 megapixels (`TooLarge`).
- At most 8 downloads run at once, the newest asked first: what's on screen now. Asks for the
  same image, form and size share one download; when the last asker goes, it's cancelled.
- 5xx and network errors: 3 attempts in all, the waits doubling from 0.5–1 s with jitter, at
  most 5 s. *Discord's client tries 5 times [loader, §9 of the research].* A 429 pauses that
  host for its `Retry-After` (at most 60 s; 1 s without one), every account's downloads from it
  included, also those already waiting for a slot, and counts as an attempt. 404, 403 and 410: `NotFound`, remembered for the
  session. 415 (the proxy's "not an image"): `NotAnImage`.
- Offline (`Account::disconnect`, before `connect`): kept files answer; anything else fails at
  once as `Unavailable`, for the host to ask again online. A closed account answers `Closed`.

## Signed attachment links

Attachment links carry `ex` (expiry, hex Unix seconds), `is` (issued) and `hm` (signature).
The client treats a link as expired an hour before `ex`, or when it has none [679164].

- Before fetching through a signed link that is due, the core renews it with `POST
  /attachments/refresh-urls {"attachment_urls": […]}` (the account's REST, with its token and
  rate limits), which answers `{"refreshed_urls": [{"original", "refreshed"}]}`. Asks within
  50 ms share a request of at most 50 links; one request runs at a time per account. The
  proxy URL takes the renewed signature in place of its own.
- Renewed links are kept beside the store, by path, for the session. *Discord's client
  instead clears and fetches again a channel whose cached messages hold an expired link;
  Akari keeps the messages (what a UI shows is never dropped), and a renewal reloads no row.*
- A 403 or 404 through a link whose `ex` has passed (a clock off by more than the margin)
  renews it once more and tries again; then it's `NotFound`.
- When a renewal fails, or Discord answers without the link, a link that hasn't expired yet is
  used as it is. An expired one Discord won't renew is `NotFound` for the session (no renewal
  is asked again); a failed request is `Unavailable`.
- Logs name image keys (IDs) and outcomes, never a URL: a signed link is a secret for as long
  as it's valid.

## On disk

- Per account, under the directory the host passes (`DiscordClient::with_image_cache`): on
  macOS `~/Library/Caches/<bundle>/Images/<user id>/`, which the system may purge. The user is
  the one READY names (the host's guess until then); accounts of one user share it.
- The files as Discord sent them under `files/`, named by a digest of their key (no key text in
  a path), and an index in `index.sqlite` (key, form, size, animated, whether it's the
  source's own size, bytes, last use), served by a thread of its own.
- A download is written apart first (`tmp/`, per launch), then moved into place and indexed.
  At open, files the index doesn't know (older than a minute) and earlier launches' downloads
  (older than an hour) are deleted: another process may be using the directory (two akari-cli
  runs), so its newest files stay. Each process recounts the index's size every 64 files it
  keeps.
- The smallest kept file with enough pixels, in the same form, answers; so does one of the
  source's own size. A file gone when it's answered (the system purged it) is fetched again.
- Past 256 MB the least recently used go, down to 90 %, never one used in the last minute.
- An index that isn't one (not a database, corrupt, another schema) is dropped with the
  directory and starts empty. One that is busy, or being created by another process, is
  tried again and never deleted.
- Logout and `forget_account` delete the account's directory, also while it's open (and when
  this process never opened its index, or another process uses it): the open
  account's later asks are `Unavailable`, nothing downloads, and a download that was on its way
  leaves nothing behind;
  `clear_caches` deletes those of accounts that aren't open; `Account::clear_images` empties
  it.

## Placeholders

Attachments carry a thumbhash (`placeholder`, only `placeholder_version` 1).
`media::placeholder` decodes it as Discord's client does (a worker in the bundle): thumbhash's
algorithm with the luma AC halved and only up to the first harmonics (`detail: 1`), chroma 1.1
times stronger (`pop: 1.1`), at most 32 pixels on the longest side, bytes truncated.

## Layout

`media::layout` places media from sizes alone, so a row's height is final before anything
loads: contain and cover never upscale (1920 × 1080 → 550 × 309, 1080 × 1920 → 197 × 350,
100 × 5000 → 7 × 350); a single attachment fits 550 × 350, a video at least 150 × 110 [619517,
515718]; the mosaic of 2–10 [394839, 838541]; embed images 400 × 300 and thumbnails 80 × 80
[659674]; the forum gallery's columns and card heights [559296]. A column narrower than 550
scales the mosaic as a whole (Akari's rule until O1).
