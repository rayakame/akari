# The forum page

What a forum channel shows when the user opens it: its posts as cards in the message list's
place, with search, the tag filter and Sort & View. Read from Discord's web client (canary
653948, 2026-10-28; modules in brackets), the list's spacing as Chris measured the live client
against Akari (2026-10-31), the NEW badge, posts beside the forum, "(N New)", the form for a new
post and media channels from canary 671387 (2026-11-11 and 12). What rests on the client's code
alone, with no look at the live client's requests, says *(code read)*. Differences are marked
*Akari:*.

## Where it opens

- A forum row in the channel list opens the page; so do a forum's mention in a message and a
  link to it. The header shows the forum's symbol (two filled bubbles, as in the channel list, in
  the header icons' gray) and name, without its topic: the topic is the page's guidelines. A
  media channel's row opens the same page as a gallery ([below](#media-channels)).
- Opening the page refreshes its list and asks Discord for the first 10 posts' first messages
  (the official client does that as the row is pressed, module 473503). It subscribes the guild
  as opening any channel does, so the gateway's thread lists arrive.
- The page is read like a channel: while it shows, the window is key and the user is engaged,
  the forum is acked at "now" whenever a post is newer than its ack or a post has unread
  activity past it ([read-states.md](../protocol/read-states.md#forums)). An age-restricted
  forum shows the age gate first, as a channel does, and isn't read behind it.
- A card's click opens its post beside the page, in the split view
  ([layout.md](layout.md#split-view)); Shift+click opens it alone, full width. The forum keeps
  its post: coming back to the forum shows it again, also after a relaunch. The split's X,
  Escape with nothing left to mark, and a click on the forum's own row while it's open close it
  and the forum forgets it. The open post's card has the selected fill (`surface-higher`).
  *Akari: "Open in Full View" and the split's title open the post alone and the forum keeps it,
  as a channel keeps its thread; Discord forgets it there. Akari has no "Open threads in split
  view" setting: a click always opens beside.*

## The list [207777]

- **Active posts** are the forum's posts the store has (THREAD_LIST_SYNC, THREAD_CREATE):
  pinned first, then by last activity (the newest message, else the post's ID) under Recently
  Active, by creation under Date Posted.
- **Frozen between refreshes**: which posts show and their order change only when the list
  refreshes: opening the forum, a THREAD_LIST_SYNC of its guild (which follows every new
  connection), and the pill. A deleted or archived post leaves at once, a pin change re-sorts and
  brings a newly pinned post, the tag filter applies at once; new activity doesn't move a card.
  *Akari: a new connection keeps the list until that sync, where Discord rebuilds it at once
  from the joined posts.*
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
  while searching, a pencil while a closed form holds a draft), the field (20 semibold,
  "Search or create a post...", 100 characters at most, typed line breaks become spaces), with
  a topic the guidelines button ("Post Guidelines"), and "New Post" (the brand's small button
  with a bubble). Without the permission to post the field says "Search", "New Post" is off and
  the status row says "You do not have permission to post in this channel." while the field
  has the focus. The row turns into the form for a new post ([below](#creating-a-post)).
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

16 under the search row, 19 above the first card, 32 tall:

- **"Sort & View"** (a capsule, 12 and 8 inside, the secondary control's 12 % fill and a 1 pt
  `border-subtle`, as the live client draws it; canary 653948's CSS has `base-lowest` without a
  border): a menu with "Sort by" (Recently
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

- **List** (16 from the page's sides, 16 apart): radius 12, `base-low`, `border-subtle`; hovered
  `surface-high` with a stronger border and a shadow. 13 above and below (with the border), 10
  at the sides; 102 tall with one line of title and no tags: the title's ink 18 below the top,
  title, message and footer about 27 apart, the footer 14 above the bottom (the live client puts
  the 8 above the footer, where canary 653948's CSS has it below):
  - the pinned badge (22, the brand, a white pin, "Pinned") and the first 3 tags (24 tall, 8
    inside, 12 semibold; one in the filter on `base-lowest`), then "+N" naming the rest;
  - the title (20 semibold, 2 lines);
  - the first message on one line: the author (semibold) and its text (medium, the message's
    markdown on one line), *15 where Discord's is 14, which matches its ink beside SF*, "Click
    to see attachment" and its kin without text, "Original message was deleted" once Discord says
    so, nothing while it loads;
  - an **unread** post (active, with messages past the user's read position, whatever its mute
    or unread setting; Discord's `isForumPostUnread`) shows its title and author in
    `text-strong` and its message in `text-default`; a read or archived one all in
    `text-muted`;
  - a **new** post has the NEW badge after its title ([below](#the-new-badge));
  - the message count (a filled bubble) and the time: "12", "50+" for posts before 2022-07-01,
    "100k+"; "•"; the
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
- The message line is one line at the text's size (Akari's 15, as the author's) whatever the
  message holds: a message of
  only emoji isn't jumbo there (Discord parses every one-line preview with `formatInline`, which
  turns jumbo off; its emoji are 18 high), a heading isn't large. A custom emoji is its image,
  18 high ([emoji.md](emoji.md#custom-emoji)). *Akari: the gallery card's text is capped the
  same way and names its custom emoji (`:name:`), where Discord's, a box of several lines, can
  show jumbo emoji.*
- **"(N New)"** after the count, in the brand's text color (14 semibold): how many messages
  came since the user last read a post they had opened: "(3 New)", "(25+ New)" from 25, and
  "(1+ New)" while Discord hasn't said how many. Only for an active post that is unread, was
  opened before (it has a read state of its own) and has a read position; never on a NEW post.
  The number is the smaller of Discord's count and the post's message count
  ([read-states.md](../protocol/read-states.md#forums)). *Akari: reading the post anywhere
  sets its count to 0, so the next message says "(1 New)"; Discord's own count can keep its old
  number and drift up. With a message count of 0 Akari says "1+", Discord "0".*
- *Akari:* no reactions on cards yet (backlog "Reactions on forum cards"), no typing faces, and
  only attachments as a card's media (Discord also shows embeds' images).

### The NEW badge

Discord's `isNewForumThread` [218461, 767581; the cards' hook `X5` in 435470]. A card shows NEW
when all of this holds:

- the post is active (not archived);
- the user never opened it: it has no read state of its own (a mention in it leaves none).
  Discord's `_persisted`: set by any
  ack of the post (opening it in a focused window acks it at once, also with nothing unread;
  reading it on another device; the user's own message in it; a mark as read that includes it),
  by READY's read states, and for the user's own new post when it's created. Joining or
  following a post doesn't;
- it was made after the forum's read position **as it was when the forum was opened**
  (`ackMessageIdAtChannelSelect`: the forum's ack at the channel's selection; without an ack,
  the user's join to the server);
- it was made after the user joined the server.

Whether the user joined the post, its pin and its owner play no part, except that the user's
own post counts as opened. It is a different rule from the unread look: a card can be unread
without NEW (an old post never opened), and NEW posts are always among the never opened.

How it ends:

- **Opening the post**, here or on another device: at once. *Akari acks a never-opened post
  when it shows at the bottom, unread or not, as Discord does.*
- **Opening the forum the next time.** Showing the forum acks it at "now" right after the
  position was taken, so on the next visit nothing older than this visit is new: a post is NEW
  for one visit. Opening and closing posts beside the forum is the same visit; going to another
  channel and back is the next one.
- **A relaunch**: the position is kept in memory only, and taken again from the forum's ack,
  which the last visit moved. *Akari shows the forum from its disk cache before it's connected:
  it takes the position again with the session's first READY, whose ack counts.*
- Archiving the post.
- Marking the forum as read while it shows changes nothing (the position was taken before);
  marking the server as read acks its joined posts that are unread or mentioned, which ends
  theirs.

The badge [805251, CSS 963434, 223139]: "New" in capitals, 12 semibold, white on `#5966f2`
(`--badge-primary-background`, the same in both themes), a round pill 16 tall with 8 inside. In
the list it stands in the title's text, 8 after it, its text 2 above the title's baseline, and
wraps with it; when title and badge would need a third line, the title keeps its two lines and
the badge stands at its right, 4 from the top. In the gallery it ends the author's row, at its
top. *Akari: the same rule in the core (`Store::new_posts`), read with the cards' unread look.
Each time the app opens the forum's page is a visit. Until the next READY the forum's own ack
is the Mac's "now", where Discord's is its server's: with the Mac's clock ahead, a post made
right after a visit can miss its NEW on the next. The forum row's "N New" still follows the
forum's current ack ([below](#the-forums-row)).*

## Loading and empty

- While older posts or a search load: placeholder cards (`base-low`, radius 12, 102 tall, 16
  apart as the cards, at 33%, 100% and 66% by turns, as many as fill the window below 200 of
  header, 118 each); in the gallery, tiles for the window's rows.
- With no post to show and nothing loading: "Be the first to start the conversation!" / "What
  do you want to post about in #ideas?"; with a filter "There are no posts with this tag yet!"
  (or "these tags") / "Be the first to start a conversation with this tag" (or "these tags").
  Where the user may post, "Create a post" (a small secondary button) under the first of them
  opens the form. *Akari: a symbol where Discord shows its illustration.*

## A post

A post opens as a thread does, beside the forum or alone ([message-list.md](message-list.md#threads)),
with its own beginning and a bar under its first message *(code read)*:

- **The beginning**: the forum's symbol on a round `base-lower` disc, the post's name as the
  title, "Original message was deleted" under it once the start is loaded without the first
  message, and the post's tags as small pills on one row. The tags are those the user may
  apply: a moderated tag shows there only with Manage Threads on the forum, while cards show
  every tag to everyone.
- **The first message** is the post's first row; its ID is the post's.
- **The bar** under it (8 above a 1 pt `border-subtle` line, 6 above and below 32-high small
  secondary buttons at the right, 4 apart): "Follow" with a bell, or "Following" with a
  checkmark, which joins or leaves the post (`location` "Forum Toolbar"; tooltip "Add this post
  to your channel list and receive notifications about it."; off for an archived post; a full
  post says "Cannot join post" / "This post has reached the maximum possible number of
  members."), and "Copy Link" (a link symbol), which copies
  `https://discord.com/channels/{server}/{post}` and says "Copied" with a checkmark for 2 s.
  Once the start is loaded and the first message is gone, the bar leads the list.
- **Under the bar**: with replies, an 8-high `border-subtle` divider and 16 before the first
  reply, which starts a group. Without replies, for everyone who may send in the post, unless
  it's locked for them or they started it: a box (`background-mod-normal`, radius 12, 16 around,
  at most 448 wide, 12 inside) with "Start the conversation!" (16 semibold) and "Be the first
  to share what you think!" (14).
- *Akari: the bar scrolls with the list; Discord's sticks to the list's top and then shows
  "Jump to Top" (backlog). No reactions in the bar yet ("React to Post", the default reaction).
  A symbol stands where Discord's box has its animation, and the box has no tail.*
- Reading: the post beside the forum is read as a split thread is, the forum at "now" as before.
  *Akari never moves the forum's ack back to its newest post after a post was read, which
  Discord's code does about 3 s later (not observed on the wire).*

## Creating a post

Read from the client's code *(code read; the request wasn't observed)*.

- **Who**: Send Messages on the forum (Create Public Threads plays no part), and no timeout.
- **Opening the form**: "New Post"; Shift+Enter in the field; the empty state's "Create a
  post"; a click on the row while a closed form holds a draft. What the search field held
  becomes the title when the form has none, and the search ends.
- **The form**, in the header row's card:
  - a × ("Clear") in the magnifying glass's place: drops the title, tags and message and closes;
  - the title ("Title"; 100 characters, line breaks become spaces); Enter in a title goes on to
    the message;
  - the message ("Enter a message...", the channel composer's field, at least 80 high and at
    most half the page; it starts as the forum's template, which may be posted as it is);
  - the tags the user may apply as small pills that toggle (a tag symbol before them, 16 above,
    12 below, then a hairline), at most 5: with five chosen the others are off; a moderated tag
    only with Manage Threads; no row when there is none to apply;
  - the controls (40 high, 12 above), at the right: the forum's slowmode for new posts (a timer
    and "Slowmode is enabled.", or the countdown while it runs; tooltip "Slowmode is enabled.
    Members can create one post every 30 seconds."), the guidelines button, and "Post" (a
    spinner while the post is made). "Post" is on with a title and a message, outside a
    cooldown, and with a tag where the forum requires one; else "A tag is required to create a
    post in this channel" stands under the controls.
- **Escape** in the title or the message closes the form and keeps what was typed: the row
  shows a pencil, and a click on it opens the form again. A draft with a message opens the form
  with the page. Drafts stay per forum for the session.
- **Errors**: "A title is required" and "A message is required" under their fields (typing
  ends them), AutoMod's sentence under the title or the message it blocked, "Cannot create
  post" / "This server has reached the maximum number of active posts. Free up space by
  archiving a post.", "Cannot create post" / "You are being rate limited.", else "Error" / "An
  error occurred, please try again."; a slowmode refusal starts the countdown. The form stays
  open with everything typed.
- **After**: the forum's list refreshes with the new post, the post opens beside the forum if
  the forum is still open, and the form closes empty (the message back to the template). The
  user's own post is never NEW and counts as read for the forum.
- The request and its answers: [messages.md](../protocol/messages.md).
- *Akari: the tags wrap onto more rows where Discord has "See more tags" and its tag search; no
  Preview mode, no emoji and sticker buttons and no "Add Media" (uploads come with message
  features); drafts don't outlive the session (Discord's do); the guidelines don't open by
  themselves with the form. An unconfirmed create says "Post may exist" (Akari's words).*

## Media channels

A media channel (type 16) is a forum whose page is a gallery *(code read)*:

- Its row opens the page; the gallery is the only view, whatever the channel's default: the
  button reads "Sort" and its menu has no "View as".
- Tiles are 16:9 (a card is `60 + ⌈(column − 26) × 9/16⌉ + 24 + 36` tall). Each image shows
  whole, with Discord's high shadow and square corners, over a cover of itself under
  `rgba(28, 27, 37, 0.9)`. *Akari: the cover is the image's blurred placeholder, where Discord
  blurs a 1.5× copy of the image by 4.*
- An attachment flagged as the post's thumbnail (flag 2) is the tile alone, and isn't among
  the first message's attachments in the post. Without one, up to four media form the mosaic;
  a post without media shows its text as a forum's gallery card.
- Posts open, are followed and read as a forum's. A channel that hides download options hides
  them in its posts ([viewer.md](viewer.md), [attachments.md](attachments.md)).
- *Akari: "New Post" is off there, with the tooltip "Creating media posts needs uploads, which
  Akari doesn't have yet." Discord's form has a thumbnail picker and "Upload your media"; its
  client has no rule or sentence for a media post without media.*

## The forum's row

The channel list's forum row shows, unless muted: the mention badge; else, when its unreads
resolve to All Messages, "N New" in the brand's color for new posts (active, not joined, newer
than the forum's ack, never opened); else the muted count of active posts with unread activity
past the ack. Counts above 999 read "1k+" to "9k+". Reading the page acks the forum at "now",
so its counts clear while it shows. *Akari: Discord's own row keeps "N NEW" until the user
leaves the forum (its count uses the ack from when the forum was selected); Akari's follows the
forum's current ack.*
