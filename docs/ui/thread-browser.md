# The Threads browser

What a text or announcement channel's threads look like when the user asks for all of them:
the popout under the channel header's Threads button, the modal "See all threads" opens, and
the channel list's "More Active Threads". Read from Discord's web client (canary 646904,
2026-10-22; buttons and the click trap from canary 648857, 2026-10-23) and observed in the
official client on 2026-10-22 and 2026-10-23.

## Where it opens

- **The header's Threads button** is the first button of a text or announcement channel's
  header, and of a thread's full view, where it browses the thread's parent. Forums, voice
  channels, DMs and the split view have none. It shows "Threads" as its tooltip and stays
  selected while its popout is open; clicking it again closes the popout.
- **The popout** hangs under the button: its right edge on the button's, its top just under the
  header's line. It's 35% of the window wide, at least 480 and at most 600, and as tall as its
  list, at least 400 and at most 80% of the window (observed: 480 × 651 in an 813-point
  window). Radius 8, Discord's `shadow-border` ring and `shadow-high` (*Akari: the window's
shadow*). A click outside it, Escape,
  the button or a row closes it; so does opening another channel. Discord's Threads popout has a
  click trap: a layer over the whole window under it takes every press outside it, the button's
  too, and the popout closes when that press is released, whichever button; nothing behind gets
  the click, the hover or the wheel (observed 2026-10-23: the official client only closes it).
- **The modal**: "threads" in a "started a thread" line ("See all threads.") and "See All" in
  the channel list's popout open the same browser centered over the window, the page dimmed
  (black at about 72%). It's 680 wide (less 48 than the window when that's narrower) and as tall
  as its list, but it stops as far from the window's top and bottom as Discord's top bar plus 24
  (56; observed 680 × 700 in an 812-point window). 8 above the browser and 16 below it, a 1
  border in `border-subtle`, radius 12, `shadow-high`, and an X at the header's end. The X,
  Escape, a click on the dimmed page (press and release on it, the left button) or a row closes
  it. The dimmed page covers the whole window: nothing behind reacts to the pointer.
- **Nothing behind either reacts**: no hover, no cursor change, no click, no wheel, no focus.
  *Akari shows both as windows of their own, attached to the session's window so they move with
  it (`ChildWindows`): the browser's borderless panel over a shield, a borderless window over the
  whole session window (the title bar too) that's clear for the popout and the dimmed page for
  the modal. Every mouse event outside the browser goes to the shield, the window server sends
  nothing to the covered window, and a press on the shield closes the browser and goes no further,
  whichever button; the shield shows the arrow and swallows the wheel. Both follow the window's
  size; entering or leaving full screen, minimizing or closing the window, and the app going to
  the background close the browser, as do Escape, the X, a row, and another channel or server.
  This departs from Discord in two details (D126): both close on the press, any button (Discord's
  popout on the release, its modal on a left click), and the window's buttons and title bar are
  under the shield while either shows.*
- **Focus**: both are modal dialogs ("Threads") with a focus lock, and the search field takes the
  focus when they open; Tab and Shift+Tab move only among what's inside. Closing gives the focus
  back to what had it when it opened, if nothing else took it meanwhile; Discord's typing then
  goes to the composer. *Akari's panel is the key window while it shows (the session's stays the
  main one, its title bar active), so typing, Tab and Shift+Tab stay in it and nothing reaches
  the composer; Escape closes it unless an input method composes, the menu's Close (Cmd+W) closes
  it; the Window menu's other commands (Fill, Center, Move & Resize, Zoom, Minimize, full
  screen) act on the session's window with the browser open: the panel hands them on, and the
  browser follows the window, or closes when it's minimized or goes full screen. On closing, the session's window takes
  the key back and the channel's composer the focus (a Shift+click's split keeps its own). The
  channel stays read behind it: the panel's key, clicks and keys count as the window's. VoiceOver
  meets a dialog named "Threads" and starts in the field, or at the title without one.*

## The header

