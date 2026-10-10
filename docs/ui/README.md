# UI reference

Reference notes on how the official Discord client looks and behaves, so every Akari
app can mirror it and Discord users can switch with zero relearning.

## What goes here

- Layout: server rail, channel list, message area, member list, user panel, and how
  they resize, collapse and scroll
- Interaction patterns: unread and mention indicators, replies, threads, reactions,
  embeds, attachments, typing indicators, jump to unread, message grouping by author
  and time, hover actions, context menus
- Keyboard shortcuts and their per-platform mapping (Cmd on macOS, Ctrl elsewhere)
- Theme colors for the dark and light themes
- Places where a platform convention wins over Discord's behavior, and why

## Files

- [layout.md](layout.md): how the window is divided, the size of each part, and the connection
  bar
- [server-list.md](server-list.md): the server rail, its order, icons, pill and initials
- [channel-list.md](channel-list.md): categories, channel rows, and the DM list at home
- [message-list.md](message-list.md): message layout, the grouping rule, times, day dividers,
  system messages, loading history, jump to present, edits and deletes
- [theme.md](theme.md): the dark and light color tokens
- [composer.md](composer.md): the composer, sending, failed messages, slowmode and the length
  limit
- [shortcuts.md](shortcuts.md): keyboard shortcuts and which ones Akari has
- [login.md](login.md): the login screen

## Conventions

- One file per topic, e.g. `layout.md`, `shortcuts.md`, `message-list.md`.
- Describe behavior in our own words, tables or our own diagrams.
- Never check in Discord assets: no screenshots, logos, icons, illustrations, sounds or
  the gg sans font. Akari uses platform icons (SF Symbols, Adwaita, Material) and
  system fonts.
