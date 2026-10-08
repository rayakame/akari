# Dispatches

What `akari_core::gateway::decode` makes of the dispatches after READY, and what the state
store (`akari_core::state`) does with each. Sources:
[Gateway Events](https://docs.discord.food/gateway/gateway-events) and the resource pages
linked below. READY itself is in [ready.md](ready.md).

Everything here is **unverified** against real traffic unless it says otherwise. The
fixtures in `crates/akari-core/tests/fixtures/` are built from the reference's tables with
fake IDs.

## Decoding

- A dispatch Akari doesn't decode stays `DispatchEvent::Other(name)`; the store ignores it.
- A dispatch whose data fails to decode is skipped. The gateway still counts its `s`, so a
  resume doesn't replay it, and logs a warning with the event name and `s` only. serde's
  error text can quote values from the payload, so it is never logged.
- A message's `attachments`, `embeds`, `mentions`, `mention_roles`, `sticker_items` and
  `reactions` skip entries that fail to parse, so one odd embed doesn't drop a message.
- Discord may send an event more than once
  ([consistency](https://docs.discord.food/reference#consistency)). The store applies every
  dispatch idempotently: a repeated MESSAGE_CREATE, CHANNEL_CREATE or GUILD_CREATE replaces
  what it created the first time.

## Partial updates

GUILD_UPDATE, CHANNEL_UPDATE, THREAD_UPDATE, MESSAGE_UPDATE, GUILD_MEMBER_UPDATE and
USER_UPDATE decode into their own models (`GuildUpdate`, `ChannelUpdate`, `MessageUpdate`,
`GuildMemberUpdate`, `UserUpdate`), not into the full ones:

- Every field is optional except the keys (`id`; `channel_id` for messages; `guild_id` and
  `user` for members). A missing field means "unchanged".
- Nullable fields are `Option<Option<T>>`: `Some(None)` means Discord sent `null`, which
  clears the value.
- A list that is present replaces the whole list.

The reference documents most of these as full objects: GUILD_UPDATE is "a guild
object", CHANNEL_UPDATE and THREAD_UPDATE "a channel object", USER_UPDATE "a user object".
MESSAGE_UPDATE has been documented as a full message object since 2024-07 (the reference's
commit "Message updates are no longer partial" removed the note that updates "may contain
only a subset").
GUILD_MEMBER_UPDATE includes its optional fields "only if changed"
([guild member update](https://docs.discord.food/gateway/gateway-events#guild-member-update)).
Merging is correct either way. The one risk is a full object that leaves out a nullable key
to mean `null`; the store would then keep the old value.

Two details:

- GUILD_UPDATE is read flat, as documented, and also from a `properties` object, in case
  `CLIENT_STATE_V2` nests the guild's fields there as it does in READY and GUILD_CREATE.
- MESSAGE_UPDATE's `tts` isn't read: "The value for `tts` will always be `false` in message
  updates."

## What each dispatch does

| Dispatch | Model | Store |
|---|---|---|
| READY_SUPPLEMENTAL | `ReadySupplemental` | See [below](#ready_supplemental) |
| GUILD_CREATE | `GatewayGuild` | A new or returning guild: `GuildAdded` with its channels, threads and the current user's member. A known guild is replaced: `GuildUpdated`, `CurrentMemberUpdated` and channel events for what changed. An unavailable guild: `GuildUnavailable` |
| GUILD_UPDATE | `GuildUpdate` | `GuildUpdated` if something changed. Roles are replaced only if the update carries them |
| GUILD_DELETE | `GuildDelete` | With `unavailable`: an outage, `GuildUnavailable`. Without: the user left or was removed, `GuildRemoved`. Both drop the guild's channels, threads, member and message windows |
| GUILD_ROLE_CREATE, GUILD_ROLE_UPDATE | `GuildRoleEvent` | The role is added or replaced: `GuildUpdated` |
| GUILD_ROLE_DELETE | `GuildRoleDelete` | `GuildUpdated` |
| GUILD_MEMBER_UPDATE | `GuildMemberUpdate` | Only for the current user: `CurrentMemberUpdated`. Users also get it for friends and DM partners; those are skipped |
| CHANNEL_CREATE, THREAD_CREATE | `Channel` | `ChannelAdded`, or `ChannelUpdated` for a known channel. DM recipients go to the user directory (`UserUpdated` if a known one changed). A channel in a guild the store doesn't know is skipped |
| CHANNEL_UPDATE, THREAD_UPDATE | `ChannelUpdate` | `ChannelUpdated` if something changed |
| CHANNEL_DELETE, THREAD_DELETE | `ChannelDelete` | `ChannelRemoved`, also for the threads of a deleted channel. CHANNEL_DELETE "will be partial" for private channels and THREAD_DELETE has only `id`, `guild_id`, `parent_id` and `type`, so only those are read |
| MESSAGE_CREATE | `Message` | Added to the channel's window if the channel is viewed and its window reaches the newest message: `MessageInserted` |
| MESSAGE_UPDATE | `MessageUpdate` | Patches a loaded message: `MessageUpdated` |
| MESSAGE_DELETE, MESSAGE_DELETE_BULK | `MessageDelete`, `MessageDeleteBulk` | `MessageDeleted` for each loaded message |
| USER_UPDATE | `UserUpdate` | `CurrentUserUpdated` |

An update or deletion for something the store doesn't know changes nothing and is logged at
`debug` level with its IDs.

## READY_SUPPLEMENTAL

[READY_SUPPLEMENTAL](https://docs.discord.food/gateway/gateway-events#ready-supplemental)
follows READY when Identify requests `PRIORITIZED_READY_PAYLOAD`. Akari decodes:

- `guilds`: only the IDs. The supplemental guilds carry voice states and activity
  instances, which Akari doesn't model yet.
- `merged_members`: one list per guild, in the order of `guilds`. These are other users'
  members (voice users, friends, DM partners); the current user's member is in READY. They
  are decoded so a captured payload can be checked, but the store keeps only the current
  user's member. Whether they carry `user` or `user_id` isn't documented; READY_SUPPLEMENTAL
  has no `users` list.
- `lazy_private_channels`: DMs "omitted from Ready because they were already in client
  state cache". They only come when Identify names a `private_channels_version`, which
  Akari doesn't send, so this should stay empty. If they come, the store adds them
  (`ChannelAdded`).

Presences, `disclose` and `game_invites` aren't modeled. `akari-cli connect --capture` only
captures READY.

## Threads

User accounts "are only synced threads they have been added to"
([gateway guild](https://docs.discord.food/gateway/gateway-events#gateway-guild-object)).
The store keeps the joined threads from READY and whatever THREAD_CREATE brings. The full
thread list comes through THREAD_LIST_SYNC once the client subscribes to a guild, which
Akari doesn't do yet, so THREAD_LIST_SYNC isn't decoded. Losing access to a channel doesn't
send THREAD_DELETE for its threads ([threads](https://docs.discord.food/topics/threads));
they stay in the store until the next session.

## Large guilds

For user accounts, the gateway stops "sending non-stateful events for guilds without a
subscription" once a guild has more members than `large_threshold`, 250 by default
([Identify](https://docs.discord.food/gateway/gateway-events#identify-structure)). Gateway
guilds over the threshold are marked `large`. Live messages probably don't arrive in such
guilds until Akari subscribes to them with op 14 or op 37, whose payloads the reference
doesn't document. `state::Guild::large` exposes the flag.

## The state store

`akari_core::state::Store` holds an account's state in its own types, and
`akari_core::Account` keeps it current: one background task per account reads the gateway
and applies each event.

- **Reads** are synchronous from any thread and return `Arc` snapshots that later changes
  never touch. The state sits behind a `RwLock`. Writes are a few map operations. A READY
  is converted on a blocking thread before the write lock is taken, so neither readers nor
  a runtime worker wait for the conversion; comparing a later session with the old state
  does happen under the lock.
- **Events** (`StoreEvent`) carry the new value for added and updated things, and IDs for
  removed ones; never lists. They are sent while the write lock is held, so they arrive in
  the order of the changes. Each subscriber has an unbounded queue: a subscriber that stops
  reading never holds up the store, and a warning is logged at 10,000 unread events and
  every doubling. Subscribe first, then read: an event may describe a change the read
  already shows, and applying it again is harmless.
- **The account's task** ends the store's subscriptions however it stops. After
  `close()` or a fatal gateway error the connection state is `Closed` with that error; if
  the task panics or the Tokio runtime shuts down first, it is `Closed` with
  `GatewayError::Stopped` and the gateway is closed too.
- **A new session** (READY after a failed resume) is compared with the old state, and only
  the differences become events, followed by `Ready`. A guild that is added, removed or
  goes down implies its channels. RESUMED changes nothing.
- **Members:** only the current user's, for permissions. **Users:** the DM and group DM
  recipients; message authors are copies sent with each message, and equal copies share
  one allocation.
- **Permissions** for a guild channel or thread follow
  [Permissions](https://docs.discord.food/topics/permissions): owner and `ADMINISTRATOR`
  get everything; otherwise `@everyone` and the member's roles, then the `@everyone`, role
  and member overwrites. Threads use their parent's overwrites, and there `SEND_MESSAGES`
  stands for `SEND_MESSAGES_IN_THREADS`. A timeout leaves only `VIEW_CHANNEL` and
  `READ_MESSAGE_HISTORY`, AutoMod quarantine also `CHANGE_NICKNAME`. Without `VIEW_CHANNEL`
  nothing is left. READY sends channels the user can't see, and the official client hides
  them.

### Message windows

- `Account::view_channel` gives a channel a window: one contiguous run of messages, at most
  200. At most 10 channels keep one; viewing an eleventh drops the least recently viewed
  window (`MessagesCleared`). Messages for channels without a window aren't kept.
- `latest` means the window ends with the channel's newest message, and only then are live
  messages appended. `oldest` means it starts with the channel's first message.
- Trimming never removes what the user is reading. A live append past the limit drops the
  oldest messages. A batch of older history past the limit drops the newest and sets
  `latest` to false, like Discord's "jump to present"; live messages then wait until newer
  history brings the window back to the present. Every trim emits `MessagesTrimmed`.
- A new session marks every window stale (`MessagesStale`) instead of clearing it, so a UI
  keeps its scroll position. Edits and deletes still apply, but new messages are held back
  until the window is refreshed and reconciled, because appending them could leave a gap.

## Open points

- **Author nicknames.** Messages keep only their author as Discord sent it. Server
  nicknames will come from a member cache keyed by (guild, user) that resolves display
  names, filled by op 8 member requests and MESSAGE_CREATE's `member`. MESSAGE_CREATE's
  `member` isn't kept until then, so live and loaded messages show the same names.
- **Unread and mention badges.** They need an event for MESSAGE_CREATEs in channels
  without a window: the channel, the message ID, and whether it mentions the current user
  (directly, through a role, or `@everyone`). CHANNEL_UPDATE isn't sent when
  `last_message_id` changes, so tracking it belongs there too. Every MESSAGE_CREATE
  reaches the store; only the window step drops those for channels without a window, so
  the event goes in front of it.
- **Guild subscriptions** (op 14 or op 37) for live messages in large guilds, and
  THREAD_LIST_SYNC with them.
- **Refreshing stale windows** and loading history come with the REST client.
