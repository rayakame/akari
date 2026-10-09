# User settings

Two kinds of per-user settings that READY already carries and Akari doesn't read yet. Both
change what the server list and the channel list show, so until they're read Akari's lists
differ from the official client's in the ways noted below. Neither is implemented; this
file records what a later milestone needs.

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

The parts Akari will need:

| Field | Number | Contents |
|---|---|---|
| `guild_folders` | 14 | `GuildFolders`: `repeated GuildFolder folders = 1`, `repeated fixed64 guild_positions = 2` (deprecated) |
| `GuildFolder` | | `repeated fixed64 guild_ids = 1`, `Int64Value id = 2`, `StringValue name = 3`, `UInt64Value color = 4` |
| `status` | 11 | `StatusSettings`: `StringValue status = 1`, `CustomStatus custom_status = 2`, `BoolValue show_current_game = 3`, … |

- **Server list order.** The folders give the user's own order: guilds outside a folder are
  single-entry folders without an ID
  ([Guild Folders Structure](https://docs.discord.food/resources/user-settings-proto#guild-folders-structure)).
  Until Akari reads them, `Store::guild_list` puts the most recently joined guild first, so
  the server rail's order differs from the official client's for most users.
- **Status.** The chosen status lives here too, so another device's status survives Akari
  connecting (see [gateway.md](gateway.md#presence)).
- **Updates** come as `USER_SETTINGS_PROTO_UPDATE` with `{settings: {type, proto}, partial}`;
  type 1 is these preloaded settings, and `partial` means "merge with what you have"
  ([User Settings Proto Update](https://docs.discord.food/gateway/gateway-events#user-settings-proto-update)).
- **Writes** (`PATCH /users/@me/settings-proto/1`) must send the whole top-level field they
  change, or its other subfields reset to their defaults
  ([Modify User Settings Proto](https://docs.discord.food/resources/user-settings-proto#modify-user-settings-proto)).

Plan: its own small core milestone after the AkariKit view models, with a minimal decoder
for only the fields above instead of a protobuf dependency.

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