48 tall with its line (`border-subtle`) as its last point, 16 in from each side: the thread
symbol in a 24 box, 8, "Threads" (16 semibold), 16, a 1 × 24 divider, 16, then the search field
"Search for Thread Name", focused when the browser opens. The divider and the field only show
when the user can read the channel's history. Discord's field and the spacer after it share the
room left (measured: 236 in the popout, 315 in the modal, beside Discord's Create button), so
its width is half of the room plus 107.5. Discord's "Create" button follows, 8 after the
spacer, where the user may create a public or a private thread: its small primary button (32
tall, at least 60 wide, 12 inside, radius 8, 14 medium white on the brand color, darker while
hovered and pressed); it closes the browser and opens the New Thread form
([thread-create.md](thread-create.md)). Then the modal's X.

The field is AppKit's own text field in the key panel: the I-beam over its input, editing,
selection and the input method are the system's. Its magnifier is `icon-strong` (#FBFBFB dark,
#28282D light); its clear button, while there's text, is `icon-subtle` (#ABACB2 dark, #595A63
light) and `icon-strong` while hovered, with the pointing hand. The modal's X is Discord's design-system
icon-only button (size sm): a 32 square (30 and a 1 border) with radius 8, 8 after the spacer;
idle a 16 glyph in `control-icon-only-icon-default` (#ABACB2, light #595A63) on nothing; hovered
a fill of rgba(151, 151, 159, 0.161) (#36363C on the modal's #242429; Chris's screenshot
#36363B), a 1 border of 0.039 (light 0.2) and the glyph `icon-strong` (#FBFBFB, light #28282D);
pressed a fill of 0.2 (light 0.239); the pointing hand. The header's and the split's icon buttons
(Threads, More, the split's X) change their glyph's color only (`icon-muted` #96979E, hovered
`icon-subtle`, selected `icon-strong`) and show the pointing hand.

## The list

One list with up to three sections, each left out with its label when it has no thread:

1. **"{n} joined threads"**: the active threads the user joined. These include joined threads
   the channel list no longer shows (read, past their "Hide After Inactivity").
2. **"{n} other active threads"**: the active threads the user hasn't joined.
3. **"older threads"**: archived threads, loaded 25 at a time (below).

Discord's client knows a connection's active threads from that connection. *Akari's store also
keeps threads across a reconnect that the new connection didn't list (and the cache's); they
stay out of the sections until the gateway, a page or a search brings them again, since they
may have been archived or left meanwhile
([dispatches.md](../protocol/dispatches.md#threads)).*

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
without a preview. *Akari draws the members' avatars ([images.md](images.md)); a member the
store doesn't know yet shows the loading disc.*

**Clicking a row** closes the browser and opens the thread full width, closing its channel's
split, as a thread's row in the channel list does. **Shift+click** opens it in the split view
beside its channel, going there first (Discord does so when "Split View for Threads" is on;
Akari always). Right-click shows the thread's menu (Mark As Read, Join or Leave Thread).

## Older threads

The first time a channel's browser opens in a connection (with Read Message History), Discord's
client asks `threads/search` for its archived threads, 25 at a time
([dispatches.md](../protocol/dispatches.md#threads)). The next page loads when the list is
scrolled within 200 of its bottom while Discord has more. Pages stay for the connection, so
reopening asks nothing. Paging by offset can skip a thread that's unarchived between two pages;
the official client pages the same way. *Akari: a browser open when a new connection forgets the pages asks
again; one opened while another's page loads follows that page.* The section shows every archived thread of the channel the client knows,
down to the oldest one the pages brought (all once there are no more): a thread archived after
the page loaded is there when the browser opens again, without a reload (observed). A failed page,
or Discord's "not indexed yet", leaves the section as it is until the next connection.

While all three sections are empty and the first page loads, a spinner shows (64 above and below).
With nothing to show: **"There are no threads."** (20 semibold) and "Stay focused on a conversation
with a thread - a temporary text channel." (16), centered with 32 around, at least 400 tall.
*Akari's fits a smaller room, so the header stays in a 500-point window.* Where the user may create
a thread, a "Create Thread" button follows 24 below (the medium primary button: 40 tall, at least
100 wide, 16 inside, 16 medium), which opens the form like Create. *Discord shows its illustration
above them; Akari shows a large thread symbol.*

## Search

Enter searches the field's text: Discord's first 25 matches by name, active and archived, in its
order, as one list without labels (16 above, 8 below). Without matches, the empty state.
Editing the text keeps the results until the next Enter; clearing the field brings the sections
back. A search that fails, or that Discord can't answer yet, changes nothing. Discord ignores
Enter while a search runs. *Akari searches the newer text once the running search is answered
and shows only that answer.*

## More Active Threads

Resting the pointer on a text or announcement channel's row for 200 ms, when the channel has
active threads the list doesn't show (not joined, or joined but past their window), opens a
popout 17 to the right of the row, its top level with the row's (Discord's default alignment
for that side), moved up to stay 12 inside the window's bottom but never above 12 (its position
layer, module 623646): "More Active Threads" (14
medium, `text-muted`; 8 above, 8 to the sides, 2 below), up to three of those threads, the
newest message first, each with its owner's face (16), its name, a bullet and its time in
`text-muted`, and "See All" with the thread symbol, which opens the modal. Rows are 8 + 18 + 8
tall, 2 apart, radius 4, hovered in `background-mod-normal` with `text-strong`. The popout is
8 around its content, 272 to 400 wide, radius 12, `surface-high` with a `border-subtle` ring and
`shadow-high`. A row opens its thread as the browser's do (Shift: beside). Its threads and times
follow new messages while it shows. It goes 250 ms after the pointer leaves the row and the
popout (moving to another row closes it unless that row shows its own); a click on the row
closes it at once, and it goes with its row (another server, a collapsed category). It's a plain
group, not a dialog: no focus lock, nothing focused.
*Akari keeps it open while the pointer is on the popout; whether Discord's does is to check. It's
a borderless panel of its own beside the row, attached to the session's window, without a shield
and never key (the list keeps its hover and the composer its focus); it takes its first click,
shows the pointing hand over its rows and "See All" (from its own tracking, as a window that's
never key gets no cursor rects), and goes with the app going to the background or the window
closing, too.*
