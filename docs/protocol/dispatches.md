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
  `reactions` skip entries that fail to parse, so one odd embed doesn't drop a message. A
  `nonce` that is neither a string nor a non-negative integer becomes `None`.
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
| GUILD_CREATE | `GatewayGuild` | A new or returning guild: `GuildAdded` with its channels, threads and the current user's member. A known guild is replaced: `GuildUpdated`, `CurrentMemberUpdated` and channel events for what changed; a known channel keeps its `last_message_id` if that is newer. An unavailable guild: `GuildUnavailable` |
| GUILD_UPDATE | `GuildUpdate` | `GuildUpdated` if something changed (including `default_message_notifications`). Roles are replaced only if the update carries them |
| GUILD_DELETE | `GuildDelete` | With `unavailable`: an outage, `GuildUnavailable`. Without: the user left or was removed, `GuildRemoved`. Both drop the guild's channels, threads, member and message windows |
| GUILD_ROLE_CREATE, GUILD_ROLE_UPDATE | `GuildRoleEvent` | The role is added or replaced: `GuildUpdated` |
| GUILD_ROLE_DELETE | `GuildRoleDelete` | `GuildUpdated` |
| GUILD_MEMBER_UPDATE | `GuildMemberUpdate` | Only for the current user: `CurrentMemberUpdated`. Users also get it for friends and DM partners; those are skipped |
| CHANNEL_CREATE, THREAD_CREATE | `Channel` | `ChannelAdded`, or `ChannelUpdated` for a known channel. A thread with the user's `member` is joined ([Threads](#threads)). DM recipients go to the user directory (`UserUpdated` if a known one changed). A channel in a guild the store doesn't know is skipped. A known channel keeps its `last_message_id` if that is newer. A thread in a forum or media channel also raises its parent's `last_message_id` to the thread's ID, without an event: a new post is the forum's newest "message" |
| CHANNEL_UPDATE, THREAD_UPDATE | `ChannelUpdate` | `ChannelUpdated` if something changed. `last_message_id` only moves forward; an older ID or `null` leaves it |
| CHANNEL_DELETE, THREAD_DELETE | `ChannelDelete` | `ChannelRemoved`, also for the threads of a deleted channel. CHANNEL_DELETE "will be partial" for private channels and THREAD_DELETE has only `id`, `guild_id`, `parent_id` and `type`, so only those are read |
| THREAD_MEMBER_UPDATE | `ThreadMemberUpdate` | The user's own thread member: the thread's membership, mute and level are replaced, `ChannelUpdated` ([Threads](#threads)) |
| THREAD_MEMBERS_UPDATE | `ThreadMembersUpdate` | The user's ID in `added_members` joins the thread, in `removed_member_ids` leaves it: `ChannelUpdated`. Other members and `member_count` are ignored |
| MESSAGE_CREATE | `Message` | Counts toward the channel's read state first: a message of the current user reads the channel up to it (not a poll result), and others can raise its mention count ([read-states.md](read-states.md)). Added to the channel's window if the channel is viewed and its window reaches the newest message: `MessageInserted`. One whose nonce matches a pending message of ours replaces it: `MessageReplaced` ([messages.md](messages.md#sending)). Then it raises the channel's `last_message_id`: a DM or group DM gets `ChannelUpdated`, after the window's events; a guild channel or thread changes without an event. A message of the current user that isn't the echo of a send that started the channel's slowmode cooldown (one from another device) starts that cooldown ([messages.md](messages.md#slowmode)) |
| MESSAGE_UPDATE | `MessageUpdate` | Patches a loaded message: `MessageUpdated` |
| MESSAGE_DELETE, MESSAGE_DELETE_BULK | `MessageDelete`, `MessageDeleteBulk` | `MessageDeleted` for each loaded message |
| USER_UPDATE | `UserUpdate` | `CurrentUserUpdated` |
| USER_GUILD_SETTINGS_UPDATE | `UserGuildSettings` | A full settings object for one guild (`guild_id: null` for DMs) replaces that entry: `GuildSettingsChanged` if a value changed. `collapsed` isn't read ([user-settings.md](user-settings.md#user-guild-settings)) |
| USER_SETTINGS_PROTO_UPDATE | `SettingsProtoUpdate` | Type 1 only: the folders change as [user-settings.md](user-settings.md#the-settings-proto) describes, `ServerListChanged` if the server list did. A proto that doesn't decode changes nothing |
| NOTIFICATION_SETTINGS_UPDATE | `NotificationSettings` | Replaces the account's notification flags; only the badges that change send events |
| MESSAGE_ACK | `MessageAck` | Moves the channel's read state ([read-states.md](read-states.md)): `BadgeChanged`, and `GuildBadgeChanged` if the guild's badge changed. Acks of other read state types are ignored |
| CHANNEL_UNREAD_UPDATE, PASSIVE_UPDATE_V2 | `ChannelUnreadUpdate`, `PassiveUpdateV2` | Raise each listed channel's `last_message_id`, forward only, without a channel event; a channel that turns unread sends `BadgeChanged` ([read-states.md](read-states.md#badges)). A channel the store doesn't know, or one of another guild, is skipped ([Large guilds](#large-guilds)) |
| RELATIONSHIP_ADD, RELATIONSHIP_UPDATE, RELATIONSHIP_REMOVE | `Relationship`, `RelationshipUpdate`, `RelationshipRemove` | Keep the blocked and ignored users, whose messages never count as mentions. ADD also announces a changed type, such as a friend becoming blocked; UPDATE never does |

An update or deletion for something the store doesn't know changes nothing and is logged at
`debug` level with its IDs. A change that moves a badge (a message, an ack, settings, roles,
the member, a channel's overwrites) also sends `BadgeChanged` and `GuildBadgeChanged` after its
own events ([read-states.md](read-states.md#badges)).

GUILD_CREATE and GUILD_DELETE (and READY's diff) also send `ServerListChanged` after their own
events when the server list changed: a guild joined, left, went down or came back.

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
The store keeps the joined threads from READY and whatever THREAD_CREATE brings.

**Membership.** `ThreadInfo::member` is the user's membership: its mute (`muted`,
`mute_config.end_time`) and its notification level from the member's `flags`
([read-states.md](read-states.md#threads)). It changes live:
- READY's and GUILD_CREATE's threads are joined, with their `member`'s mute and level, or a
  default membership without one.
- THREAD_CREATE with `member` joins. Discord sends it also when the user is added to a
  thread, for example by a mention: the official client lists that thread at once (observed
  2026-10-17). Without `member`, a known thread keeps its membership.
- THREAD_MEMBER_UPDATE replaces the membership. THREAD_MEMBERS_UPDATE joins when it adds the
  user and leaves when it removes them; everything else in it is ignored. THREAD_MEMBER_LIST_UPDATE
  (op 37's member lists) never changes membership.
- Updates for a thread the store doesn't know are skipped.

Members decode leniently: a field in another shape falls back to its default, and a broken
entry in `added_members` is skipped, so payload drift doesn't drop the dispatch.

**What the channel list shows.** `Store::channel_list` puts a thread right after its parent,
newest created first, when the user joined it, it isn't archived, its parent is viewable, and
it is inside its "Hide After Inactivity" window, unread, or has a mention. The window ends
`auto_archive_duration` minutes after the later of its newest message and its
`archive_timestamp` (the [threads](https://docs.discord.food/topics/threads) topic); pinned
forum posts (channel flag `PINNED`, 1 << 1) have none. The official client keeps an unread
thread past its window (observed 2026-10-17: a thread with an unread mention and one with a
plain unread message stayed listed past their hour, also after a reload, and went once read).
The guild's badge counts exactly the listed threads. Neither the parent's mute nor "hide muted
channels" changes what's listed; which listed threads a list shows under a collapsed category
or a muted parent is the apps' rule ([channel-list.md](../ui/channel-list.md#threads)).

The account's deadline task wakes at the earliest window end, as it does for the mute ends.
`ThreadListingChanged { guild_id, thread_id }` reports a thread whose listing changed while the
thread itself didn't: its window ended, it was read past it, or a message came after it.

**Joining and leaving** (observed 2026-10-17; not sent by Akari yet):
`POST /channels/{thread}/thread-members/@me?location=…` joins (the docs say PUT; the official
client sends POST) and `DELETE` on the same path leaves, without a body, answered 204.
`location` names where the user chose it: `Sidebar Overflow` (the "…" of a thread beside its
parent), `Toolbar Overflow` (the "…" of a thread opened full width), `Context Menu` (a thread
row's menu, leave only).

The full thread list comes through THREAD_LIST_SYNC once the client subscribes to a guild with
`threads: true`. Akari's guild subscriptions ask for it, but THREAD_LIST_SYNC isn't decoded
yet, so the store ignores it. Losing access to a channel doesn't send THREAD_DELETE for its
threads ([threads](https://docs.discord.food/topics/threads)); they stay in the store, no longer
listed, until the next session.

## Large guilds

The reference says the gateway stops "sending non-stateful events for guilds without a
subscription" once a guild has more members than `large_threshold`, 250 by default
([Identify](https://docs.discord.food/gateway/gateway-events#identify-structure)). Gateway
guilds over the threshold are marked `large`, and `state::Guild::large` exposes the flag. In
practice the line is higher: guilds of up to 75,000 members are subscribed automatically on
connect (discord.py-self), and an observation on 2026-10-15 agrees
([gateway.md](gateway.md#guild-subscriptions)). Akari subscribes every guild with a viewed
channel, like the official client, so live messages arrive in the biggest ones too.

Identify sends `PASSIVE_GUILD_UPDATE_V2` (bit 14), so guilds without a subscription send
[PASSIVE_UPDATE_V2](https://docs.discord.food/gateway/gateway-events#passive-update-v2)
"periodically" instead of CHANNEL_UNREAD_UPDATE, which then comes only for directory
channels. It carries only what changed: `updated_channels` (each channel's `id`,
`last_message_id` and `last_pin_timestamp`), voice states and their members. Akari decodes
`guild_id` and each channel's `id` and `last_message_id`, skipping a channel entry that
doesn't parse, and the store raises those channels' `last_message_id`.

Observed on 2026-10-15 with akari-cli on a test account:

- Only the account's server over 75,000 members sent PASSIVE_UPDATE_V2, also when Akari had
  sent no op 37 at all. The servers of 250 to 75,000 members sent none.
- With that server's channels marked read just before, it came 4 times in 20 minutes, 4 to 6
  minutes apart, and once in a second run. In an earlier run, with all its channels already
  unread, none came in 10 minutes. So it most likely comes when a channel goes from read to
  unread, not for every message (**unverified**). A channel then gets no further update until
  it has been read again, so the update must never be undone by anything but an ack or READY.
- CHANNEL_UNREAD_UPDATE came twice in a 10-minute run with bit 14 set and not at all in a
  20-minute one, although the account has no directory channels. Where it came from is
  **unverified**.

## Read states

The store keeps READY's channel read states and changes them with MESSAGE_ACK, MESSAGE_CREATE
and the unread updates, and keeps every channel's and guild's badge as it goes; the rules are
in [read-states.md](read-states.md).

## The state store

`akari_core::state::Store` holds an account's state in its own types, and
`akari_core::Account` keeps it current: one background task per account reads the gateway
and applies each event.

- **Reads** are synchronous from any thread and return `Arc` snapshots that later changes
  never touch. The state sits behind a `RwLock`, and UIs read it from their main thread, so
  writes stay short. A READY is converted on a blocking thread and compared with the old
  state under a read lock before the write lock is taken; the old state is dropped after
  the lock is released. A GUILD_CREATE converts its guild before taking the lock. A write
  that holds the lock for more than 4 ms logs a warning with the event's name (never
  payload values); on the test fixtures the longest hold is well under 0.1 ms.
- **Events** (`StoreEvent`) carry the new value for added and updated things, and IDs for
  removed ones; never lists. They are sent while the write lock is held, so they arrive in
  the order of the changes. Each subscriber has an unbounded queue: a subscriber that stops
  reading never holds up the store, and a warning is logged at 10,000 unread events and
  every doubling. Subscribe first, then read: an event may describe a change the read
  already shows, and applying it again is harmless. `Subscription::next_batch` takes up to
  a given number of buffered events at once, so a host across an FFI boundary pays one
  round trip per batch.
- **The account's task** ends the store's subscriptions however it stops. After
  `close()` or a fatal gateway error the connection state is `Closed` with that error; if
  the task panics or the Tokio runtime shuts down first, it is `Closed` with
  `GatewayError::Stopped` and the gateway is closed too.
- **A new session** (READY after a failed resume) is compared with the old state, and only
  the differences become events, followed by `Ready`. A guild that is added, removed or
  goes down implies its channels. RESUMED changes nothing.
- **The disk cache** can fill the store before the first READY (`StoreEvent::Restored`;
  read everything, as after READY). The first READY after that is compared with the
  restored state like a new session, so only real changes become events, but it doesn't
  mark windows stale: windows loaded since the restore are as fresh as READY. A READY that
  arrives before the cache was read wins, and the cached state isn't used. See
  [cache.md](cache.md).
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
  `Account` refreshes stale windows right after READY; a window that is current again sends
  `MessagesRefreshed`, also when the refresh found nothing new. See
  [messages.md](messages.md#after-a-new-session).
- A channel's first page can show the cache's copy of the channel while it loads: if the
  copy is read before the page arrives (or the page fails), it fills the empty window as
  stale (`MessagesLoaded`, then `MessagesStale`), and the page then reconciles it like a
  refresh or, when it doesn't reach it, replaces it with the present. A page never waits
  for the disk.
- `MessagesLoaded { first, last }` announces messages added within that ID range: history
  at either end, or messages a refresh filled in between loaded ones. The UI reads the
  window for the range.
- Our own pending and failed messages sit in the window's outbox, after its messages, and
  are never trimmed ([messages.md](messages.md#sending)). A window with an outbox is never
  evicted; the next least recently viewed one goes instead. When its channel goes away
  (deleted, left, or its guild down), the window loses its messages but keeps the outbox,
  so the text can still be retried, discarded or offered to the user again.

### Lists for display

Three reads give lists in the order a UI shows them. They are display rules every app needs,
so they live in the core.

- **`Store::guild_list`**: the server list. The user's own order and folders are in the
  settings proto, which Akari doesn't read yet ([user-settings.md](user-settings.md)).
  Until then the most recently joined guild comes first, where the official client puts a
  newly joined server, then guilds without a known join date, by ID. **Unverified**: where
  the official client puts guilds missing from the user's saved order.
- **`Store::channel_list`**: `display_order`, keeping only channels with `VIEW_CHANNEL`,
  each followed by its listed threads ([Threads](#threads)). A category shows when it holds at least one visible channel; an empty
  category shows when the user can view it, because servers use empty categories as
  dividers. A channel whose permissions can't be computed (for example without the
  current user's member) is hidden. **Unverified**: the category rule; to be compared with
  the official client in the macOS app's manual check. Servers with opt-in channels show
  more channels than the official client, which hides those the user didn't pick
  ([user-settings.md](user-settings.md#opt-in-channels-community-onboarding)).
- **`Store::private_channel_list`**: the DM list. The latest conversation comes first, by
  `last_message_id`, or by the channel's own ID (its creation time) when it has none; ties
  by channel ID, newest first. The official client sorts the same way after the user's
  favorites, which are in the settings proto. Like it, Akari leaves out message requests
  and conversations marked as spam (`is_message_request`, `is_spam` from READY,
  CHANNEL_CREATE and CHANNEL_UPDATE; accepting a request brings the DM back).

Every channel keeps `last_message_id` from READY, CHANNEL_CREATE, CHANNEL_UPDATE and
MESSAGE_CREATE. In forum and media channels it holds the newest post's thread ID, so its
type is a generic snowflake. Only DMs and group DMs report a new value with `ChannelUpdated`,
because their list order depends on it. Guild channels change silently, and a later READY's
diff doesn't report a guild channel whose only change is its last message, so a reconnect
doesn't bring an event for every active channel.

## Open points

- **Previewing (lurking) guilds.** A guild the user previews without joining isn't in
  READY, so its channels aren't in the store, and viewing one does nothing: no window
  events and no subscription. akari-cli's `tail` warns about it.
- **Author nicknames.** Messages keep only their author as Discord sent it. Server
  nicknames will come from a member cache keyed by (guild, user) that resolves display
  names, filled by op 8 member requests and MESSAGE_CREATE's `member`. MESSAGE_CREATE's
  `member` isn't kept until then, so live and loaded messages show the same names.
- **Unread and mention badges.** They need an event for MESSAGE_CREATEs in guild channels
  without a window: the channel, the message ID, and whether it mentions the current user
  (directly, through a role, or `@everyone`). Each channel's `last_message_id` is already
  tracked; only that event is missing.
- **THREAD_LIST_SYNC**, which guild subscriptions bring, for the full thread list.
