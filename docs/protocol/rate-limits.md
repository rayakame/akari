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
  headers." Akari uses them when they come and doesn't depend on them. **Unverified**
  which ones Discord sends to Akari.
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
  next request waits for the reset.
- **The global limit** is enforced on the client as a sliding window of 50 request starts
  per second, across all routes of the account.
- **A 429** pauses its route key (or, if global, every route) for the retry delay, then
  retries, up to 3 times. The delay is the body's `retry_after`, else
  `X-RateLimit-Reset-After`, else `Retry-After`; the reference doesn't say which wins
  when they disagree. A 429 with none of them isn't retried, and the caller gets
  `RequestError::RateLimited`.
- **At most 10 s of waiting.** A request that would wait longer for a pause or a bucket
  reset, such as a send in slowmode (**unverified** whether slowmode answers 429), fails
  at once with `RequestError::RateLimited { retry_after }` instead, so a UI can say how
  long to wait and a send shows as failed rather than pending.
- **502 and 504** are retried once after 1 s.
- **A 401** marks the token as rejected: every later request fails at once with
  `RequestError::Unauthorized` instead of adding to the invalid-request count, and the
  account closes as for the gateway's 4004 (see [messages.md](messages.md#errors)).
- **After `close()`** requests fail with `RequestError::Closed`. A request that was
  waiting for its route when the account closed or got a 401 isn't sent.
- A retry sends the same request body, so a message send keeps its nonce (see
  [messages.md](messages.md#sending)).

## Not implemented yet

- `X-RateLimit-Scope: shared` 429s, which "are not counted against you", are handled
  like per-route ones.
- 202 "unavailable resources" responses with their own `retry_after`.
- Closing doesn't cut a wait short: a request waiting for its route fails with
  `RequestError::Closed` only once the wait is over (at most 10 s).
- The `HIGH_GLOBAL_RATE_LIMIT` user flag, which raises the global limit to 1,200 per second.
