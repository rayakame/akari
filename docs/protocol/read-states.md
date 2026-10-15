# Read states

Where the user stopped reading each channel, how many mentions wait there, and the unread
dots and red counts Akari draws from them. The main reference is the
[read state topic](https://docs.discord.food/topics/read-state); where it is silent, Akari
follows the official desktop client as observed on 2026-10-14 (the UI side is in
[server-list.md](../ui/server-list.md#what-makes-a-server-unread) and
[channel-list.md](../ui/channel-list.md#what-makes-a-row-unread)). Akari doesn't acknowledge
anything yet: it shows what the user read elsewhere, and its own messages.

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
aren't kept: Akari has no pins view, and sends no acks yet.

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
- **A thread** uses its guild's join time.

A read state the store starts this way (for a DM's first message, or a guild channel's first
mention) is marked `estimated`: its `acked` is the join or the message before, not a message
the user read. An ack or the user's own message replaces it with a real one.

The newest message only moves forward: from MESSAGE_CREATE, CHANNEL_CREATE, CHANNEL_UPDATE
and GUILD_CREATE (an older or missing `last_message_id` changes nothing), and from the unread
updates of guilds without a subscription. Only READY replaces it. A new thread in a forum or
media channel is the forum's newest "message": THREAD_CREATE raises the parent's
`last_message_id` to the thread's ID. A thread's own messages move only the thread, so an
unjoined thread leaves its parent read; the "started a thread" message in the parent is an
ordinary message there.

## MESSAGE_ACK

- **The position** moves forward. With `manual` (mark as unread) it can move back.
- **The count:** `mention_count` replaces it. Without one it stays, and drops to 0 once the
  ack reaches the channel's newest message (discord.py-self and concord do the same).
- An ack for a channel without a read state starts one. Acks of other read state types are
  ignored.

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
  joined, unarchived threads that the user can view; a thread's mentions count even before
  Akari learns that the mention added the user to it. Its dot needs one of them unread and
  not under a muted category or muted channel, and the guild itself unmuted; its count adds
  up all their mentions, muted or not. With "hide muted channels", muted channels don't count
  at all. A folder's badge is its guilds' sum, added up by the host.
- **Community servers with opt-in channels** ("Channels & Roles": the `COMMUNITY` feature and
  the user's `OPT_IN_CHANNELS_ON`) follow the official client's two extra rules: a channel
  whose newest message is over 7 days old shows neither unread nor mentions, and one that
  neither it nor its category is picked in (`OPT_IN_ENABLED`) shows unread only with mentions
  ([user-settings.md](user-settings.md#opt-in-channels-community-onboarding)). Without them,
  old mentions there showed in Akari, and reading the channel in the official client never
  cleared them: that client sends no ack for a channel it shows as read.
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

- **Acks:** Akari doesn't acknowledge channels yet, nor mark them read.
- **Mentions in a voice channel's chat:** counted, but what the official client shows for
  them is **unverified**.
- **Joining or leaving a thread elsewhere** (THREAD_MEMBER_UPDATE) isn't decoded; such a
  thread counts for its guild from the next READY.
- **PASSIVE_UPDATE_V2 on the read → unread transition** is a reading of three runs, not
  documented (**unverified**).
- **CHANNEL_UNREAD_UPDATE** came in a session with `PASSIVE_GUILD_UPDATE_V2`, for an account
  without directory channels; where from is **unverified**.
- **`MENTION_ON_ALL_MESSAGES`** under a muted category, and a thread's own mute, aren't
  modeled; the test account's flags are 0.
