# Gateway

What `akari_core::gateway::decode` relies on. Connecting, identify, resume and
heartbeating get their notes when they are built.

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
`DispatchEvent::Other(name)`; neither is an error. A dispatch without `s`, `t` or `d`, or a
Hello without `d`, is a `DecodeError::MissingField`. Invalid Session with `d: null` counts
as not resumable.

## Hello

[Hello](https://docs.discord.food/gateway/gateway-events#hello) carries
`heartbeat_interval` in milliseconds and `_trace`, the gateway servers that handled the
connection. The first heartbeat goes out right away, optionally after a random delay of up
to one interval
([heartbeat interval](https://docs.discord.food/gateway/using-gateway#heartbeat-interval)).
Identify doesn't have to wait for Hello
([connection lifecycle](https://docs.discord.food/gateway/using-gateway#connection-lifecycle)).

## Encoding and compression

`decode` takes plain JSON. The gateway connection will request transport compression with
`compress=zstd-stream` in the connection URL
([query string params](https://docs.discord.food/gateway/using-gateway#query-string-params)).
The reference lists it next to `zlib-stream` without limiting it to bots; **unverified**
against real traffic. Decompression itself belongs to the gateway connection milestone; the
`zstd` crate is pinned in the workspace for it.

How zstd-stream works
([zstd-stream](https://docs.discord.food/gateway/using-gateway#zstd-stream-compression)):

- One zstd decompression context stays alive for the lifetime of the connection.
- Each WebSocket message is exactly one gateway message, so the WebSocket message boundary
  is the message boundary; there is no end marker to look for.
- A message doesn't end the zstd frame. Call `ZSTD_decompressStream` repeatedly until all
  of the message's data has been processed; it won't necessarily return 0, because the
  frame stays open.

The alternative, which Akari doesn't use, is
[zlib-stream](https://docs.discord.food/gateway/using-gateway#zlib-stream-compression): one
inflate context per connection, and a message is complete once the buffer ends in
`00 00 ff ff`. ETF isn't supported.
