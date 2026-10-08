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
- Text frames are decoded as plain JSON, so the connection still works if Discord ignores
  the compression request for user accounts. Which of the two Discord sends is
  **unverified**.

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
- Op 1 from Discord is answered right away, without moving the regular schedule.
- If the previous heartbeat wasn't acknowledged by the time the next one is due, the
  connection is a zombie: Akari closes it with 4000 and resumes on a new one.
- Without Hello within 20 s, Akari closes the connection (4000) and tries again.
- After a device sleep, the late timer sends one heartbeat, not a burst.

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
- `seq` is the last `s` received. Once Akari decides to close a connection, it reads no
  further messages; their sequence numbers aren't counted, so the resume replays them
  instead of losing them.
- Op 7 (Reconnect): close with 4000 and resume.
- Op 9 (Invalid Session): close with 4000, wait a random 1–5 s, then resume if `d` is
  `true`, otherwise forget the session and identify on `Endpoints::gateway`. The reference
  is inconsistent here: its Invalid Session note allows staying on the connection, its
  resuming section says to disconnect. Akari reconnects, like twilight and discord.js.
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
  `SendError::NotConnected`; nothing is queued across connections.
- `close()` and dropping the `Gateway` close the socket with 1000, which ends the session.
  Without a socket, after `disconnect()` or between reconnects, there's nothing to send 1000
  on: Discord keeps the session until it times out after a few minutes. `disconnect()`
  closes with 4000 and keeps `session_id` and `seq`; the next `connect()` resumes. A later
  pause/resume API for mobile backgrounding maps onto these two calls.
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
for Identify. `akari-cli connect --status <status>` sends it after every READY, because a
new session starts with `unknown` again.

Whether an account appears online to others after Identify with `unknown` alone, without a
presence update, is **unverified**.

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
- READY_SUPPLEMENTAL, op 14 guild subscriptions, voice states, presence activities.
- `GET /gateway` with a cached URL, and persisting a session across launches for a fast
  resume after a cold start.
