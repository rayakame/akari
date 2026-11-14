# Reactions: the requests

How `Account` adds and removes reactions and lists the users who reacted
(`crates/akari-core/src/account/reactions.rs`, the routes in `rest/account.rs`). What the
gateway sends and how the store counts it is in
[dispatches.md](dispatches.md#reactions); the references are
[Create Reaction](https://docs.discord.food/resources/message#create-reaction) and the routes
after it.

**Everything here that shapes a request is from the code read of canary 671549, not observed**
(modules 649963 and 652215): the paths, the emoji's form in them, the queries, the statuses
and the error codes. No request of the official client was looked at in a network tab.
Answers are read leniently: a 2xx is success whatever its body, and the list of users skips
an entry that doesn't decode.

## Routes

`{c}` is the channel, `{m}` the message, `{emoji}` the emoji as below.

| What | Request | Notes |
|---|---|---|
| Add the user's reaction | `PUT /channels/{c}/messages/{m}/reactions/{emoji}/@me?location={location}&type=0` | no body. `type` 0 is a normal reaction, 1 a super reaction, which Akari never sends |
| Remove the user's reaction | `DELETE /channels/{c}/messages/{m}/reactions/{emoji}/{type}/@me?location={location}&burst={false\|true}` | the typed path: `{type}` is 0 or 1, and `burst` says the same |
| Remove another user's | `DELETE /channels/{c}/messages/{m}/reactions/{emoji}/{type}/{user_id}?location=Message&burst={false\|true}` | from the reactions window; needs Manage Messages |
| Remove every reaction | `DELETE /channels/{c}/messages/{m}/reactions` | no query; Manage Messages |
| Remove one emoji's | `DELETE /channels/{c}/messages/{m}/reactions/{emoji}` | no query; normal and super reactions at once; Manage Messages |
| The users who reacted | `GET /channels/{c}/messages/{m}/reactions/{emoji}?limit={n}&after={user_id}&type={0\|1}` | `after` left out on the first page; a JSON array of user objects |

`location` is where the user clicked, in the official client's words, percent-encoded:
`Message` (its default, and the reactions window's), `Message Inline Button` (a reaction
under a message, the add button at the row's end), `Message Hover Bar`, `Message Context
Menu`, `Message Reaction Picker`, `Forum Toolbar`. `Account::react` and `unreact` take it as
`ReactionLocation`. The client also declares `Message Shortcut`, `Double Tap`, `In App
Notification` and `Mobile Media Viewer`, which Akari has no place for.

Success is expected as 204 without a body; the official client reads none.

Before any of these the official client opens an archived thread it may write in
(`PATCH /channels/{id}` with `{"archived": false}`, see
[read-states.md](read-states.md)); Akari does the same, and a thread that won't open fails the
call like a refusal.

## The emoji in the path

- A Unicode emoji is the emoji string itself, as UTF-8, percent-encoded: ❤️ (U+2764 U+FE0F)
  is `%E2%9D%A4%EF%B8%8F`, 👍🏽 is `%F0%9F%91%8D%F0%9F%8F%BD`. Nothing is normalized: a
  variation selector, a skin tone, the joiners of a family and both letters of a flag go as
  given, and ❤ without U+FE0F is another reaction than ❤️.
- A custom emoji is `name:id` with the colon as it is: `blob_wave:700000000000000001`. An
  animated one has no prefix. For an emoji the message already has, Akari sends the name the
  message has it under, whatever the caller gave.
- A custom emoji whose name is gone (a deleted emoji still on a message) goes as `null:id`,
  which is what the official client's template makes of a missing name.

The official client puts the raw string into the URL and leaves the encoding to the browser.
Akari encodes each byte outside `A-Z a-z 0-9 - _ . ! ~ * ' ( )`, which is the same for every
emoji and also covers the keycaps (`#️⃣` starts with `#`, which would end a URL's path:
`%23%EF%B8%8F%E2%83%A3`). Whether Discord wants the colon literal or as `%3A`, and what it
makes of U+FE0F, isn't observed.

Requests for ❤️ and for a custom emoji, as Akari sends them:

```
PUT    /api/v9/channels/300000000000000002/messages/400000000000000003/reactions/%E2%9D%A4%EF%B8%8F/@me?location=Message%20Inline%20Button&type=0
DELETE /api/v9/channels/300000000000000002/messages/400000000000000003/reactions/%E2%9D%A4%EF%B8%8F/0/@me?location=Message%20Inline%20Button&burst=false
GET    /api/v9/channels/300000000000000002/messages/400000000000000003/reactions/%E2%9D%A4%EF%B8%8F?limit=100&type=0
PUT    /api/v9/channels/300000000000000002/messages/400000000000000003/reactions/blob_wave:700000000000000001/@me?location=Message%20Hover%20Bar&type=0
DELETE /api/v9/channels/300000000000000002/messages/400000000000000003/reactions/blob_wave:700000000000000001/1/100000000000000002?location=Message&burst=true
```

## The user's own reaction

`Account::react(channel, message, emoji, location)` and `unreact(…)` are optimistic, as the
official client is: it dispatches its own MESSAGE_REACTION_ADD or REMOVE before the request
and the inverse when the request fails.

- **At once.** The store applies the change as the user's own dispatch would, on every copy of
  the message, with the events a dispatch sends
  ([dispatches.md](dispatches.md#reactions)). Discord's echo then finds the user's flag set
  (or clear) and changes nothing. There is no pending look.
- **What may be asked** is the message's actions ([message-actions.md](../ui/message-actions.md)):
  `react` for an emoji the message doesn't have, `toggle_reaction` for one it has. Without it
  the call is `InvalidRequest` and nothing changes or is sent. A row is resolved to the
  message it shows (`Store::shown_message`), so a thread's starter row reacts to the message
  in the parent channel.
- **What the store already shows is not asked.** `react` on a reaction that is the user's,
  as a normal or a super reaction, and `unreact` on one that isn't, succeed without a
  request. The official client refuses a normal reaction on top of the user's own super
  reaction with an alert.
- **Super reactions.** Akari never adds one. `unreact` on a reaction that is the user's only
  as a super reaction (`me_burst` without `me`) removes the super reaction: `{type}` 1 and
  `burst=true`.
- **One request per message and emoji, the latest wish wins.** While a request runs, a
  further call changes the store at once and replaces what is asked next; the call before it
  is answered with success, since nothing failed for it. When the request is answered, the
  next one goes out only if Discord doesn't have what the user wants now. React, unreact,
  react in quick succession is one PUT; twenty toggles are at most the request that ran and
  one more.
- **A refusal** puts the store back to what Discord has: what its last answer left, or, if
  the gateway spoke of the user's reaction since (their own add or removal from another
  device, MESSAGE_REACTION_REMOVE_ALL, MESSAGE_REACTION_REMOVE_EMOJI), what the gateway
  said. So a refused removal never puts back a reaction a moderator removed meanwhile, and
  the undo of a deleted message's reaction changes nothing. The undo goes through the same
  rules as a dispatch: a reaction put back sits at the row's end if its emoji had left the
  list, as in the official client. If the user took the wish back before the refusal came,
  nothing is undone and nothing more is asked: Discord kept what they want.
- *Akari:* the official client has no such guard (its inverse dispatch is unconditional) and
  no latest-wish rule (every click is its own request).

## Removing others' reactions

`Account::remove_reaction(channel, message, emoji, kind, user)`,
`remove_reactions(channel, message, None)` and `remove_reactions(channel, message, Some(emoji))`
need the message's `remove_reactions` action (Manage Messages; never in a DM), else
`InvalidRequest` without a request. They change nothing in the store: the gateway's
MESSAGE_REACTION_REMOVE, REMOVE_ALL or REMOVE_EMOJI does, as in the official client, whose
optimistic removal of another user's reaction touches only its list of users.
`remove_reaction` for the user's own ID is `unreact` with the location `Message` (the
reactions window's ✕ on one's own row).

## The users who reacted

`Account::reactors(channel, message, emoji, kind, after, limit)` returns a `ReactorsPage`:
the users in Discord's order and `has_more`, which is set when Discord returned as many
entries as were asked for. The official client asks 100 for its window, then
`after=<the last user's id>` while the count says more, and 3 once per message, emoji and
type for a reaction's tooltip. Akari's call asks every time and keeps nothing; `limit` is
clamped to 1 to 100. The users are the store's own records where it has the same user.
Whether the objects carry a `member` in a guild isn't known; none is read.

## Errors

The official client's one handler for the five changing calls tells these apart, and
`ReactionError` follows it:

| Answer | Official client | `ReactionError` |
|---|---|---|
| 403, code 30010 | rolled back; alert "We appreciate the enthusiasm but..." / "Your reaction was not added because there are too many reactions on this message." | `TooMany` |
| 403, code 90001 | rolled back; the window shakes | `Blocked` |
| any other 403 (50013, 50001, a timeout) | rolled back silently, never retried | `Forbidden` |
| code 10014 | (retried once, then rolled back silently) | `UnknownEmoji` |
| 429 | waits `Retry-After` and asks once more; a second 429 rolls back | `RateLimited { retry_after }` after the one retry |
| anything else | one immediate retry, then rolled back silently | `Request(RequestError)` |

The limit of distinct emoji on a message is the server's: the client has no number for it and
sees only 30010 (20, by all accounts; not observed). The statuses and codes for a timed-out
member, a missing Read Message History, an emoji from another server without Use External
Emojis and a locked thread aren't observed either; they arrive as `Forbidden`, `UnknownEmoji`
or `Request` by the table above.

## Retries

The six calls may be repeated without harm, so they have a retry rule of their own
([rate-limits.md](rate-limits.md)):

- a 429 with a delay is waited out and the request sent once more, on the same lane; a second
  429 is the error;
- a 502, a 504 or a connection that broke is asked once more after the retry delay;
- nothing else is retried, and never more than once each. *Akari:* the official client also
  retries a 400, a 404 and a 500 once, at once.

Rate limit values for these routes aren't known: no bucket or limit header of a PUT or DELETE
`…/@me` was observed. Each route has its own lane per channel, so a channel's adds run one
after another and a list of users never waits behind them.

## akari-cli

`akari-cli react <channel> <message> <emoji>`, `unreact …` and
`reactors <channel> <message> <emoji> [--burst] [--after <user>] [--limit <n>]` call these. The
emoji is the Unicode emoji itself or a custom one as `name:id` (also `a:name:id`,
`<:name:id>`, `<a:name:id>`).
