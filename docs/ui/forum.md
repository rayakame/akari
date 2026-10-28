# The forum page

What a forum channel shows when the user opens it: its posts as cards in the message list's
place, with search, the tag filter and Sort & View. Read from Discord's web client (canary
653948, 2026-10-28; modules in brackets). Posts opening beside the forum, creating posts, NEW
and the unread look on cards, and media channels come with the next forum PR; until then the
differences are marked *Akari:*.

## Where it opens

- A forum row in the channel list opens the page; so do a forum's mention in a message and a
  link to it. The header shows the forum's symbol and name, without its topic: the topic is the
  page's guidelines. Media channels don't open yet (their tooltip says so).
- Opening the page refreshes its list and asks Discord for the first 10 posts' first messages
  (the official client does that as the row is pressed, module 473503). It subscribes the guild
  as opening any channel does, so the gateway's thread lists arrive.
- The page is read like a channel: while it shows, the window is key and the user is engaged,
  the forum is acked at "now" whenever a post is newer than its ack or a post has unread
  activity past it ([read-states.md](../protocol/read-states.md#forums)). An age-restricted
  forum shows the age gate first, as a channel does, and isn't read behind it.
- *Akari:* a card's click opens its post in full, as Shift+click does in Discord, until posts
  open beside the forum.

## The list [207777]

- **Active posts** are the forum's posts the store has (THREAD_LIST_SYNC, THREAD_CREATE):
  pinned first, then by last activity (the newest message, else the post's ID) under Recently
  Active, by creation under Date Posted.
- **Frozen between refreshes**: which posts show and their order change only when the list
  refreshes: opening the forum, a new connection's READY, a THREAD_LIST_SYNC of its guild, and
  the pill. A deleted post leaves at once and a pin change re-sorts; new activity doesn't move a
  card.
- **"N New Posts"**: someone else's new post raises the pill at the bottom center
  ("1 New Post", "3 New Posts"; 36 tall, radius 36, `surface-high`, `border-subtle`, the brand's
  text); a click refreshes the list and scrolls to the top. The user's own post joins at once.
- **The tag filter** works on the client: a post shows with any of the tags (Match Some) or all
  of them (Match All); an untagged post never shows while filtering.
- **Older Posts**: the archived posts load 25 a page, the first page when the page shows (and
  again for a filter the connection hasn't loaded), the next within 200 of the list's bottom (a
  gallery card's height in the gallery, at least 200), never while a search shows. Each tag
  filter pages on its own; the section is every archived post of the forum Akari has that passes
  the filter, down to the last page's oldest, by the sort. "OLDER POSTS" (12 bold, `text-muted`,
  16 above and 8 below) heads it when active posts show above. Without Read Message History,
  "You do not have permission to view the post history of #ideas." stands in its place
  (`base-lowest`, 40 tall, radius 4, 16 regular `text-muted`).

## The header row

- The search row: a card (radius 12, `base-low` under the input background, `border-subtle`,
  the brand's border while focused), 8 inside: a 32 box with the magnifying glass (a spinner
  while searching), the field (20 semibold, "Search", 100 characters at most, typed line breaks
  become spaces) and, with a topic, the guidelines button ("Post Guidelines"). *Akari: "Search"
  until posts can be created, where Discord says "Search or create a post...".*
- **Search** runs 350 ms after the query or the tag filter last changed, not again for the same
  query and tags, and not while one runs (the newer one runs after it). Enter searches no sooner.
  The results replace both sections, in Discord's order. Under the field, while a search runs or
  has answered: "Searching...", "No matching posts" (also for a failure), or "1 post with
  "bug"" / "3 posts with "bug"", then "Clear" (12 semibold) once answered. Emptying the field or
  "Clear" shows the list again. Without Read Message History nothing is searched and "You do not
  have permission to search the post history of #ideas." takes the results' place.
- **The guidelines** (the forum's topic): hidden until the button opens them, 16 under the
  search row, a card with "Post Guidelines", "Close" and the text (14), at most 220 tall until
  "View All". *Akari: "View All" shows the rest in place, where Discord opens a modal; the topic
  shows as plain text.*

## The tags bar

16 above, 4 below, 32 tall:

- **"Sort & View"** (`base-lowest`, a capsule, 12 and 8 inside): a menu with "Sort by" (Recently
  Active, Date Posted), "View as" (List, Gallery), "Tag Matching" (Match Some, Match All; with
  tags) and "Reset to default", which restores the forum's sort, view and matching and keeps the
  tag filter. The choices stay per forum for the session, starting from the forum's defaults.
- A 2 × 24 divider (16 each side) when the forum has tags, then the tags as pills on one row
  (32 tall, a capsule, 12 inside, 14 semibold, the emoji before the name; selected: the brand at
  15% with the brand's border), and "All" right after the last that fits, or the filter's count
  in a brand bubble, with a chevron: it opens "Select Tags" with every tag and "Clear all".

## Cards

Cards are buttons: the hand over all of them ([cursors.md](cursors.md)), Tab reaches them,
VoiceOver reads "Post {title}, {count} messages".

- **List** (16 from the page's sides, 8 apart): radius 12, `base-low`, `border-subtle`; hovered
  `surface-high` with a stronger border and a shadow. 12 above and below, 10 at the sides:
  - the pinned badge (22, the brand, a white pin, "Pinned") and the first 3 tags (24 tall, 8
    inside, 12 semibold; one in the filter on `base-lowest`), then "+N" naming the rest;
  - the title (20 semibold, 2 lines), muted until cards show unread;
  - the first message on one line: the author (14 semibold) and its text (14 medium, the
    message's markdown on one line), "Click to see attachment" and its kin without text,
    "Original message was deleted" once Discord says so, nothing while it loads;
  - the message count and the time: "12", "50+" for posts before 2022-07-01, "100k+"; "•"; the
    last activity (or the creation under Date Posted) as "5m ago", "3h ago", "2d ago",
    ">30d ago", then the date; its tooltip says when it was posted;
  - the first media at the right, 72 square, radius 8, 12 from the text; covered media (spoiler,
    sensitive, waiting for the scan) shows its placeholder and the eye.
- **Gallery** (24 from the sides, 16 apart): Discord's columns: up to 320 wide one 320 column,
  else `n = max(⌊w/450 + (w/320 − w/450)/2⌋, 1)` columns sharing the width. A card is
  `60 + ⌈(column − 26) × 2/3⌉ + 24 + 36` tall: the author and "Posted 5h ago", the title (16
  heavy, one line), up to four media (one, two, one beside two, a square; "+N" for more) or the
  message's start fading out, the tags over the media's bottom, and the count.
- **First messages**: a card that comes on screen without its post's first message asks for it
  and the next 4 of the active list (unfiltered); Akari sends them 10 a request, one request at a
  time, and asks a missing one again at most every 30 s while its card shows
  ([dispatches.md](../protocol/dispatches.md#forums)).
- *Akari:* no reactions on cards yet (backlog "Reactions on forum cards"), no typing faces, no
  NEW badge or "(N New)", and only attachments as a card's media (Discord also shows embeds'
  images).

## Loading and empty

- While older posts or a search load: placeholder cards (`base-low`, radius 12, 102 tall, 8
  apart, at 33%, 100% and 66% by turns, as many as fill the window below 200 of header, 118
  each); in the gallery, tiles for the window's rows.
- With no post to show and nothing loading: "Be the first to start the conversation!" / "What
  do you want to post about in #ideas?"; with a filter "There are no posts with this tag yet!"
  (or "these tags") / "Be the first to start a conversation with this tag" (or "these tags").
  *Akari: a symbol where Discord shows its illustration, and no "Create a post" yet.*

## The forum's row

The channel list's forum row shows, unless muted: the mention badge; else, when its unreads
resolve to All Messages, "N New" in the brand's color for new posts (active, not joined, newer
than the forum's ack, never opened); else the muted count of active posts with unread activity
past the ack. Counts above 999 read "1k+" to "9k+". Reading the page acks the forum at "now",
so its counts clear while it shows. *Akari: Discord's own row keeps "N NEW" until the user
leaves the forum (its count uses the ack from when the forum was selected); Akari's follows the
forum's current ack.*
