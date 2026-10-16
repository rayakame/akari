# Rate limits

How `AccountRest` (`crates/akari-core/src/rest/`) keeps an account's REST requests within
Discord's limits. Source: [Rate Limits](https://docs.discord.food/topics/rate-limits).
Login and logout go through the plain REST client without the limiter; they are few, and
their 429s are reported to the caller.

## What Discord sends

- Per-route limits come in `X-RateLimit-Limit`, `X-RateLimit-Remaining`,
  `X-RateLimit-Reset-After` (float seconds) and `X-RateLimit-Bucket`, a hash shared by
  "a set of similar endpoints". Limits are kept per top-level resource: the channel,
  guild or webhook ID in the path.
- For user tokens these headers are usually missing: "User authorization _usually_ only
  returns the **Retry-After**, **X-RateLimit-Global**, and **X-RateLimit-Scope**
  headers." Akari uses them when they come and doesn't depend on them. Observed on
  2026-10-11 (the macOS app's `load` log, second account): the first `GET
  /channels/{id}/messages` after a launch carried no `X-RateLimit-*` headers. Whether fast
  paging brings them, or 429s, is **unverified** until the paging check.
- A 429 body has `message`, `retry_after` (float seconds) and `global`.
  `X-RateLimit-Global: true` or `"global": true` marks the global limit.
- The global limit is 50 requests per second per account.
- 401, 403 and 429 responses count toward Cloudflare's ban on invalid requests (10,000 per
  10 minutes, then a 24 hour ban). Without any `Retry-After` the reference says not to
  retry programmatically.
- Discord publishes no per-route values for user tokens, so none are hard-coded.

## What Akari does

- **One lane per route key.** A route key is the HTTP method, the route template (e.g.
  `channels/{}/messages`) and the top-level ID. Requests on one key run one at a time, in
  the order they were made; a retry keeps its place, so a send retried after a 429 still
  goes out before the sends made after it. Different keys run in parallel: a slow history
  load (`GET`) never holds up a send (`POST`) in the same channel.
- **Buckets** are learned: once a response names its `X-RateLimit-Bucket`, routes with the
  same hash and top-level ID share `Remaining` and `Reset-After`. With `Remaining: 0` the
  next request waits for the reset, counted from when the response's headers arrived (Discord
  counts from its answer, so this is never early), not from the end of the body. While
  `Remaining` is above 0, or no bucket is known, nothing waits.
- **The global limit** is enforced on the client as a sliding window of 50 request starts
  per second, across all routes of the account.
- **A 429** pauses its route key (or, if global, every route) for the retry delay, then
  retries, up to 3 times. The delay is the body's `retry_after`, else
  `X-RateLimit-Reset-After`, else `Retry-After`; the reference doesn't say which wins
  when they disagree. A 429 with none of them isn't retried, and the caller gets
  `RequestError::RateLimited`.
- **At most 10 s of waiting.** A request that would wait longer for a pause or a bucket
  reset, such as a send in a channel with a long slowmode, fails at once with
  `RequestError::RateLimited { retry_after }` instead, so a UI can say how long to wait and
  a send shows as failed rather than pending.
- **Slowmode** answers a send with HTTP 429, JSON code 20016 ("blocked by slowmode",
  [JSON error codes](https://discord.com/developers/docs/topics/opcodes-and-status-codes#json))
  and the remaining cooldown in `retry_after`. Discord's documentation has no example of this
  answer; a Discord-compatible server and a client patch both use exactly this shape
  ([Slowmode.ts](https://github.com/meowcorded/meowcord/blob/1b9d7223b58eef91d2af869371611b2617a42cba/src/api/util/handlers/Slowmode.ts)),
  so it's **unverified** until the manual check sees it. With a slowmode of 10 s or less the
  429 is waited out like any other; longer, the send fails with `RateLimited`.
- **502 and 504** are retried once after 1 s.
- **A 401** marks the token as rejected: every later request fails at once with
  `RequestError::Unauthorized` instead of adding to the invalid-request count, and the
  account closes as for the gateway's 4004 (see [messages.md](messages.md#errors)).
- **After `close()`** requests fail with `RequestError::Closed`. A request that was
  waiting for its route when the account closed or got a 401 isn't sent.
- A retry sends the same request body, so a message send keeps its nonce (see
  [messages.md](messages.md#sending)).

## Acks

Acks go through `AccountRest` like every other request, with pacing of their own on top
(`crates/akari-core/src/account/acks.rs`). What the official client sends is in
[read-states.md](read-states.md#acks-in-the-official-client).

- **Route keys.** `POST channels/{}/messages/{}/ack` is keyed by its channel, like a send, so
  each channel's acks have their own lane and bucket. `POST read-states/ack-bulk` names no
  channel or guild and has one key for the account (top-level ID `0`).
- **One request at a time.** All acks, single and bulk, go out one after another, the earliest
  due first; never two in flight. Loads and sends have their own keys and don't wait for them.
- **Acks never wait inside the request.** None of the waits and retries above applies to them:
  a request that would have to wait for a bucket reset, a 429 pause or the global limit, or
  that Discord answers with a 429, comes back to the ack queue at once with its wait, and the
  queue sends it again when it is due (below). Meanwhile the acks due before it go out, so an
  Escape doesn't wait behind a bulk that waits for its rate limit.
- **Paced acks** (the user reads): a channel's ack waits for a 2 s lull after its latest
  target, but no longer than 10 s after its first pending one, so a busy channel is still
  acked at least every 10 s (the official client's cap isn't observed). **Explicit acks**
  (Escape, the menus) go at once, and take a pending paced target of the channel with them.
- **One pending ack per channel.** A new target merges with a pending one; the newer ID wins.
  A channel's own ack, paced or explicit, takes the channel's entry out of a bulk that waits
  for a retry and goes on the channel's route.
- **Bulks** go out in chunks of 50 entries (the most one official request was seen with), one
  request per chunk, in order. A chunk takes the bulk entries due together, so the entries of
  two bulks marked at once can share a request. A channel's pending single ack joins its
  entry in the bulk.
- **Never backwards.** An ack at or below what Discord took for the channel isn't sent; a bulk
  whose entries are all skipped sends nothing. A manual MESSAGE_ACK for the channel (marked as
  unread elsewhere) and every READY clear what Discord took, so the channel can be acked again.
  The manual MESSAGE_ACK also drops what is still pending for the channel, and READY holds
  back what was pending before it until it is checked against READY's read states
  ([read-states.md](read-states.md#acks-from-elsewhere-and-ready)).
- **Waits and errors.** An ack the limiter holds back waits in the queue until the limit
  allows; that isn't a failure. A 429, a network error or a 5xx is one: the ack is tried again
  after 2 s, then after twice as long after each further failure up to a minute, or after the
  429's `retry_after` when that is longer, however long it is. Meanwhile it merges with newer
  targets for its channel. Any other error refuses the ack: a chunk of several entries goes
  again entry by entry, and a refused entry or channel ack is dropped and undone in the store
  ([read-states.md](read-states.md#refused-acks)). A 401 or `close()` stops the acks, and
  what's pending is dropped.

## Not implemented yet

- `X-RateLimit-Scope: shared` 429s, which "are not counted against you", are handled
  like per-route ones.
- 202 "unavailable resources" responses with their own `retry_after`.
- Closing doesn't cut a wait short: a request waiting for its route fails with
  `RequestError::Closed` only once the wait is over (at most 10 s).
- The `HIGH_GLOBAL_RATE_LIMIT` user flag, which raises the global limit to 1,200 per second.
