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
| Mark the channel read, or cancel an edit or reply | Escape | Jumps to the present (below) |
| Mark the server read | Shift+Escape | Later, with read state |
| Quick switcher | Cmd+K | Later |
| Previous or next channel | Option+Up / Option+Down | Later |
| Scroll the messages | Page Up / Page Down (**unverified**) | The list's native scrolling |
| Jump to the oldest unread message | Shift+Page Up (Shift+Fn+Up on a Mac laptop) (**unverified**) | Later, with read state |
| Focus the composer | Tab (**unverified**) | Later |

Rows marked **unverified** come from community lists, not from Discord's own pages. Escape and
Shift+Escape were confirmed in the web client on a Mac on 2026-10-15.

## Notes

- **Up arrow.** Discord's blog describes Up arrow in an empty composer. Its support article
  lists Shift+Up (Option+Up on macOS) for the same action. Akari follows the blog; the key
  does nothing until editing exists, and never beeps.
- **Escape.** In Discord, Escape marks the open channel as read and cancels an edit or a
  reply in progress. Akari has no read state, edits or replies yet, so Escape jumps to the
  present: it loads the newest messages if the list shows older ones, and scrolls to the
  bottom. It does this from the message list and from the composer, through one action, so
  the read-state milestone adds marking as read in one place.
- **Escape marks read.** In the web client, Escape jumps to the present and marks the channel
  read, also when the list is scrolled up; with nothing unread it sends nothing.
  Shift+Escape marks the whole server read, and does nothing at Home
  ([read-states.md](../protocol/read-states.md#acks-in-the-official-client)).
- **Shift+Page Up** is still **unverified**: Discord's shortcut list (Cmd+/) didn't open in
  the browser or the desktop app, and the Mac help article is images only. Community lists
  give the same keys.
- **No shortcut for "jump to present"** is documented for the official client; its bar at
  the bottom of the list is clicked ([message-list.md](message-list.md#jump-to-present)).
