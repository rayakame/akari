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

## Conventions

- One file per topic, e.g. `layout.md`, `shortcuts.md`, `message-list.md`.
- Describe behavior in our own words, tables or our own diagrams.
- Never check in Discord assets: no screenshots, logos, icons, illustrations, sounds or
  the gg sans font. Akari uses platform icons (SF Symbols, Adwaita, Material) and
  system fonts.
