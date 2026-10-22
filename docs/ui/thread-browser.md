# The Threads browser

What a text or announcement channel's threads look like when the user asks for all of them:
the popout under the channel header's Threads button, the modal "See all threads" opens, and
the channel list's "More Active Threads". Read from Discord's web client (canary 646904,
2026-10-22) and observed in the official client on 2026-10-22.

## Where it opens

- **The header's Threads button** is the first button of a text or announcement channel's
  header, and of a thread's full view, where it browses the thread's parent. Forums, voice
  channels, DMs and the split view have none. It shows "Threads" as its tooltip and stays
  selected while its popout is open; clicking it again closes the popout.
- **The popout** hangs under the button: its right edge on the button's, its top just under the
  header's line. It's 35% of the window wide, at least 480 and at most 600, and as tall as its
  list, at least 400 and at most 80% of the window (observed: 480 × 651 in an 813-point
  window). Radius 8, Discord's `shadow-border` ring and `shadow-high`. A click outside it (which
  still reaches what it hit), Escape, the button or a row closes it; so does opening another
  channel.
- **The modal**: "threads" in a "started a thread" line ("See all threads.") and "See All" in
  the channel list's popout open the same browser centered over the window, the page dimmed
  (black at about 72%). It's 680 wide (less 48 than the window when that's narrower) and as tall
  as its list, but it stops as far from the window's top and bottom as Discord's top bar plus 24
  (56; observed 680 × 700 in an 812-point window). 8 above the browser and 16 below it, a 1
  border in `border-subtle`, radius 12, `shadow-high`, and an X at the header's end. The X,
  Escape, a click on the dimmed page or a row closes it.

## The header

48 tall with its line (`border-subtle`) as its last point, 16 in from each side: the thread
symbol in a 24 box, 8, "Threads" (16 semibold), 16, a 1 × 24 divider, 16, then the search field
"Search for Thread Name", focused when the browser opens. The divider and the field only show
when the user can read the channel's history. Discord's field and the spacer after it share the
room left (measured: 236 in the popout, 315 in the modal, beside Discord's Create button), so
its width is half of the room plus 107.5. Discord's "Create" button and the modal's X follow;
*Akari has no Create button until it can create threads (the next PR).*

## The list

One list with up to three sections, each left out with its label when it has no thread:

1. **"{n} joined threads"**: the active threads the user joined. These include joined threads
   the channel list no longer shows (read, past their "Hide After Inactivity").
2. **"{n} other active threads"**: the active threads the user hasn't joined.
3. **"older threads"**: archived threads, loaded 25 at a time (below).

Both active sections and the older threads are ordered by their newest message, newest first
(a thread without one by its own ID), and follow new messages live: a thread whose newest message
changes moves while the browser is open (observed). A private thread shows only to its members
and to people with Manage Threads. The labels are Discord's words, also for one thread ("1
joined threads"), 14 medium, not uppercase (observed), in `text-default`.

Each label sits 8 below what's above it and 8 above its first card; the first label 8 below the
header (its box 8–32, the first card at 40). Cards are 72 tall with 8 below each, 16 in from
the list's sides: fill `background-surface-higher`, a 1 border in `border-subtle`, radius 8,
padding 15; hovered, `background-surface-highest` and `border-normal`. Rows carry no unread dot
or mention badge.

A card has the thread's name (16 semibold) and a second line (14):

- an active thread with a known newest message: its author's face (16) and 8, the author's
  name (semibold, at most 150 wide), ": ", the message on one line as the thread box shows it
  (formatted, mentions inert), " • " and the time ("Ray: test • 6h ago");
- an active thread without one: "No recent messages • 1d ago", the time from its newest
  message, else its creation;
- an archived thread: the owner's face, "Started by", 4, the owner's name (semibold), a bullet
  in `background-mod-muted` with 8 each side, "Last active 3d ago" (its newest message, else
  when it was archived); with an unknown owner, a 16 circle with the thread symbol and only
  "Last active …".

Times use the thread box's words ("1m ago" … "29d ago", ">30d ago", then the date). On the
right, 16 after the text: the faces of Discord's member preview, 24 each at a 16 step, each cut
where the next lies, then "+N" for the members not shown, or "50+" from 50 members; nothing
without a preview. *Akari draws initials faces; an unknown member is a plain colored circle.*

**Clicking a row** closes the browser and opens the thread full width, closing its channel's
split, as a thread's row in the channel list does. **Shift+click** opens it in the split view
beside its channel, going there first (Discord does so when "Split View for Threads" is on;
Akari always). Right-click shows the thread's menu (Mark As Read, Join or Leave Thread).

## Older threads

The first time a channel's browser opens in a connection (with Read Message History), Discord's
client asks `threads/search` for its archived threads, 25 at a time
([dispatches.md](../protocol/dispatches.md#threads)). The next page loads when the list is
scrolled within 200 of its bottom while Discord has more. Pages stay for the connection, so
reopening asks nothing. The section shows every archived thread of the channel the client knows,
down to the oldest one the pages brought (all once there are no more): a thread archived after
the page loaded is there when the browser opens again, without a reload (observed). A failed page,
or Discord's "not indexed yet", leaves the section as it is until the next connection.

While all three sections are empty and the first page loads, a spinner shows (64 above and
below). With nothing to show: **"There are no threads."** (20 semibold) and "Stay focused on a
conversation with a thread - a temporary text channel." (16), centered with 32 around, at least
400 tall. *Discord shows its illustration above them and a "Create Thread" button below; Akari
shows a large thread symbol and no button.*

## Search

Enter searches the field's text: Discord's first 25 matches by name, active and archived, in its
order, as one list without labels (16 above, 8 below). Without matches, the empty state.
Editing the text keeps the results until the next Enter; clearing the field brings the sections
back. A search that fails, or that Discord can't answer yet, changes nothing.

## More Active Threads

Resting the pointer on a text or announcement channel's row for 200 ms, when the channel has
active threads the list doesn't show (not joined, or joined but past their window), opens a
popout 17 to the right of the row, its top level with the row's: "More Active Threads" (14
medium, `text-muted`; 8 above, 8 to the sides, 2 below), up to three of those threads, the
newest message first, each with its owner's face (16), its name, a bullet and its time in
`text-muted`, and "See All" with the thread symbol, which opens the modal. Rows are 8 + 18 + 8
tall, 2 apart, radius 4, hovered in `background-mod-normal` with `text-strong`. The popout is
8 around its content, 272 to 400 wide, radius 12, `surface-high` with a `border-subtle` ring and
`shadow-high`. A row opens its thread as the browser's do (Shift: beside). It goes 250 ms after
the pointer leaves the row and the popout; a click on the row closes it at once.
*Akari keeps it open while the pointer is on the popout; whether Discord's does is to check.*
