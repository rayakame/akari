# akari-core

The Discord client without a UI: the wire models and gateway decoding, `DiscordClient` with
both logins, the gateway connection, the state store (`akari_core::state`), the disk cache
(`akari_core::cache`) and `Account`, which keeps the store current, loads history and sends
messages. Hosts reach it through `akari-ffi` (Swift, Kotlin) or link it directly (Linux, the
CLI). How Discord's side behaves is in `docs/protocol`; how messages load is in
`docs/protocol/messages.md`.

## Before messages enter a window

`Account::set_message_preparer` registers a `MessagePreparer`: the host's work on messages
before they enter a channel's window, so that its reads find it done. akari-ffi parses each
message's markdown there, so pages are never parsed on the main thread.

- **When:** for every history page `load_messages` brings, and for the disk cache's copy of a
  channel that a first page shows before the network answers. Never for gateway events
  (new, edited or confirmed messages); a host handles those when it reads them.
- **What it gets:** the messages read-only (`&[Arc<Message>]`); it can't change them or the
  store. A page's messages are a copy converted for it; the window's own are converted under
  the store's lock, with edits that arrived during the load applied.
- **Where it runs:** outside the store's lock, on a blocking thread that its own load awaits:
  for a page after its request (and rate-limit permit) is done, for the cache's fill while the
  request runs. A slow preparer holds up only that load: other loads, the gateway and reads go
  on. The store drops a page whose window moved
  meanwhile, so loads that finish out of order stay safe. A preparer that panics prepares
  nothing, and the load goes on.
- **What it reports:** how many messages it prepared. `LoadReport.prepare` and `prepared`
  carry its time and count, including the cache's fill on a first page, and the "history
  page loaded" log line shows them as `prepare_ms` and `prepared`.
