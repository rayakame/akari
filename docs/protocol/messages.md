# Messages

How `akari_core::Account` loads message history into the store's windows and sends
messages. Sources: [Messages](https://docs.discord.food/resources/message) and
[Rate Limits](https://docs.discord.food/topics/rate-limits). Windows themselves are in
[dispatches.md](dispatches.md#message-windows), the request limits in
[rate-limits.md](rate-limits.md).

## Loading history

[List Messages](https://docs.discord.food/resources/message#list-messages):
`GET /channels/{channel.id}/messages` with `limit` (1–100, default 50) and at most one of
`before`, `after` or `around`. "If multiple are provided, only `around` or `before` is
respected." Without `READ_MESSAGE_HISTORY` the list is empty.

The reference doesn't say how a page is ordered. Discord's bot documentation says "from
newest to oldest", and open-source clients (discord.py-self) and the official client
treat every page that way. Facts Akari relies on, **unverified** beyond that:

- `before=X` gives the newest `limit` messages before X.
- `after=X` gives the *oldest* `limit` messages after X, so paging forward from a
  window's last message leaves no gap.
- `around=X` gives about half the limit on each side of X, and X.

Akari sorts every page by ID itself, so the order Discord uses doesn't matter for
correctness. A message in a page that fails to decode is skipped and logged by ID only.

`Account::load_messages(channel, load)` takes an intent instead of raw IDs, so a window
can't get a hole. The limit is clamped to 1–100.

| `MessageLoad` | Request | Window |
|---|---|---|
| `Latest` | no cursor | Views the channel. If the window is at the present, the page fills it from the old end; a stale window is refreshed (below); otherwise the window is replaced by the page (`MessagesCleared`, then `MessagesLoaded`): "jump to present" |
| `Older` | `before` = the window's first message | Older history, `MessagesLoaded`. Past 200 messages the newest are trimmed and `latest` becomes false |
| `Newer` | `after` = the window's last message | Newer history. Nothing to do if the window is already at the present. A short page means the present is reached |
| `Around { id }` | `around` = `id` | Views the channel and replaces the window with the page |

- A load that ends at the present (`Latest`, `Newer`, the stale refresh) holds live
  messages back while it runs, and adds them after the page. Otherwise a message that
  arrives during the request could land in the window before the messages it follows.
- A load whose window was cleared, replaced or evicted while it ran is dropped: the page
  belongs to messages that are no longer there.
- On an error the window stays as it was.

## After a new session

A new session (READY after a failed resume) can have missed anything. Every window is
marked stale (`MessagesStale`): its messages stay, so a UI keeps its scroll position,
and new messages are held back until a refresh. Right after READY, `Account` refreshes
every stale window with the latest 100 messages:

- If the page reaches the window (its oldest message is at or before the window's
  newest), the page is the truth within its ID range: messages missing from it are
  deleted (`MessageDeleted`), changed ones updated (`MessageUpdated`), missed ones
  inserted (`MessagesLoaded` with the range). The held live messages follow, and the
  window is fresh and at the present again.
- If more than 100 messages were missed, the page can't be joined to the window. The
  window keeps its messages, stays stale, and stops being at the present (`latest` false,
  `MessagesStale` again); the UI offers "jump to present", which is a `Latest` load.
- Older parts of a window aren't re-checked, like the official client.

## Sending

[Create Message](https://docs.discord.food/resources/message#create-message):
`POST /channels/{channel.id}/messages` with `{content, nonce, tts: false, flags: 0}`.

- **Nonce.** "Used for message deduplication (will be present in the returned object and
  accompanying Message Create Gateway event)." Like the official client, Akari sends a
  snowflake of the current time, as a decimal string, strictly increasing per account.
  "Sending multiple messages in the same channel with the same nonce in a short period of
  time will result in only the first message being sent"; how short is undocumented. A
  nonce isn't kept in history. `enforce_nonce` isn't in the reference and no user client
  sends it.
- **Pending.** `Account::send_message` adds the message at once to the window's outbox,
  with the nonce as its provisional ID and `Delivery::Pending` (`MessageInserted`). The
  outbox is shown after the window's messages and is never trimmed, and a window with
  messages in its outbox is never evicted.
- **Confirmed.** The REST response and the gateway's MESSAGE_CREATE both carry the nonce,
  and whichever comes first replaces the pending message: `MessageReplaced { pending_id,
  message }`. The message is then in the window if the window is at the present; otherwise
  it comes with the next load. The other one finds nothing pending and is handled like any
  repeated message.
- **Failed.** On an error the message stays in the outbox as `Delivery::Failed`
  (`MessageUpdated`). `retry_message` sends it again with the same nonce; `discard_message`
  drops it (`MessageDeleted`).
- **Same nonce on every retry.** A 429 or 502/504 retry and `retry_message` all send the
  original nonce, so a request Discord processed before the client saw an error doesn't
  post the message twice, and the echo still matches.
- **Order.** Sends to one channel go out one at a time, in the order they were made,
  retries included (see [rate-limits.md](rate-limits.md)).
- **Where it goes.** Sending into a window that isn't at the present first jumps to the
  present, like the official client. Sending into a stale window just queues; the refresh
  brings the confirmed message.
- **Content.** Empty or whitespace-only content is refused before any request
  (`RequestError::InvalidRequest`); Discord would answer 50006. The length isn't checked
  locally: the limit is 2,000 characters, or 4,000 with Nitro (**unverified**), and Discord
  answers 50035 when it's too long.

## Errors

`RequestError` is what a load or send returns:

- `Unauthorized`: a 401. The token is gone: every later request fails at once, and the
  account closes with `GatewayError::AuthenticationFailed` like a gateway 4004, so a UI
  shows the login screen.
- `RateLimited { retry_after }`: still rate limited after the retries.
- `CaptchaRequired(challenge)`: the request needs a captcha, which Akari doesn't solve
  yet. A send that needs one stays in the outbox as failed.
- `Discord { status, code, message }`: an API error, e.g. 50013 Missing Permissions,
  50001 Missing Access, 10003 Unknown Channel, 20016 slowmode, 50035 invalid form body
  ([JSON error codes](https://docs.discord.food/topics/errors#json-error-codes)).
- `Network(TransportError)`, `UnexpectedResponse`, `InvalidRequest` (empty content, a
  failed message that isn't there), `Closed` (the account is closed).

None of them carries the token or message content.

## Live messages

MESSAGE_CREATE, MESSAGE_UPDATE and MESSAGE_DELETE change only windows; see
[dispatches.md](dispatches.md). In guilds over the large threshold Discord sends them only
to clients that subscribed to the guild, so Akari subscribes every guild with a viewed
channel ([gateway.md](gateway.md#guild-subscriptions)).
