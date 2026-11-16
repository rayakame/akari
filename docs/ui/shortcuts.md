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
| Edit the last message | Up arrow in an empty composer | Done: the newest loaded message the user may edit ([edit-delete.md](edit-delete.md#starting-an-edit)) |
| Save an edit, a new line in it, cancel it | Return, Shift+Return, Escape in the editor | Done; Option+Return adds a line too ([edit-delete.md](edit-delete.md#keys)) |
| Mark the channel read, or cancel an edit or reply | Escape | Done: ends an open edit first, then a pending reply, the pane's own before the other pane's; else marks the channel read and jumps to the present (below) |
| Mark the server read | Shift+Escape | Done |
| Mark unread from a message | Alt+click on it | Done: Option-click anywhere in the message's box but on its buttons ([message-actions.md](message-actions.md#mark-unread)) |
| The longer hover bar | Shift held over a message | Done: Copy Link and Mark Unread come first; the bar changes while the pointer rests |
| An action without its question, a reply without its ping | Shift+click on Delete, Pin, Unpin, Reply | Done, in the hover bar and the menu, and the pins popout's ✕ ([replies.md](replies.md#starting-a-reply), [edit-delete.md](edit-delete.md#deleting), [pins.md](pins.md#pinning-and-unpinning)). Shift counts as it is at the click |
| Toggle the pins popout | Cmd+P | Done: View > Show Pinned Messages (Hide while open), wherever the header has the pins button; File > Print is gone ([pins.md](pins.md#opening-and-closing)) |
| Jump to a pinned message and keep the popout open | Shift+click on Jump | Done ([pins.md](pins.md#the-popout)) |
| A focused message's keys: E edit, R reply, P pin, Backspace delete, + react, F forward, T thread, Cmd+C copy text, Alt+Enter mark unread | on a message with keyboard focus (Tab or the arrows) | Later: Akari's messages take no keyboard focus yet |
| Quick switcher | Cmd+K | Later |
| Previous or next channel | Option+Up / Option+Down | Later |
| Scroll the messages | Page Up / Page Down (**unverified**) | The list's native scrolling |
| Jump to the oldest unread message | Shift+Page Up (Shift+Fn+Up on a Mac laptop) (**unverified**) | Done: jumps like a click on the unread bar (below) |
| Focus the composer | Tab (**unverified**) | Later |
| Create a thread | Cmd+Shift+Option+N | Done: File > Create Thread, the open channel's New Thread form or the open thread's parent's, disabled where there's none ([thread-create.md](thread-create.md)) |
| Start a new post | Shift+Enter in a forum's search field | Done: opens the form, with what the field held as its title; Enter in the title goes on to the message, Escape closes the form and keeps what was typed ([forum.md](forum.md#creating-a-post)) |
| Open a post alone | Shift+click on its card | Done: full width; a plain click opens it beside the forum |
| Enter or exit full screen | Ctrl+Cmd+F, Fn+F, the green button | Done: View > Enter Full Screen (SwiftUI's View menu had none); with the Threads browser open it goes full screen and closes the browser |
| The media viewer's next or previous item | Right, Down or Space / Left or Up | Done; both wrap ([viewer.md](viewer.md)) |
| Close the media viewer | Escape | Done; Cmd+W too. The app's other shortcuts are off while it shows, but Cmd+Q and Cmd+H |
| The emoji picker's selection, a pick, a pick that keeps it open, closing it | the arrows, Return, Shift+Return, Escape in the picker | Done, from its search field ([emoji-picker.md](emoji-picker.md#keys-and-the-pointer)); Escape closes it also with a search typed |

Rows marked **unverified** come from community lists, not from Discord's own pages. Escape and
Shift+Escape were confirmed in the web client on a Mac on 2026-10-15.

## Notes

- **Up arrow.** Discord's blog describes Up arrow in an empty composer. Its support article
  lists Shift+Up (Option+Up on macOS) for the same action. Akari follows the blog and the code
  read (canary 671549): the key edits the newest loaded message the user may edit, and with
  none does nothing and never beeps.
- **Escape.** In Discord, Escape marks the open channel as read and cancels an edit or a
  reply in progress; it also jumps to the present from a list scrolled up, and with nothing
  unread it sends nothing. With an edit open, Escape from the composer or the list ends it and
  does nothing else; in the editor itself Escape cancels it. With a reply pending, Escape cancels
  it and does nothing else: nothing is marked read, nothing scrolls and what was typed stays. In
  the split view the pane the key was pressed in goes first, whole: its edit, its reply, then the
  other pane's edit and reply ([replies.md](replies.md#escape)). Otherwise Escape marks the channel read
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
- **Escape with something open.** An edit, a pending reply and the New Thread form take Escape
  first, as above. The emoji picker, the media viewer, the delete, pin and unpin questions, the
  pins popout and the other panels have the keyboard while they show, and Escape closes them and
  marks nothing read, as in the official client's handler.
- **Shift+Page Up** is still **unverified**: Discord's shortcut list (Cmd+/) didn't open in
  the browser or the desktop app, and the Mac help article is images only. Community lists
  give the same keys. In Akari it jumps to the first unread message, as a click on the unread
  bar does ([message-list.md](message-list.md#the-new-messages-line-and-the-unread-bar)), from
  the message list, a message's selected text and the composer, whenever the list has a new
  messages line, and does nothing otherwise. In the composer it never extends the selection.
- **A message's keys.** From the code read of canary 671549 (the row's key handler), not
  observed: they act on the message with keyboard focus, never on the hovered one, and the
  official client's list says so ("These shortcuts work when focused on a message."). Akari
  binds none of them to the hovered message.
- **No shortcut for "jump to present"** is documented for the official client; its bar at
  the bottom of the list is clicked ([message-list.md](message-list.md#jump-to-present)).
