# User settings

Two kinds of per-user settings come with READY: the settings proto (server order and
folders, among much else) and the user guild settings (mutes, notification levels,
suppress settings). The state store reads both and keeps them current from the gateway.
Akari writes neither.

## The settings proto

With the `USER_SETTINGS_PROTO` capability (bit 9, which Akari sends), READY has no
`user_settings`; `user_settings_proto` carries the settings as a base64
`PreloadedUserSettings` protobuf instead, and a missing field means "use the defaults"
([List of Capabilities](https://docs.discord.food/gateway/using-gateway#list-of-capabilities),
[Ready](https://docs.discord.food/gateway/gateway-events#ready)). The message is
`discord_protos.discord_users.v1.PreloadedUserSettings`
([Preloaded User Settings Object](https://docs.discord.food/resources/user-settings-proto#preloaded-user-settings-object));
the docs give the field order, and the field numbers come from the proto definitions they
link (discord-userdoccers/discord-protos, `PreloadedUserSettings.proto`).

Akari reads two fields with its own small reader (`gateway::settings_proto`, no protobuf
dependency) and skips everything else unread:

| Field | Number | Contents |
|---|---|---|
| `versions` | 1 | `Versions`: `data_version = 3` (uint32) |
| `guild_folders` | 14 | `GuildFolders`: `repeated GuildFolder folders = 1`, `repeated fixed64 guild_positions = 2` (deprecated, not read) |
| `GuildFolder` | | `repeated fixed64 guild_ids = 1` (packed or not), `Int64Value id = 2`, `StringValue name = 3`, `UInt64Value color = 4` |

A wrapper that is present but empty means 0 or "", an absent one `None`. Input that doesn't
decode (bad base64, a length past the end, a group, an unknown wire type, a name that
isn't UTF-8) is an error naming only a byte offset.

**The server list** (`Store::server_list`) follows the folders:

- An entry with an `id` is a folder: its available guilds in order, its name (`None` when
  empty: the app shows the guilds' names) and color (0xRRGGBB, `None` when unset). A folder
  whose guilds are all gone isn't listed.
- An entry without an `id` places its guilds, one by one, outside any folder
  ([Guild Folders Structure](https://docs.discord.food/resources/user-settings-proto#guild-folders-structure)).
- Guilds no entry places (joined since) come first, the newest join first: the official
  client shows a new server at the very top (observed 2026-10-14). Unknown and unavailable
  guilds are skipped, and a guild listed twice shows once.
- Without the field, the list is the join order, newest first.

`Store::guild_list` is the same list with each folder's guilds in its place. A change sends
`ServerListChanged`, also when a guild is joined, left or goes down.

- **Updates** come as `USER_SETTINGS_PROTO_UPDATE` with `{settings: {type, proto}, partial}`;
  type 1 is these preloaded settings, and `partial` means "merge with what you have"
  ([User Settings Proto Update](https://docs.discord.food/gateway/gateway-events#user-settings-proto-update)).
  Akari replaces the folders when the update carries field 14, keeps them when a partial
  one doesn't, and clears them when a full one doesn't. Other types are ignored.
- **A proto that doesn't decode** changes nothing: READY keeps the folders the store had
  (from the last session or the disk cache), an update is skipped. Both log a warning with
  the byte offset, never the bytes.
- **Status.** The chosen status lives here too (field 11), so another device's status
  survives Akari connecting (see [gateway.md](gateway.md#presence)). Not read yet.
- **Writes** (`PATCH /users/@me/settings-proto/1`) must send the whole top-level field they
  change, or its other subfields reset to their defaults
  ([Modify User Settings Proto](https://docs.discord.food/resources/user-settings-proto#modify-user-settings-proto)).
  Akari doesn't write the proto.

## User guild settings

READY's `user_guild_settings` (`{entries, partial, version}` with
`VERSIONED_USER_GUILD_SETTINGS`, bit 3) has an entry per guild the user changed something
in (and for guilds the user has left), plus one with `guild_id: null` for DMs when the user
changed those ([User Guild Settings](https://docs.discord.food/resources/user-settings#user-guild-settings-object)).
`USER_GUILD_SETTINGS_UPDATE` replaces one entry. The store keeps each as `GuildSettings`
(`Store::guild_settings`), and a change sends `GuildSettingsChanged`:

- **Mutes:** a guild or a channel is muted while `muted` and its `mute_config.end_time` is
  unset or later than now (`muted_now`). Discord sends nothing when a timed mute runs out,
  so the account keeps one timer for the earliest end and sends `GuildSettingsChanged` for
  the guild then. It checks again after a reconnect, in case the computer slept past an end. `Store::channel_muted` is a channel's own override
  only: in the official client a muted category's channels look normal, and a muted server's
  icon isn't dimmed (observed 2026-10-14).
- **Levels:** `message_notifications` 0 all messages, 1 only mentions, 2 nothing, 3 inherit.
  `Store::notification_level` walks the channel's override, its category's (a thread's: its
  channel's, then the category's), the guild's, then the guild's
  `default_message_notifications`. A guild without an entry has Discord's defaults.
- **Unreads:** the `UNREADS_ALL_MESSAGES` and `UNREADS_ONLY_MENTIONS` flags (guild 1 << 11 and
  1 << 12, channel 1 << 10 and 1 << 9) become `unreads`.
- `suppress_everyone`, `suppress_roles` and `hide_muted_channels` are kept as sent. The
  suppress settings decide what counts toward a mention badge, not the highlighted row: the
  official client still tints a suppressed @everyone or role mention (observed 2026-10-14).
- **Category collapse isn't synced.** `channel_overrides[].collapsed` exists, but the
  official clients don't write it: collapsing or expanding a category in the web or the
  desktop client sends no request, and categories collapsed on the phone were expanded on
  the web (observed 2026-10-14). Akari ignores the field and keeps collapse in the app,
  per server. Don't add a sync for it.
- **How the official clients write levels** (observed 2026-10-14, for the follow-up that
  adds settings writes): the bulk route `PATCH /users/@me/guilds/settings` with only the
  changed field, e.g. `{"guilds":{"<guild>":{"channel_overrides":{"<channel>":{"message_notifications":0}}}}}`
  or `{"guilds":{"<guild>":{"message_notifications":1}}}`.

`notification_settings.flags` (`USE_NEW_NOTIFICATIONS` 1 << 4, `MENTION_ON_ALL_MESSAGES`
1 << 5) is kept from READY and NOTIFICATION_SETTINGS_UPDATE, without an event.

## Opt-in channels (community onboarding)

Servers with onboarding let members pick channels; the official client then shows only the
picked channels and the server's defaults, plus a "Show All Channels" switch. Akari's
`Store::channel_list` shows every channel the user can view, so on such servers it lists
more channels than the official client. The manual checks on large community servers will
show the difference.

Where the state lives:

- **The guild** has the `GUILD_ONBOARDING` feature (also `GUILD_ONBOARDING_EVER_ENABLED`,
  `GUILD_ONBOARDING_HAS_PROMPTS`); there is no flag bit for it
  ([Guild Features](https://docs.discord.food/resources/guild#guild-features)).
- **READY's `user_guild_settings`** (a `{entries, partial, version}` object with
  `VERSIONED_USER_GUILD_SETTINGS`, bit 3) has one entry per guild
  ([User Guild Settings Flags](https://docs.discord.food/resources/user-settings#user-guild-settings-flags),
  [Channel Override Flags](https://docs.discord.food/resources/user-settings#channel-override-flags)):
  - the entry's `flags`: `OPT_IN_CHANNELS_OFF` (1 << 13, show all channels) and
    `OPT_IN_CHANNELS_ON` (1 << 14, hide channels that aren't opted in);
  - `channel_overrides[].flags`: `OPT_IN_ENABLED` (1 << 12, the channel is shown), next to
    `FAVORITED` (1 << 11) and the unread settings.
- **The member's flags** say whether onboarding was started or completed
  (`STARTED_ONBOARDING` 1 << 3, `COMPLETED_ONBOARDING` 1 << 1;
  [Guild Member Flags](https://docs.discord.food/resources/guild#guild-member-flags)); the
  current user's member is in READY's `merged_members`.
- **The onboarding itself** (prompts, options with their `channel_ids`,
  `default_channel_ids`) is REST only: `GET /guilds/{guild.id}/onboarding`
  ([Get Guild Onboarding](https://docs.discord.food/resources/guild#get-guild-onboarding)).
  Picking options writes `OPT_IN_ENABLED` overrides, which arrive as
  `USER_GUILD_SETTINGS_UPDATE`
  ([User Guild Settings Update](https://docs.discord.food/gateway/gateway-events#user-guild-settings-update)).
- **Never listed:** channels with the flag `IS_GUILD_RESOURCE_CHANNEL` (1 << 7) or
  `IS_SCHEDULED_FOR_DELETION` (1 << 9)
  ([Channel Flags](https://docs.discord.food/resources/channel#channel-flags)). Akari
  doesn't filter these yet either.

The docs don't state the official client's rule. **Unverified**, to check on the second
account before implementing: opt-in applies when the guild has `GUILD_ONBOARDING` and the
entry has `OPT_IN_CHANNELS_ON`; a channel shows when it or its category has
`OPT_IN_ENABLED`, and channels `@everyone` can't view, the selected channel, favorites and
channels with unread mentions show regardless.
