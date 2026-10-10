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
  (`RequestError::InvalidRequest`); Discord would answer 50006.
- **Length.** The limit is 2,000 characters without Nitro and with Nitro Basic, 4,000 with
  Nitro (`premium_type` 2) ([Discord's support article on Nitro](https://support.discord.com/hc/en-us/articles/115000435108)
  and its [Nitro Basic article](https://support.discord.com/hc/en-us/articles/33694251638295)).
  Nitro Classic (1) doesn't list longer messages among its perks, so it gets 2,000
  (**unverified**). Discord counts Unicode code points, not UTF-16 units or graphemes: a
  community test of an 80-character limit accepted 80 emoji and refused 81
  ([openclaw#156096](https://github.com/openclaw/openclaw/pull/156096); **unverified** by
  Akari until the manual check). Too long content is answered with HTTP 400, code 50035, and
  `errors.content._errors[0].code` `BASE_TYPE_MAX_LENGTH` with the message "Must be 2000 or
  fewer in length." ([a write-up of Discord's responses](https://github.com/mymoomin/RSStoWebhook/blob/3fabc7c5441f360d2f7d0b9331d3a2522e16b1cc/design/discord-api.md)).
  Discord's OpenAPI description gives `content` a maximum length of 4,000
  ([discord-api-spec](https://github.com/discord/discord-api-spec)).

- **Queueing and delivering.** `send_message` is `queue_message` followed by
  `deliver_message`. `queue_message` refuses empty or too long content and a closed account
  before anything is queued, and returns the pending ID; a UI that keeps a draft clears it only
  then. `deliver_message` sends a queued message (jumping a detached window to the present).
  A pending message is delivered by one call at a time: a second `deliver_message` or a
  `retry_message` for it while it's on its way fails with `InvalidRequest` and sends nothing.
- **Akari's check.** `message_length(content)` counts code points, and
  `Store::message_length_limit()` gives the limit for the current user's `premium_type`
  (2,000 before READY). `send_message` refuses longer content with
  `RequestError::TooLong { limit }` before anything is queued or sent. Discord's own refusal
  (50035 with `BASE_TYPE_MAX_LENGTH` on `content`) becomes the same `TooLong`, with the limit
  read from Discord's message, or Akari's limit when the message holds no number; the message
  then stays failed like any other. So a count that differs from Discord's never shows as a
  generic error.

## Slowmode

`Store::slowmode(channel)` says how a channel's slowmode applies to the current user:
`interval` (the channel's or thread's `rate_limit_per_user`), `exempt`, and `until`, when the
user may send again. It is `None` for channels without slowmode, for DMs and group DMs, and
for unknown channels.

- **Exempt:** the user's permissions in the channel contain `BYPASS_SLOWMODE` (bit 52); the
  owner and administrators hold every bit ([models.md](models.md)). An exempt user never has a
  cooldown.
- **A cooldown starts** when `send_message` or `retry_message` queues a message, as the
  official client starts it when the user sends, so a second send right after the first can
  be held back by the UI instead of failing at Discord. A message of the current user from
  another device (MESSAGE_CREATE whose nonce isn't the send that started the cooldown)
  starts it too, since Discord counts per user.
- **A failed send** clears the cooldown it started, unless something moved it since: Discord
  didn't count that message. A `RateLimited { retry_after }` answer (Discord's 20016, see
  [rate-limits.md](rate-limits.md)) instead holds the cooldown until at least `retry_after`
  from now.
- **The clock:** cooldowns are kept as monotonic instants and turned into wall-clock time
  when read, so changing the system clock doesn't stretch or cut them.
- No event reports a cooldown. It only changes with the user's own sends and messages, which
  already bring window events, so a UI re-reads `slowmode` for its channel on those.

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
  50001 Missing Access, 50007 a user who doesn't accept the message (a 403), 10003 Unknown
  Channel, 50035 invalid form body
  ([JSON error codes](https://docs.discord.food/topics/errors#json-error-codes)).
- `TooLong { limit }`: the message is longer than the limit, Akari's or Discord's (above).
- `Network(TransportError)`, `UnexpectedResponse`, `InvalidRequest` (empty content, a
  failed message that isn't there, a message already being delivered), `Closed` (the account
  is closed).

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
| Millions of members, default | flags and the channel range | new messages; the guild's edits and deletes arrived too |
| Millions of members, `tail --flags-only` | flags only | new messages |

A first run in the server of millions showed nothing live because the account was only
previewing it: a previewed guild isn't in READY, so no op 37 was sent. Previewing guilds
is an open point for a later milestone.

## Components V2

A message with the `IS_COMPONENTS_V2` flag (`1 << 15`,
[message flags](https://docs.discord.food/resources/message#message-flags)) is laid out
entirely by its `components`: text displays, sections, containers, media galleries and so on.
Its `content` and `embeds` don't work, polls and stickers are disabled, and attachments show
only where a component exposes them
([components](https://docs.discord.food/resources/components)). Without its components it
would show as an empty row.

Akari doesn't model `components` yet. The state message keeps the flag, the FFI record says
`components_v2`, and the apps show a one-line placeholder instead of the layout; the row still
lists the message's attachments. Reading and rendering the components is a later milestone.
