# Gateway

How akari-core talks to the gateway: `akari_core::gateway::decode` reads single messages,
and `Gateway`, created from a `DiscordClient`, runs the connection.

Sources: [Using the Gateway](https://docs.discord.food/gateway/using-gateway),
[Gateway Events](https://docs.discord.food/gateway/gateway-events) and
[Opcodes and Close Codes](https://docs.discord.food/gateway/opcodes-and-close-codes). The
older `/topics/gateway…` URLs redirect there.

## Payload envelope

Every gateway message is `{op, d, s, t}`
([payload structure](https://docs.discord.food/gateway/gateway-events#gateway-payload-structure)):

| Field | Type | Notes |
|---|---|---|
| `op` | integer | Opcode |
| `d` | ?JSON value | Event data |
| `s` | ?integer | Sequence number; `null` unless `op` is 0 |
| `t` | ?string | Event name such as `READY`; `null` unless `op` is 0 |

Clients keep the last non-null `s` for heartbeats and resuming
([dispatch events](https://docs.discord.food/gateway/using-gateway#dispatch-events)).

`decode` parses the envelope first and keeps `d` as borrowed raw JSON. Only payloads Akari
understands are parsed further, and nothing is copied for the rest.

## Opcodes Akari receives

From the [opcode table](https://docs.discord.food/gateway/opcodes-and-close-codes#gateway-opcodes):

| Op | Name | `d` | `GatewayEvent` |
|---|---|---|---|
| 0 | Dispatch | Event data | `Dispatch { seq, event }` |
| 1 | Heartbeat | Ignored | `Heartbeat`: send one right away |
| 7 | Reconnect | `null` | `Reconnect` |
| 9 | Invalid Session | Whether the session may be resumed | `InvalidSession { resumable }` |
| 10 | Hello | `{heartbeat_interval, _trace}` | `Hello` |
| 11 | Heartbeat ACK | `null` | `HeartbeatAck` |

Any other opcode decodes to `Unknown { op }`, and any dispatch Akari doesn't parse yet to
`DispatchEvent::Other(name)`; neither is an error. `RESUMED` becomes
`DispatchEvent::Resumed`. A dispatch without `s`, `t` or `d`, or a Hello without `d`, is a
`DecodeError::MissingField`. A dispatch whose data doesn't parse is a
`DecodeError::Dispatch` that keeps its `s` and `t`, so the connection still counts the
sequence number and a resume doesn't replay the event. Invalid Session with `d: null`
counts as not resumable.

## Connecting

`DiscordClient::gateway(token)` returns a `Gateway` that stays idle until `connect()`. The
connection then runs in a background task:

- URL: `Endpoints::gateway` (default `wss://gateway.discord.gg/`) with
  `?v=9&encoding=json&compress=zstd-stream`
  ([query string params](https://docs.discord.food/gateway/using-gateway#query-string-params)).
  Akari doesn't call `GET /gateway`; the default URL is stable, and a host can override it.
- The upgrade request sends `User-Agent` (the client properties' `browser_user_agent`) and
  `Origin` (`Endpoints::origin`, `https://discord.com`), as the desktop client's renderer
  does.
- TLS uses the `DiscordClient`'s rustls config, so the gateway verifies certificates exactly
  like REST.
- Incoming messages may be up to 64 MiB, compressed or not.

## Encoding and compression

`decode` takes plain JSON. The connection requests transport compression with
`compress=zstd-stream`. The reference lists it next to `zlib-stream` without limiting it to
bots.

How zstd-stream works
([zstd-stream](https://docs.discord.food/gateway/using-gateway#zstd-stream-compression)):

- One zstd decompression context stays alive for the lifetime of the connection.
- Each WebSocket message is exactly one gateway message, so the WebSocket message boundary
  is the message boundary; there is no end marker to look for.
- A message doesn't end the zstd frame. Call `ZSTD_decompressStream` repeatedly until all
  of the message's data has been processed; it won't necessarily return 0, because the
  frame stays open.

What Akari does with it (`gateway::decompress`):

- Each binary WebSocket message is decompressed until its input is used up and zstd stops
  short of filling the output buffer; then nothing is left inside zstd.
- Output over 64 MiB fails with a typed error and ends the gateway with
  `GatewayError::MessageTooLarge`. Reconnecting wouldn't help: a resume replays the same
  message.
- The output buffer grows for large messages such as READY and shrinks back to 256 KiB
  afterwards.
- Corrupt data drops the connection; the resume starts over with a fresh context.
- Discord honors `compress=zstd-stream` for user accounts: checked on 2026-10-08 with
  `akari-cli connect`, which received READY as zstd-compressed binary frames. Text frames
  are still decoded as plain JSON, in case that ever changes.

The alternative, which Akari doesn't use, is
[zlib-stream](https://docs.discord.food/gateway/using-gateway#zlib-stream-compression): one
inflate context per connection, and a message is complete once the buffer ends in
`00 00 ff ff`. ETF isn't supported.

## Heartbeat

[Hello](https://docs.discord.food/gateway/gateway-events#hello) carries
`heartbeat_interval` in milliseconds and `_trace`, the gateway servers that handled the
connection. Following
[heartbeat interval](https://docs.discord.food/gateway/using-gateway#heartbeat-interval):

- The first heartbeat goes out after a random delay of up to one interval, then one per
  interval. A heartbeat carries the last `s`, or `null` before the first dispatch.
- An interval below 1 s is raised to 1 s, so the heartbeat reserve can never take the whole
  send budget (see [Limits](#limits)).
- Op 1 from Discord is answered right away, without moving the regular schedule.
- If the previous heartbeat wasn't acknowledged by the time the next one is due, the
  connection is a zombie: Akari closes it with 4000 and resumes on a new one.
- Without Hello within 20 s, Akari closes the connection (4000) and tries again.
- Timers run on tokio's `Instant`, a monotonic clock that stops while the device sleeps
  (macOS, iOS, Linux, Android). After a wake the schedule simply continues; no late
  heartbeat fires. A socket that died during the sleep is noticed only when a heartbeat
  goes unacknowledged, up to about two intervals later, and until then `send()` reports
  success for writes into the dead socket. Hosts should call `disconnect()` before the
  device suspends and `connect()` after it wakes; see [Not implemented yet](#not-implemented-yet).

## Identify

Sent after Hello, on a connection without a session to resume
([Identify](https://docs.discord.food/gateway/gateway-events#identify)). The reference lets
clients identify before Hello; that the official client waits is **unverified**.

| Field | Value |
|---|---|
| `token` | The account's token |
| `capabilities` | `1597`: exactly the shape-changing capabilities in [ready.md](ready.md#capabilities). `AUTH_TOKEN_REFRESH` stays off, so READY never carries a replacement token |
| `properties` | `DiscordClient::properties()`, the same object `X-Super-Properties` encodes, so Identify always matches REST |
| `presence` | `{status: "unknown", since: 0, activities: [], afk: false}`: the gateway assigns the initial status ([Status Type](https://docs.discord.food/resources/presence#status-type)) |
| `compress` | `false` (no payload compression; transport compression is used) |
| `client_state` | `{guild_versions: {}, api_code_version: 0}`, as in the reference's example |

The reference's example sends `capabilities: 1734653`. Akari sends only the bits its
models understand; whether the smaller value matters to anti-abuse is **unverified**.

## Resuming and reconnecting

From [resuming](https://docs.discord.food/gateway/using-gateway#resuming):

- READY's `session_id` and `resume_gateway_url` make the session resumable. Resume
  (`{token, session_id, seq}`) goes to `resume_gateway_url`, but only over `wss://`: the
  token is never sent in plaintext. Any other URL is ignored, and resuming goes through
  `Endpoints::gateway`.
- If two connection attempts in a row on `resume_gateway_url` end before Hello (connecting
  fails, no Hello, or the connection drops first), Akari resumes through
  `Endpoints::gateway` instead. Failures there keep resuming, since an Identify would need
  the same server; Discord answering with op 9 (either `d`) or 4003/4007/4009 starts a new
  session.
- `seq` is the last `s` received. Once Akari decides to close a connection, it reads no
  further messages; their sequence numbers aren't counted, so the resume replays them
  instead of losing them.
- Op 7 (Reconnect): close with 4000 and resume.
- Op 9 (Invalid Session): close with 4000, wait a random 1–5 s, then resume if `d` is
  `true`, otherwise forget the session and identify on `Endpoints::gateway`. The reference
  is inconsistent here: its Invalid Session note allows staying on the connection, its
  resuming section says to disconnect. Akari reconnects, like twilight and discord.js.
  `disconnect()` followed by `connect()` skips a pending backoff but still waits out op 9's
  1–5 s.
- The first reconnect after a drop is immediate; repeated ones back off exponentially up to
  60 s, with jitter. The count starts over after a connection that stayed ready for 30 s.
- Closing with 1000 or 1001 invalidates the session; any other code keeps it resumable for
  a few minutes ([initiating a disconnect](https://docs.discord.food/gateway/using-gateway#initiating-a-disconnect)).
  `close()` sends 1000, `disconnect()` sends 4000 and keeps the session for the next
  `connect()`.

## Close codes

What Akari does when Discord closes the connection, per the
[close code table](https://docs.discord.food/gateway/opcodes-and-close-codes#gateway-close-event-codes):

| Code | Meaning | Akari |
|---|---|---|
| 4000 | Unknown error | Resume |
| 4001 | Unknown opcode | Resume |
| 4002 | Decode error (also: a payload over 15 KiB) | Resume |
| 4003 | Not authenticated, or the session was invalidated | New session |
| 4004 | Authentication failed | Stop: `GatewayError::AuthenticationFailed`; the user has to log in again |
| 4005 | Already authenticated | Resume |
| 4007 | Invalid `seq` | New session |
| 4008 | Rate limited | Resume |
| 4009 | Session timed out | New session |
| 4010–4014 | Shards, API version, intents | Stop: `GatewayError::Rejected { code }`; reconnecting can't fix them |
| 4015 | Too many user account sessions | Stop: `Rejected` |
| 4016 | Console connection request canceled | Stop: `Rejected` |
| 1000, 1001, others, none | | Resume; a dead session answers with op 9 |

## Limits

- **Sending:** 120 gateway events per connection every 60 s
  ([rate limiting](https://docs.discord.food/gateway/using-gateway#rate-limiting)). Akari
  counts every send in a sliding 60 s window. Heartbeats, Identify and Resume always go
  out; commands get what's left after a reserve of `ceil(60 s / heartbeat_interval) + 4`
  (6 at the usual 41.25 s). That leaves at most 114 commands per minute, a few less while
  the window also holds heartbeats and Identify. Commands over the budget wait for room; if
  the connection drops meanwhile, they fail with `SendError::NotConnected`. The reference
  doesn't say whether heartbeats count, so Akari assumes they do.
- **Payload size:** a payload over 15 KiB gets the connection closed with 4002
  ([sending events](https://docs.discord.food/gateway/using-gateway#sending-events)), so
  `send()` refuses it with `SendError::TooLarge`.
- **Presence:** 5 updates per 20 s
  ([Update Presence](https://docs.discord.food/gateway/gateway-events#update-presence)).
  Not enforced by the driver yet; a host that lets the user change status quickly has to
  debounce.
- **Writes:** a write that can't finish within 10 s, because the peer stopped reading and
  the send buffer is full, counts as a lost connection (`TransportErrorKind::Timeout`) and
  resumes. Otherwise a stalled socket would hold off heartbeats, `close()` and
  `disconnect()` until the OS gives up on the TCP connection.
- **Incoming:** messages up to 64 MiB.

## Using a `Gateway`

- `next()` returns events in order. Heartbeats and reading the socket never wait for it:
  events go into an unbounded queue, so only dispatches can grow memory, and a reader that
  stalls for minutes doesn't cost the connection. The trade-off is memory: if a reader
  stops for good, the queue keeps growing. Akari logs a warning once 10,000 events are
  waiting and again at every doubling (20,000, 40,000, …), and re-arms after the queue
  drains below 10,000.
- `Dispatch(Ready)` starts a new session: everything an earlier session delivered is stale.
  `Dispatch(Resumed)` ends the replay after a resume. `Reconnecting { resume, delay,
  reason }` comes before each new connection.
- After a fatal error, `next()` returns it once, then `GatewayError::Closed`.
- `send(command)` resolves once the command is written. Without a ready session (before
  READY or RESUMED, while reconnecting, after `disconnect()`) it fails at once with
  `SendError::NotConnected`; nothing is queued across connections. After `close()` it
  returns `SendError::Closed`, also while the close handshake is still running.
- `close()` and dropping the `Gateway` close the socket with 1000, which ends the session.
  Without a socket, after `disconnect()` or between reconnects, there's nothing to send 1000
  on: Discord keeps the session until it times out after a few minutes. `disconnect()`
  closes with 4000 and keeps `session_id` and `seq`; the next `connect()` resumes. A later
  pause/resume API for mobile backgrounding maps onto these two calls. A `close()` during a
  reconnect cuts that connection's close handshake short and ends the gateway without
  another `Reconnecting`.
- The `capture` feature, which only akari-cli enables, adds
  `Gateway::capture_next_ready()`: the next READY arrives once more as
  `ConnectionEvent::CapturedReady` with its raw JSON, also when it then fails to decode. See
  [ready.md](ready.md#checking-a-real-ready).

## Presence

`GatewayCommand::UpdatePresence { status }` sends op 3 with only a status
([Update Presence](https://docs.discord.food/gateway/gateway-events#update-presence)):
`{since: 0, activities: [], status, afk: false}`. The statuses a client can send are
`online`, `idle`, `dnd` and `invisible`
([Status Type](https://docs.discord.food/resources/presence#status-type)); `unknown` is only
for Identify. A new session starts with `unknown` again, so `Account::set_status` remembers
the chosen status and sends it after every READY; after RESUMED only if it didn't go out
before. `akari-cli connect --status <status>` does the same on a bare `Gateway`.

Observed on 2026-10-08 with `akari-cli connect --keep-open`, watched from a second account:

- With no other session (the phone app force-quit, the account shown as offline), Identify
  with `unknown` alone made the account appear online. A presence update isn't needed to
  show up.
- `--status dnd` (op 3 with only the status) showed the account as do not disturb.
- While another session was active (the phone session from scanning the QR code), the
  account showed as online with the mobile indicator the whole time, and `--status dnd`
  had no visible effect: another session's status can override Akari's. See the open
  point under [Not implemented yet](#not-implemented-yet).

## Guild subscriptions

Op 37 (Guild Subscriptions Bulk) is only a row in the reference's
[opcode table](https://docs.discord.food/gateway/opcodes-and-close-codes#gateway-opcodes); its payload is
undocumented. Akari sends what the official client and open-source clients
(discord.py-self, Abaddon) are observed to send when a channel is opened. In a large
guild that includes the channel's member list:

```json
{"op": 37, "d": {"subscriptions": {"200000000000000001": {
  "typing": true, "activities": true, "threads": true,
  "channels": {"300000000000000002": [[0, 99]]},
  "thread_member_lists": []
}}}}
```

- `GatewayCommand::SubscribeGuilds { guilds }`, one `GuildSubscription` per guild. Only
  keys that are present are updated. Akari sends no `members` and no `member_updates`.
- Every guild with a viewed channel gets the three flags. `typing: true` is what makes
  the gateway treat the guild as subscribed (discord.py-self).
- **Guilds READY marks `large`** (over 250 members) also get member lists: the first 100
  entries (`[[0, 99]]`, the official client's first page) of every viewed channel's list,
  and for a viewed thread its parent channel's list plus the thread's own
  (`thread_member_lists`). This mirrors the official client, which loads the member list of
  the channel it shows. It is not required for MESSAGE_CREATE, verified on 2026-10-09 (see
  below). A `channels` map replaces the one sent before for that guild
  (discord.py-self merges them client-side for that reason), so every send lists all
  viewed channels. When a large guild has no viewed channel any more, its lists are dropped
  with `"channels": {}` and `"thread_member_lists": []`; the guild stays subscribed.
- `Account` sends a guild's entry when one of its channels is viewed or loaded while
  online and the entry changed, and sends the entries of all guilds with a viewed channel
  again after every READY and RESUMED, like the official client. Each entry goes out once
  per change: a channel viewed right after READY isn't sent again by the re-send. A guild
  viewed while offline is subscribed after the next READY or RESUMED.
- The payload is split into several commands so each stays under 15 KiB, the official
  client's limit, below the gateway's 16 KiB.
- The first subscription to a guild may bring a GUILD_CREATE for it, which the store
  applies as a replacement. `threads: true` brings THREAD_LIST_SYNC, and member lists bring
  GUILD_MEMBER_LIST_UPDATE and THREAD_MEMBER_LIST_UPDATE. None of these are decoded yet:
  they stay `DispatchEvent::Other`, and decoding a 100-member SYNC (about 70 KiB) to that
  takes about 65 µs, with nothing kept.

Which subscription a guild needs for live messages depends on its size:

| Members | Live MESSAGE_CREATE, UPDATE, DELETE | Source |
|---|---|---|
| Up to 250 (not `large`) | Always | [Identify](https://docs.discord.food/gateway/gateway-events#identify-structure): `large_threshold` |
| 250 to 75,000 | Subscribed automatically on connect | discord.py-self (**unverified**; the reference says they need a subscription) |
| Over 75,000 | Only with the guild subscription | discord.py-self |
| Millions | With the guild subscription; the channel's member list isn't needed | A/B test on 2026-10-09 in a server of 4,518,424 members (below). [discord.py #6340](https://github.com/Rapptz/discord.py/issues/6340) saw no messages in 500,000 and 700,000 member servers until an op 14 that also set `typing`, so it was most likely the guild subscription there too |

Observed on 2026-10-09 with `akari-cli tail` on a test account:

- **A small server** (under 250 members): new, edited and deleted messages arrived live;
  op 37 carried the flags only.
- **A mid-size server** (a few thousand): new and edited messages arrived live; op 37
  carried `channels: {<channel>: [[0, 99]]}` and `thread_member_lists: []`.
- **A server of millions** (4,518,424 members), one minute per run in the same channel:
  with `tail --flags-only` op 37 carried only the flags and 6 live messages arrived; with the
  default, op 37 also carried `channels: {<channel>: [[0, 99]]}` and 5 arrived, along with
  MESSAGE_UPDATE, MESSAGE_DELETE, TYPING_START, reactions and GUILD_MEMBER_LIST_UPDATE. So
  the member list isn't needed for live messages. A first run had shown nothing live because
  the account was only previewing the server: the guild wasn't in READY and no op 37 was
  sent. Joining it was the fix.
- A guild the user only previews (lurks) isn't in READY, so Akari can't view it yet; see
  [Not implemented yet](#not-implemented-yet).

### Debugging subscriptions

`RUST_LOG=akari_core::subscriptions=debug,akari_core::dispatches=debug` makes akari-cli
log every op 37 payload it sends, a debug line when a channel can't be subscribed because
it isn't in the state, and each dispatch's name, sequence number and guild ID. Nothing
else from a payload is logged, and the guild ID is only read while that log is on. The
hidden `akari-cli tail --flags-only <channel_id>` sends op 37 without member lists.
Logs written to a file carry no color codes, so they can be searched:

```sh
RUST_LOG=akari_core::subscriptions=debug,akari_core::dispatches=debug \
  akari-cli tail <channel_id> 2> tail.log
grep 'sending op 37' tail.log
grep 'event=MESSAGE_CREATE' tail.log | grep -c 'guild_id=<guild_id>'
```

The log holds real guild and channel IDs, though no message content or token; keep it out
of the repo.

## Keeping the token out of logs

- Identify and Resume carry the token. akari-core never logs payloads, and nothing in the
  gateway module uses `#[instrument]`, which would record the arguments of a function that
  holds the token. `tests/gateway_logs.rs` connects, resumes and closes with a `TRACE`
  subscriber and checks that the token never shows up.
- The JSON strings built for Identify and Resume are ordinary copies that aren't zeroed,
  like the login request bodies (see [login.md](login.md#keeping-secrets-out-of-logs)).
- **tungstenite logs every frame at `trace` level through the `log` crate, Identify and
  Resume included.** Hosts must not forward `log` records at `trace` level for
  `tungstenite`. akari-cli doesn't forward `log` records at all.

## Not implemented yet

- Op 40 (QoS Heartbeat) and op 41 (Update Time Spent Session ID). The reference recommends
  both and the official client is believed to send them; the effect of their absence on
  anti-abuse is **unverified**.
- Op 14 member list subscriptions, voice states, presence activities.
- Previewing (lurking) guilds the user hasn't joined. They aren't in READY, so their
  channels aren't in the store and can't be viewed or subscribed. The official client
  previews them through other requests; that's for a later milestone.
- `GET /gateway` with a cached URL, and persisting a session across launches for a fast
  resume after a cold start.
- Status across several sessions. When a user has several sessions with a presence,
  Discord broadcasts an overall one, shown as the session with `session_id` `all`
  ([Session Object](https://docs.discord.food/resources/presence#session-object)); how it
  picks that status isn't documented. The official client also stores the chosen status
  in the user settings proto (`status.status`, "used to sync presence across clients",
  [Status Settings](https://docs.discord.food/resources/user-settings-proto#status-settings-structure)).
  A later milestone has to find out how Discord combines the sessions' statuses and set
  the status through the settings as well, so another device doesn't override Akari's.
- A pause/resume API for suspend and wake. Open point: on suspend it has to
  `disconnect()` and on wake `connect()`, because the gateway's own timers don't notice a
  sleep (see [Heartbeat](#heartbeat)).
