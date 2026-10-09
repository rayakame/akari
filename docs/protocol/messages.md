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
| `Older` | `before` = the window's first message | Older history, `MessagesLoaded`. Past 200 messages the newest are trimmed and `latest` becomes false. Nothing to do without a window |
| `Newer` | `after` = the window's last message | Newer history. Nothing to do without a window or if it is already at the present. A short page means the present is reached, which also ends a stale window's staleness |
| `Around { id }` | `around` = `id` | Views the channel and replaces the window with the page |

- A load that ends at the present (`Latest`, `Newer`, the stale refresh) holds live
  messages back while it runs, and adds them after the page. Otherwise a message that
  arrives during the request could land in the window before the messages it follows.
- A load whose window was cleared, replaced or evicted while it ran is dropped: the page
  belongs to messages that are no longer there. So is an `Older` or `Newer` page whose
  end of the window was trimmed or deleted meanwhile, since it would leave a gap.
- A page is short if Discord sent fewer entries than the limit; messages skipped because
  they didn't decode still count.
- Edits and deletes that arrive while a page is loading are applied to the page when it
  lands, since it may predate them: a deleted message never comes back. An edit older than
  the page's own version of the message is skipped.
- On an error the window stays as it was, except a stale one (below). A load whose future
  is dropped before its page arrives, e.g. because its task was cancelled, ends the same
  way.

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
- A refresh that fails with a network error or a 5xx is tried again three times, after 1,
  2 and 4 s, so a flaky network right after a reconnect doesn't detach every window. If it
  still fails, or Discord refuses it (4xx), the window detaches the same way, so it doesn't
  hold live messages back forever.
- An empty window takes the page as it is. An empty page (no messages, or no
  `READ_MESSAGE_HISTORY`) leaves the messages and makes the window fresh again.
- A refresh that finishes after another load already refreshed the window changes
  nothing.
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
  sends it. The hidden `akari-cli send --repeat-nonce <channel_id> <text>` sends a message,
  then the same body again, to check that Discord ignores the repeat (**unverified** until
  then).
- **Pending.** `Account::send_message` adds the message at once to the window's outbox,
  with the nonce as its provisional ID and `Delivery::Pending` (`MessageInserted`). The
  outbox is shown after the window's messages and is never trimmed, and a window with
  messages in its outbox is never evicted. If the `send_message` future is dropped before
  Discord answers, e.g. because its task was cancelled, the message becomes failed.
- **Confirmed.** The REST response and the gateway's MESSAGE_CREATE both carry the nonce,
  and whichever comes first replaces the pending message: `MessageReplaced { pending_id,
  message }`. An echo counts only if its author is the current user: other users see our
  nonces too, and another client could send the same value. The message is then in the window if the window is at the present; otherwise
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
- **Where it goes.** Sending into a window that isn't at the present also jumps to the
  present, like the official client. The jump runs alongside the send and never holds it
  back; if it fails, the message is still sent. Sending into a stale window just queues;
  the refresh brings the confirmed message.
- **Content.** Empty or whitespace-only content is refused before any request
  (`RequestError::InvalidRequest`); Discord would answer 50006. The length isn't checked
  locally: the limit is 2,000 characters, or 4,000 with Nitro (**unverified**), and Discord
  answers 50035 when it's too long.

## Errors

`RequestError` is what a load or send returns:

- `Unauthorized`: a 401. The token is gone: every later request fails at once, and the
  account closes with `GatewayError::AuthenticationFailed` like a gateway 4004, so a UI
  shows the login screen.
- `RateLimited { retry_after }`: still rate limited after the retries, or the wait would
  be longer than 10 s (see [rate-limits.md](rate-limits.md)).
- `CaptchaRequired(challenge)`: the request needs a captcha, which Akari doesn't solve
  yet. A send that needs one stays in the outbox as failed.
- `ServerError { status }`: a 5xx; trying again later may work.
- `Discord { status, code, message }`: an API error, e.g. 50013 Missing Permissions,
  50001 Missing Access, 10003 Unknown Channel, 20016 slowmode, 50035 invalid form body
  ([JSON error codes](https://docs.discord.food/topics/errors#json-error-codes)).
- `Network(TransportError)`, `UnexpectedResponse`, `InvalidRequest` (empty content, a
  failed message that isn't there), `Closed` (the account is closed).

None of them carries the token or message content.

## Live messages

MESSAGE_CREATE, MESSAGE_UPDATE and MESSAGE_DELETE change only windows; see
[dispatches.md](dispatches.md). In large guilds Discord sends them only to sessions that
subscribed to the guild, so `Account` subscribes every guild with a viewed channel. In
large guilds it also subscribes the viewed channels' member lists, like the official
client; that isn't needed for live messages, even in a guild of millions (verified on
2026-10-09, [gateway.md](gateway.md#guild-subscriptions)).

Observed on 2026-10-09 with `akari-cli tail` on a test account:

| Server | op 37 | Live |
|---|---|---|
| Small (under 250 members) | guild flags | new, edit and delete |
| A few thousand members | flags and `channels: {<channel>: [[0, 99]]}` | new and edit |
| Millions of members, default | flags and the channel range | new, edit, delete |
| Millions of members, `tail --flags-only` | flags only | new messages |

A first run in the server of millions showed nothing live because the account was only
previewing it: a previewed guild isn't in READY, so no op 37 was sent. Previewing guilds
is an open point for a later milestone.
