# Read states

Where the user stopped reading each channel, how many mentions wait there, and the unread
dots and red counts Akari draws from them. The main reference is the
[read state topic](https://docs.discord.food/topics/read-state); where it is silent, Akari
follows the official desktop client as observed on 2026-10-14 (the UI side is in
[server-list.md](../ui/server-list.md#what-makes-a-server-unread) and
[channel-list.md](../ui/channel-list.md#what-makes-a-row-unread)). Akari shows what the user
read elsewhere and acknowledges what the user reads in it
([below](#how-akari-acks)).

## What Akari reads

| Source | Model | What it brings |
|---|---|---|
| READY `read_state` | `Versioned<ReadStateEntry>` | Per channel the last acknowledged message, the mention count and the flags; whole, since Identify names no version ([ready.md](ready.md#fields-akari-reads)) |
| READY `relationships` | `Vec<Relationship>` | Blocked users (type 2) and ignored ones (`user_ignored`), whose messages never count |
| MESSAGE_ACK | `MessageAck` | The user read a channel, in this session or another |
| CHANNEL_UNREAD_UPDATE, PASSIVE_UPDATE_V2 | `ChannelUnreadUpdate`, `PassiveUpdateV2` | Newer messages in guilds the session isn't subscribed to ([dispatches.md](dispatches.md#large-guilds)) |
| RELATIONSHIP_ADD, _UPDATE, _REMOVE | `Relationship`, `RelationshipUpdate`, `RelationshipRemove` | Changes to who is blocked or ignored |

Only channel read states (`read_state_type` 0) are kept. Identify doesn't ask for
`NON_CHANNEL_READ_STATES`, so the others (guild events, the notification center, …) should
not come; if they do, they are skipped. `last_pin_timestamp`, `last_viewed` and the flags
aren't kept: Akari has no pins view, and `last_viewed` only goes out with its own acks.

## The unread rule

A channel is unread when its newest message (`last_message_id`, kept by the store, forward
only) is past its read state's. Compared as snowflakes, so a deleted message keeps its place:
deleting the only unread message leaves the channel unread, as in the official client.

A channel without a read state:

- **A guild channel** counts as read up to when the user joined the guild (the member's
  `joined_at` as a snowflake): messages from before the join never show unread, later ones
  do. The official client showed a never-opened channel with a new message as unread.
- **A DM or group DM** is read until its next message, which starts its read state at the
  message before it, also when the DM's CHANNEL_CREATE already named that message.
- **A guild whose join time is unknown** is treated like a DM.
- **A thread** is unread from its start: its read position is one before the thread's own ID,
  so every message in it is unread, a forum post's starter message (which has the post's ID)
  included, and the new messages line sits above its first message. The official
  client did so for a thread the user was mentioned into (observed 2026-10-17: the line above
  the thread's first message, not above the mention). A joined thread from READY without a read
  state is taken the same way (**unverified**).

A read state the store starts this way (for a DM's first message, or a guild channel's or a
thread's first mention) is marked `estimated`: its `acked` is the join, the position before the
thread or the message before, not a message the user read. An ack or the user's own message replaces it with a real one.

The newest message only moves forward: from MESSAGE_CREATE, CHANNEL_CREATE, CHANNEL_UPDATE
and GUILD_CREATE (an older or missing `last_message_id` changes nothing), from the unread
updates of guilds without a subscription, and from a history page that reaches the present
(the newest messages, or a newer page that hit the end). Only READY replaces it. A new thread in a forum or
media channel is the forum's newest "message": THREAD_CREATE raises the parent's
`last_message_id` to the thread's ID. A thread's own messages move only the thread, so an
unjoined thread leaves its parent read; the "started a thread" message in the parent is an
ordinary message there.

## MESSAGE_ACK

- **The position** moves forward. An ack at or behind it changes nothing, its count included,
  so a late echo can't clear a mention counted since; only a read state the store started
  (`estimated`) takes one at its position. With `manual` (mark as unread) it can move back.
- **The count:** `mention_count` replaces it. Without one it stays, and drops to 0 once the
  ack reaches the channel's newest message (discord.py-self and concord do the same).
- An ack for a channel without a read state starts one. Acks of other read state types are
  ignored.

## Acks in the official client

What the official web client sends when the user reads, observed on 2026-10-15 in the
browser's Network list (German UI, a second account, the main account posting from a phone).
What Akari does with it is [below](#how-akari-acks).

**One channel.** `POST /channels/{channel_id}/messages/{message_id}/ack`, answered 200, with
the body

```json
{"token":null,"last_viewed":4306}
```

No `flags`, no `manual`. `last_viewed` is the number of days since the Discord epoch
(2015-01-01 UTC), rounded up. It was 4306 at 15:13 and at 19:58 UTC on 2026-10-15 (4305.63
and 4305.83 days), and 4308 at 11:36 UTC on 2026-10-17 (4307.48 days), where rounding to the
nearest day would give 4307. Akari rounds up too.

**When it goes out:**

- About 1–2 s after the user opens an unread channel. Opening a read channel sends nothing.
- At the bottom of the list, a burst of messages (five in quick succession) gives one ack,
  about 2 s after the last.
- Not while the list is scrolled up, while another app is focused, or while the window is
  minimized. Scrolling back down, focusing the browser again or restoring the window sends
  nothing either; the ack goes out on the next click (or key) in the client.
- Escape sends one plain ack with the same body, and nothing when the channel is read.
- Posting in a forum sends one plain ack. The forum stays unread, also after a reload.

**Many channels.** Shift+Escape (the server), a server's "Mark As Read" and a folder's "Mark
Folder As Read" send `POST /read-states/ack-bulk`, answered 204:

```json
{"read_states":[{"channel_id":"…","message_id":"…","read_state_type":0}]}
```

- The entries are the server's unread channels plus its forum channel(s). The forum was in
  every bulk, also when nothing had been posted there since the last one. Read channels and
  threads the user hasn't joined (such as a new forum post) have no entry. Only the server
  item's bulks were itemized (3 entries, then 2 after one more message); the folder item's
  wasn't.
- Shift+Escape at Home sends nothing.
- **Message IDs:** each channel's newest message ID. A forum's entry is a snowflake with only
  a timestamp (the low 22 bits are zero) for the moment of the click.
- **Chunks:** a 75,000+ member server marked read right after a reload, never opened, gave
  one request with exactly 50 entries. Most carried each channel's newest message ID; 6 shared
  one "now" snowflake. Whether 50 is a cap per request or that server's number of unread
  channels is **open**. [docs.discord.food](https://docs.discord.food/topics/read-state)
  documents no limit. One mark-as-read gave 254 MESSAGE_ACK events in another session, which
  points to chunks.

## How Akari acks

Akari acks what the user reads and marks channels and servers read, with the routes, body and
message IDs above. The rules are in `crates/akari-core/src/account/acks.rs` (pacing, sending,
retries and refusals), `rest/clock.rs` (Discord's time) and `state/apply/reading.rs` (what to
ack); how the requests share Discord's limits is in [rate-limits.md](rate-limits.md#acks).

**When.** Only for the channel the host says the user is reading (`Account::set_reading`),
never for one that merely receives messages, and never in answer to a MESSAGE_ACK from
elsewhere. The macOS app sets it only while the session is online, the app is active, the
window is key and visible, the list is at the present's bottom with its refresh after READY
in, and the user is engaged: opening the channel, a click or a key press in the window engages;
scrolling clearly away from the bottom, the window losing key, the app going inactive, the
window being minimized, the Mac going to sleep or fast user switching ends it, and after that,
coming back to the bottom or the window alone doesn't read
([message-list.md](../ui/message-list.md#reading-the-signal-to-ack)). A list the session
opened on its own (the open channel deleted or hidden, its server gone) isn't read until the
user clicks.

**What reading acks.** Up to the newest message the channel's list shows, while that list is at
the present and not stale, and never "now". Past that row it acks only messages known to be
deleted, since Discord keeps a deleted message as the channel's newest
([the unread rule](#the-unread-rule)): those a page reaching the present lacks, up to what was
the channel's newest message when the page was asked for, the list's newest row when it is
deleted at the present, and a message deleted while live messages were held back, once they
show. So a channel whose newest message was deleted reads like any other, and a message that
exists is never acked before the list shows it, except in a channel without Read Message
History: its pages come back empty, so reading acks its newest message. When the host sets the
channel, the store marks it read up to that message at once, so its badge goes, and the ack
follows, if the message is past the read position or the channel has mentions. After that,
only the list growing at its present end reads again: a live message, or a page that reaches
the present. A newest message raised any other way (a GUILD_CREATE, an unread update) reads
nothing until the list shows it, so a channel whose newest message is past its list stays
unread. `set_reading(None)` stops reading and clears nothing.

**Connection drops.** The core stops reading whenever the connection drops (every reconnect,
a resume included), on `disconnect()` and when the account closes: the session that comes back
may be a new one, and nothing is acked until the host says the user still reads the channel.
The macOS app sets it again once the session is online and the list's refresh is in. Before
the first READY nothing is read, and a READY acks nothing inside its write: its read states
are Discord's, and the list after it is read when the host sets the channel again or the list
grows.

**Pacing.** Per channel an ack waits for a 2 s lull after the latest message read, the first
one too, but goes out at the latest 10 s after the first one pending, so a busy channel still
syncs to the user's other devices. Escape, the menus and the unread bar's button go out at
once. Only one ack request is in flight at a time, the earliest due first; one held back by a
rate limit or waiting for a retry goes back into the queue, so the acks due before it, such as
an Escape, go first. A channel is never acked backwards: what Discord already took isn't sent
again, until another device marks the channel unread (a manual MESSAGE_ACK) or a READY comes,
which forget what was sent ([below](#acks-from-elsewhere-and-ready)).

**Errors.** A 429 or a transient error (network, 5xx) leaves the ack pending; it is retried
after a wait that doubles from 2 s up to a minute, or after the 429's `retry_after` when that
is longer, however long it is, merged with newer targets, until it goes through. A 401 ends the
account like a rejected token, and what's pending is dropped. Any other error refuses the ack
([below](#refused-acks)).

**The body.** A channel ack is `{"token":null,"last_viewed":N}` with `last_viewed` the days
since the Discord epoch, rounded up (at Discord's time, below), and no `flags` or `manual`. A
thread's is `{"token":null,"last_viewed":N,"flags":2}` (`IS_THREAD`), and one the user hasn't
joined goes without `last_viewed`: `{"token":null,"flags":2}`, as the official client sends
them ([Threads](#threads)). Akari sends the flag every time; whether the official client leaves
it out once the stored flags match isn't known.
The bulk is `{"read_states":[{channel_id, message_id, read_state_type: 0}]}`, in chunks of 50
entries (an unconfirmed reading of the one 50-entry request seen), one request per chunk. A
chunk takes whatever bulk entries are due together, so two bulks marked at once can share a
request. The channels are those whose badge shows unread or mentions, with their listed threads
([Threads](#threads); never unjoined ones), plus every forum and media channel the user can
view, read or not. With nothing unread there is no bulk, and no forum goes alone.

**Which message ID.** Reading acks the newest message the channel's list shows, or a deleted
one past it (above).
Escape, the menus and Shift+Escape ack each channel's newest message, as the official client
sends. A "now" snowflake (a timestamp, the low 22 bits zero) goes instead for:

- forum and media channels, as observed;
- channels whose newest message came from a PASSIVE_UPDATE_V2 or CHANNEL_UNREAD_UPDATE, which
  arrive once and may be behind Discord's (see [Large guilds](#large-guilds)). A READY, a
  history page that reaches the present, or a MESSAGE_CREATE that moves the newest message
  makes it ordinary again. A message Discord sends because it mentions the user (directly,
  through @everyone or @here, or through one of their roles) doesn't: Discord sends those even
  from a large guild it sends no other messages of (**unverified**; docs.discord.food doesn't
  say), so the messages after one may never arrive;
- every channel marked read before the first READY (Escape, the menus, Shift+Escape), whose
  newest messages come from the disk cache; reading itself waits for READY.

"Now" is taken when the ack is queued, at the user's action, and a retry sends the same ID, so
an ack that waits for its turn or a retry can't cover a message posted after the user acted.
It is never below the channel's newest known message. It comes from Discord's clock: the
`Date` header of the latest REST response plus the monotonic time since it arrived, never the
Mac's clock, so a Mac running ahead can't ack messages that arrive a moment later. The
header has whole seconds and is stamped before the answer travels, so the estimate runs up to
about a second behind Discord, and behind after a sleep, never ahead. Without a sample, the
newest message goes instead, and `last_viewed` falls back to the Mac's date.

**Echoes.** Discord answers an ack with MESSAGE_ACK on the gateway, also to Akari's own. One
at or behind the stored position changes nothing, so it can't clear a mention counted since,
unless it is manual (mark as unread) or the stored position is Akari's own estimate at the
same position ([MESSAGE_ACK](#message_ack)). The echo of an ack that a READY's swap applied
with READY's mention count ([below](#acks-from-elsewhere-and-ready)) sets the count at its
position too.

### Acks from elsewhere and READY

When the user marks a channel unread on another device, or reads it there while Akari is
away, one of Akari's acks for the channel may still be pending (waiting for its turn or a
retry, or on its way), and sending it afterwards would undo that:

- **A manual MESSAGE_ACK** (mark as unread elsewhere) drops the channel's pending ack, also one
  on its way, whose answer then changes nothing.
- **At a READY**, the acks pending from before it wait until READY is in. Each then goes out
  only if READY still has the position the channel had before Akari's ack; any other position
  means the channel was read or marked unread elsewhere meanwhile, and the ack is dropped. A
  position Akari estimated counts as none.
- **Acks made while a READY is prepared** (it is converted off the store's lock, which takes
  milliseconds for a large account) are the user's latest. The store takes them at once, so
  their badges go, and the swap applies them on top of READY's read states, forward only, and
  builds their channels' badges again, so a badge the user cleared stays cleared. One that
  reaches READY's newest message for the channel clears its count. Otherwise READY's count
  stays, since a mention READY counts past the ack is still unread, until the ack's echo brings
  Discord's count. Their requests wait until READY is in, like the ones from before it: one
  READY's position is past is dropped, any other goes (one at READY's position still clears
  READY's count). Nothing goes out while READY is prepared.

### Refused acks

Any error but a 429, a 401 or a transient one refuses an ack:

- **A bulk chunk** of several entries goes again entry by entry, each in a request of its own,
  since Discord may refuse it for only some of them.
- **A refused entry or channel ack** is dropped, and the store puts the channel's read state
  back to what it was before Akari's acks, so its badge shows again. It does so only while the
  store still holds what Akari's newest ack for the channel left: an ack from elsewhere, a
  newer local ack or a READY since is newer and stays. Of several acks merged for the channel,
  the undo goes back to the state that had read least far, whatever order they came in, and
  never below the position READY delivered. If a newer ack for the channel is already pending,
  that one goes out and carries the undo.
- Refusals, READY's swap and local acks take turns, so a refusal can't come between READY's
  state and its check, nor roll READY back.
- A refusal is logged at warn, with the IDs, the error's kind and Discord's status and code,
  and so is the undo.

## Mentions

Discord doesn't send a channel's new mention count; clients count it from MESSAGE_CREATE
([read state topic, "Automations"](https://docs.discord.food/topics/read-state)). Akari counts
a message only when it is past the read position and newer than the last message it counted
in that channel, so a repeated or old message adds nothing:

| Where | Counts |
|---|---|
| DM or group DM | Every message except "recipient removed" (type 2); in a muted one only messages that mention the user |
| Guild channel the user can see | A direct mention of the user; @everyone and @here unless the guild suppresses them; a role of the user's unless the guild suppresses roles; muted or not |
| Guild channel, with the account's `MENTION_ON_ALL_MESSAGES` | Every message where the notification level is all messages, unless the channel or guild is muted or it is a voice or stage channel |
| Thread the user hasn't joined | Only a direct mention of the user, which adds them to the thread. @everyone, roles and "all messages" don't: no source says they reach anyone but the thread's members |
| The user's own message | Reads the channel up to it and clears its count, except a poll result |

@silent messages (`SUPPRESS_NOTIFICATIONS`) count like any other; they only skip the
notification (observed 2026-10-14). Messages of blocked and ignored users never count.
Deleting a mentioning message lowers nothing, as in the official client.

Suppress settings decide the count, not the highlighted row: the official client still tints
a suppressed @everyone or role mention ([user-settings.md](user-settings.md#user-guild-settings)).

## Large guilds

Guilds over 75,000 members send no messages without a guild subscription, and Akari
subscribes a guild only when one of its channels is viewed
([gateway.md](gateway.md#guild-subscriptions)). Their unread dots come from PASSIVE_UPDATE_V2,
which most likely comes once, when a channel goes from read to unread
([dispatches.md](dispatches.md#large-guilds)). So one update has to be enough: it raises the
channel's newest message, nothing but an ack or READY can undo that, and an ack past it reads
the channel. In such a guild, Akari's newest message for a channel can be older than the real
one until the user opens it.

## Badges

The store keeps every channel's and guild's `Badge` (`unread`, `mentions`) as it changes, so
`Store::channel_badge` and `guild_badge` cost a lookup, and `unread_private_channels` a pass
over the DMs; none computes permissions.

- **A channel's row** is unread when the channel is, unless it is muted itself, it is a voice
  or stage channel (their chats never show unread), or its unread setting says only mentions.
  That setting is the first `UNREADS_*` flag of the channel, its parent and its category,
  then the guild's; without one, and with the account's `USE_NEW_NOTIFICATIONS`, the
  notification level decides (only mentions and nothing show no dot); otherwise every unread
  channel shows. Its count is the read state's mentions. A channel the user can't view shows
  nothing.
- **A guild** sums its text, announcement, forum, media, voice and stage channels and its
  listed threads ([Threads](#threads)): a thread's mentions count while it's listed, so a
  mention that adds the user counts from the members event that lists the thread, and a thread
  the user left or that was closed adds nothing. Its dot needs one of them unread, a channel
  not under a muted category or muted channel (a thread's parent doesn't matter), and the guild
  itself unmuted; its count adds up all their mentions, muted or not. With "hide muted
  channels", muted channels and muted threads don't count at all. A folder's badge is its
  guilds' sum, added up by the host.
- **Community servers with opt-in channels** ("Channels & Roles": the `COMMUNITY` feature and
  the user's `OPT_IN_CHANNELS_ON`) follow the official client's two extra rules: a channel
  whose newest message is over 7 days old shows neither unread nor mentions, and one that
  neither it nor its category is picked in (`OPT_IN_ENABLED`) shows unread only with mentions
  ([user-settings.md](user-settings.md#opt-in-channels-community-onboarding)). Without opt-in
  the age doesn't matter: an old mention still shows.
- **Server Guide resource channels** (the channel flag `IS_GUILD_RESOURCE_CHANNEL`, 1 << 7) show
  neither unread nor mentions and add nothing to their server, in every server: the official
  client tracks no unreads there. Discord still reports them: on 2026-10-15 READY sent one as
  unread with an @everyone mention, which the official client showed read and never acked.
- **DMs and group DMs** show unread unless muted; every message in an unmuted one counts as a
  mention, and `unread_private_channels` lists those with mentions, newest message first,
  leaving out message requests and spam as the official client does.

`BadgeChanged { channel_id, guild_id }` comes when a channel's badge or read state changes,
`GuildBadgeChanged` when a guild's badge does; a message in a channel that is already unread
sends neither, and a channel or guild that is gone has its own event instead. A write marks
what it touched (a channel and its threads, found through an index of each channel's threads;
a whole guild for roles, the member and its settings), and the index updates those at the end
of the write. READY's badges are built before the write lock with the rest of the new state,
and a later READY reports only the badges that changed. New notification flags can move every
badge, so their badges are built from a copy of the state before the write lock too, and
rebuilt under it only if another write came in between.

A deleted channel or thread, a channel a repeated GUILD_CREATE no longer lists, and every
channel of a guild the user leaves lose their read state, in memory and, with the next flush,
in the cache. An outage keeps them: the guild's GUILD_CREATE brings no read states back.

## Threads

Observed in the official web client on 2026-10-17, with the second account and a test server.

- **What counts for the guild is what's listed:** joined, not archived, the parent viewable, and
  inside the "Hide After Inactivity" window or unread or with a mention
  ([dispatches.md](dispatches.md#threads)). A thread with an unread mention or one plain unread
  message stayed listed past its hour, also after a reload, and the server badge counted it. So
  the window never takes a dot or a count away: it only hides read threads. `BadgeChanged`
  and `ThreadListingChanged` tell a list when a thread's state or its listing changed.
- **A thread's own mute and level** come from the user's thread member (`muted`,
  `mute_config.end_time`, and `flags`: `ALL_MESSAGES` 2, `ONLY_MENTIONS` 4, `NO_MESSAGES` 8;
  `HAS_INTERACTED` 1 isn't a level). A muted thread looks like a muted channel: its row isn't
  unread, it adds no dot, and its mentions still count and show. A timed thread mute ends on its
  own, with `GuildSettingsChanged` for its guild. The level changes no badge: with "Only
  @mentions", plain messages still show the thread unread. Akari keeps the level for
  notifications.
- **A muted parent** leaves its threads alone: neither a muted channel nor a muted category
  changes their unread state, dots or badges. With "hide muted channels", a thread under a
  hidden muted channel still counts; a thread muted itself adds nothing (**unverified**).
- **Thread settings** (not sent by Akari yet): `PATCH /channels/{thread}/thread-members/@me/settings`
  with `{"flags":4}` (Only @mentions; the response's member had `flags: 5`, `muted: false`,
  `mute_config: null` and `join_timestamp`), `{"flags":2}` (back to the default),
  `{"muted":true,"mute_config":{"selected_time_window":900,"end_time":"<ISO>"}}` (15 minutes)
  and `{"muted":false}`.
- **Acks** (observed; Akari sends the same bodies): opening an unread joined thread
  acked `POST /channels/{thread}/messages/{id}/ack` with
  `{"token":null,"last_viewed":4308,"flags":2}`. Opening an unjoined thread with new messages
  acked `{"token":null,"flags":2}`, without `last_viewed`. Opening the parent also acked its
  "started a thread" message, like any message.

## The disk cache

The read states are cached per channel ([cache.md](cache.md)), so a cold start shows the
last session's badges before the gateway connects; READY then reports only what changed.
Blocked and ignored users aren't cached: they only matter for live messages, which come
after READY.

## Checking one channel

`akari-cli read-state <channel_id>` shows where a channel's badge comes from:
- READY's raw read state entry and the channel's fields;
- what Akari holds after READY, and with `--cache <file>` what a copy of the app's cache held;
- the badges, permissions and opt-in flags;
- how the newest 100 messages over REST compare with the read position.

`--watch` then follows new messages and badge changes. It prints IDs, flags and counts, never
names or message content.

## Open points

- **Acks:** what the official client does is [above](#acks-in-the-official-client), what Akari
  does [here](#how-akari-acks). Open: whether 50 is a cap per request, and what a folder's
  bulk holds.
- **Mentions in a voice channel's chat:** counted, but what the official client shows for
  them is **unverified**.
- **PASSIVE_UPDATE_V2 on the read → unread transition** is a reading of three runs, not
  documented (**unverified**).
- **CHANNEL_UNREAD_UPDATE** came in a session with `PASSIVE_GUILD_UPDATE_V2`, for an account
  without directory channels; where from is **unverified**.
- **`MENTION_ON_ALL_MESSAGES`** under a muted category isn't modeled; the test account's
  flags are 0.
