# Keyboard shortcuts

Discord's shortcuts that Akari mirrors or plans to mirror, with the macOS key. Sources:
Discord's keyboard shortcut support article
(https://support.discord.com/hc/en-us/articles/31232432266647), its keyboard navigation
article (https://support.discord.com/hc/en-us/articles/1500000056121) and its blog post on
shortcuts (https://discord.com/blog/how-to-use-keyboard-shortcuts-on-discord-create-custom-keybinds).

| What | Discord (macOS) | Akari |
|---|---|---|
| Send | Return | Done |
| New line | Shift+Return | Done; Option+Return too |
| Edit the last message | Up arrow in an empty composer | Reserved: the key is taken, editing comes later |
| Mark the channel read, or cancel an edit or reply | Escape | Marks the channel read and jumps to the present (below) |
| Mark the server read | Shift+Escape | Done |
| Quick switcher | Cmd+K | Later |
| Previous or next channel | Option+Up / Option+Down | Later |
| Scroll the messages | Page Up / Page Down (**unverified**) | The list's native scrolling |
| Jump to the oldest unread message | Shift+Page Up (Shift+Fn+Up on a Mac laptop) (**unverified**) | Done: jumps like a click on the unread bar (below) |
| Focus the composer | Tab (**unverified**) | Later |
| Create a thread | Cmd+Shift+Option+N | Done: File > Create Thread, the open channel's New Thread form or the open thread's parent's, disabled where there's none ([thread-create.md](thread-create.md)) |
| Enter or exit full screen | Ctrl+Cmd+F, Fn+F, the green button | Done: View > Enter Full Screen (SwiftUI's View menu had none); with the Threads browser open it goes full screen and closes the browser |
| The media viewer's next or previous item | Right, Down or Space / Left or Up | Done; both wrap ([viewer.md](viewer.md)) |
| Close the media viewer | Escape | Done; Cmd+W too. The app's other shortcuts are off while it shows, but Cmd+Q and Cmd+H |

Rows marked **unverified** come from community lists, not from Discord's own pages. Escape and
Shift+Escape were confirmed in the web client on a Mac on 2026-10-15.

## Notes

- **Up arrow.** Discord's blog describes Up arrow in an empty composer. Its support article
  lists Shift+Up (Option+Up on macOS) for the same action. Akari follows the blog; the key
  does nothing until editing exists, and never beeps.
- **Escape.** In Discord, Escape marks the open channel as read and cancels an edit or a
  reply in progress; it also jumps to the present from a list scrolled up, and with nothing
  unread it sends nothing. Akari has no edits or replies yet, so Escape marks the channel read
  and jumps to the present: it loads the newest messages if the list shows older ones, and
  scrolls to the bottom; a jump to the first unread message that is still loading doesn't land
  ([message-list.md](message-list.md#the-new-messages-line-and-the-unread-bar)). It does this
  from the message list and from the composer, through one action. In the split view, Escape in
  either pane does it for both: both lists are marked read, end their new messages lines and
  jump to the present, as in the official client (observed 2026-10-17), and the pane's own
  composer takes the focus. Discord's key handler (canary 646574, module 92960, read
  2026-10-22) first closes an open layer, then cancels an edit, a reply, a scheduled message or a
  sticker preview (the channel's before the thread's), closes a create-thread form, and then runs
  both lists' part: jump if newer messages aren't loaded, ack if unread, and clear the divider
  either way. When neither list jumped or acked and a split is open it closes the split, also
  with a list scrolled up or a divider just cleared, and scrolls nothing; otherwise every list
  scrolls to its bottom. Akari follows that order: with nothing newer to load and nothing
  unread in either list, one Escape ends both new messages lines and closes the split.
  With the New Thread form open, Escape closes it first, from anywhere, and does nothing else:
  the form's own step comes before the lists' part in Discord's handler (canary 649224), and what
  was typed stays ([thread-create.md](thread-create.md#closing)).
  Shift+Escape, from the same places, marks the open server read, and does nothing at Home
  ([read-states.md](../protocol/read-states.md#acks-in-the-official-client)).
- **Only the Escape key.** macOS sends Cmd+. as the same cancel command as Escape; in Akari it
  marks nothing read, and Cmd+Shift+. doesn't mark the server read. In the composer it does
  nothing, as in the official client (**unverified**); in the message list and a message's
  text AppKit handles it as usual.
- **Escape with something open.** Once Akari has replies, edits and popups, Escape closes the
  open one first and doesn't also mark the channel read, as in the official client
  (**unverified**).
- **Shift+Page Up** is still **unverified**: Discord's shortcut list (Cmd+/) didn't open in
  the browser or the desktop app, and the Mac help article is images only. Community lists
  give the same keys. In Akari it jumps to the first unread message, as a click on the unread
  bar does ([message-list.md](message-list.md#the-new-messages-line-and-the-unread-bar)), from
  the message list, a message's selected text and the composer, whenever the list has a new
  messages line, and does nothing otherwise. In the composer it never extends the selection.
- **No shortcut for "jump to present"** is documented for the official client; its bar at
  the bottom of the list is clicked ([message-list.md](message-list.md#jump-to-present)).
