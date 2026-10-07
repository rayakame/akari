# READY

What Akari reads from the READY dispatch and which shape it expects. Sources:
[Ready](https://docs.discord.food/gateway/gateway-events#ready) and
[gateway capabilities](https://docs.discord.food/gateway/using-gateway#gateway-capabilities).

Everything here is **unverified** against real traffic. The reference has no READY
example, so `crates/akari-core/tests/fixtures/ready.json` is assembled from its tables.

## Capabilities

READY's shape depends on the `capabilities` bitfield sent in Identify. The reference
"generally assumes clients opt into all new feature capabilities", and its example Identify
sends `1734653` ([Identify](https://docs.discord.food/gateway/gateway-events#identify)).
Akari's models assume these shape-changing capabilities are set, so Identify must send
them:

| Bit | Capability | Effect on READY |
|---|---|---|
| 0 | `LAZY_USER_NOTES` | No `notes` |
| 2 | `VERSIONED_READ_STATES` | `read_state` is `{entries, partial, version}` |
| 3 | `VERSIONED_USER_GUILD_SETTINGS` | `user_guild_settings` is `{entries, partial, version}` |
| 4 | `DEDUPE_USER_OBJECTS` | User objects move to `users` and only IDs stay behind (`user_id` on members, `recipient_ids` on private channels); guild `members` move to `merged_members` |
| 5 | `PRIORITIZED_READY_PAYLOAD` | Needs bit 4. `merged_members` holds only the current user's member; the rest arrives in READY_SUPPLEMENTAL |
| 9 | `USER_SETTINGS_PROTO` | No `user_settings`; `user_settings_proto` is a base64 protobuf |
| 10 | `CLIENT_STATE_V2` | A guild's own fields move into `properties`; adds `data_mode` |

The example value also sets bits 17, 19 and 20, which the reference doesn't document.

## Fields Akari reads

| Field | Model | Notes |
|---|---|---|
| `v` | `u8` | API version |
| `user` | `CurrentUser` | Full user object |
| `users` | `Vec<User>` | Every other user the payload refers to |
| `guilds` | `Vec<GatewayGuild>` | See below |
| `merged_members` | `Vec<Vec<GuildMember>>` | Same order as `guilds`; members have `user_id`, not `user` |
| `private_channels` | `Vec<Channel>` | DMs and group DMs with `recipient_ids` |
| `session_id` | `String` | For resuming |
| `resume_gateway_url` | `String` | For resuming |

Everything else is ignored for now, notably `read_state`, `user_guild_settings`,
`relationships`, `user_settings_proto`, `sessions` and the experiments, which are
positional arrays rather than objects.

READY can also carry secrets: `analytics_token`, `auth_session_id_hash` and, with
`AUTH_TOKEN_REFRESH` (bit 8), a replacement `auth_token`. None of them is modeled, so
`Debug` can't leak them. Supporting `AUTH_TOKEN_REFRESH` later means handing the new token
to the host's token storage and never logging it.

## Guilds

With `CLIENT_STATE_V2`, a
[gateway guild](https://docs.discord.food/gateway/gateway-events#gateway-guild-object)
keeps `channels`, `threads`, `roles`, `emojis`, `stickers`, `member_count`, `joined_at`,
`large` and `premium_subscription_count` at the top level, and the guild object's own
fields in `properties`. `GatewayGuild::Available` holds `properties` as a `Guild`. A guild
without `properties` that isn't marked unavailable is an error: Identify didn't request
`CLIENT_STATE_V2`.

Guilds can be unavailable for user accounts too, during an outage or when geo-restricted:
`{id, unavailable: true}`, sometimes with `geo_restricted`, `name` and `icon`
([unavailable guild](https://docs.discord.food/gateway/gateway-events#unavailable-guild-object)).
They become `GatewayGuild::Unavailable`.

Open points, all **unverified**:

- The gateway guild table doesn't list `id`. We read the top-level `id` for unavailable
  guilds and `properties.id` otherwise.
- `data_mode` is `full`, `partial` or `unavailable`. `partial` only happens when Identify
  sends `client_state.guild_versions`, which Akari doesn't do yet, so it isn't handled.
- Whether `merged_members` has an entry for an unavailable guild. The fixture assumes an
  empty list, which keeps the arrays aligned.
- `recipient_ids` vs `recipient_id`: see [models.md](models.md#channel).
