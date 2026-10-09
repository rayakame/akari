# READY

What Akari reads from the READY dispatch and which shape it expects. Sources:
[Ready](https://docs.discord.food/gateway/gateway-events#ready) and
[gateway capabilities](https://docs.discord.food/gateway/using-gateway#gateway-capabilities).

The reference has no READY example, so `crates/akari-core/tests/fixtures/ready.json` is
assembled from its tables. On 2026-10-08 a real READY, received with the capabilities
below, decoded completely ([Checking a real READY](#checking-a-real-ready) passed): no guild,
channel, thread, member, user or private channel was skipped. The open points below are
still **unverified** unless they say otherwise.

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
positional arrays rather than objects. What `user_settings_proto` and `user_guild_settings`
hold, and what ignoring them costs, is in [user-settings.md](user-settings.md).

The state store takes the current user, the guilds with their channels, threads and roles,
the current user's member from `merged_members`, the private channels, and from `users`
only the DM and group DM recipients. READY_SUPPLEMENTAL, which follows READY, is described
in [dispatches.md](dispatches.md#ready_supplemental).

READY can also carry secrets: `analytics_token`, `auth_session_id_hash` and, with
`AUTH_TOKEN_REFRESH` (bit 8), a replacement `auth_token`. None of them is modeled, so
`Debug` can't leak them. Supporting `AUTH_TOKEN_REFRESH` later means handing the new token
to the host's token storage and never logging it.

## Guilds

With `CLIENT_STATE_V2`, a
[gateway guild](https://docs.discord.food/gateway/gateway-events#gateway-guild-object)
keeps `channels`, `threads`, `roles`, `emojis`, `stickers`, `member_count`, `joined_at`,
`large` and `premium_subscription_count` at the top level, and the guild object's own
fields in `properties`. `GatewayGuild::Available` holds `properties` as a `Guild`. The same
object arrives as GUILD_CREATE, which also carries a top-level `members` list (the current
user's member when they join); READY has `merged_members` instead. A guild
without `properties` that isn't marked unavailable means Identify didn't request
`CLIENT_STATE_V2`; it is treated like any guild that fails to parse (see below).

Guilds can be unavailable for user accounts too, during an outage or when geo-restricted:
`{id, unavailable: true}`, sometimes with `geo_restricted`, `name` and `icon`
([unavailable guild](https://docs.discord.food/gateway/gateway-events#unavailable-guild-object)).
They become `GatewayGuild::Unavailable`.

Open points (questions about the reference are **unverified**):

- The gateway guild table doesn't list `id`. We read the top-level `id` for unavailable
  guilds and `properties.id` otherwise. A guild that fails to parse takes its ID from the
  top level, or from `properties` if the top level has no readable `id`.
- `data_mode` is `full`, `partial` or `unavailable`. `partial` only happens when Identify
  sends `client_state.guild_versions`, which Akari doesn't do yet, so it isn't handled.
- Whether `merged_members` has an entry for an unavailable guild. The fixture assumes an
  empty list, which keeps the arrays aligned.
- `recipient_ids` vs `recipient_id`: see [models.md](models.md#channel).
- Roles are parsed strictly, so one broken role makes its whole guild unavailable. If that
  shows up in real traffic, the alternative is to skip broken roles, mark the guild as
  degraded (for example an `incomplete_roles` flag on `AvailableGuild`) and compute its
  permissions conservatively. Not implemented.

## When parts of READY don't parse

A third-party client can't control when Discord changes a payload, and one unexpected
value must not lock the user out. Below the top level, decoding degrades instead of
failing:

| Part | On a parse error |
|---|---|
| A guild | Becomes `GatewayGuild::Unavailable` with its `id`, so `merged_members` stays aligned |
| A guild channel or thread | Skipped |
| A private channel | Skipped |
| A member in `merged_members` | Skipped; the outer list keeps one entry per guild |
| An entry in `users` | Skipped; whatever refers to that ID shows an unknown user |
| `null` instead of one of these lists, or of a member list | Treated as empty; `merged_members` keeps one entry per guild |

Each case logs a `tracing` warning with the entry's ID and the serde error's category, line
and column. serde's message itself isn't logged, because it can quote a value from the
payload. Roles stay strict, because a missing role would silently change
computed permissions: a broken role makes its guild unavailable. Errors in `user`,
`session_id`, `resume_gateway_url` or the envelope still fail READY, as does a broken
guild with no readable `id` at the top level or in `properties`.

These parts are parsed from borrowed raw JSON, so `GatewayGuild` and `Ready` decode from
JSON text only (`serde_json::from_str`/`from_slice`), not from a `serde_json::Value`. Each
guild is scanned twice: once as raw JSON, then into the model.

## Checking a real READY

The fixture is assembled, not captured. To check the models against a real payload,
capture one with akari-cli (after `akari-cli login`):

```sh
cargo run -p akari-cli -- connect --capture
```

It connects with the capabilities above (Identify sends exactly them, see
[gateway.md](gateway.md#identify)), saves the decompressed READY message (the whole
`{"op": 0, "t": "READY", …}` object) as `captures/ready-<unix time>.json` with mode 0600,
prints its absolute path and closes the session. Then run the ignored test on it. The path
must be absolute, because cargo runs the test from `crates/akari-core`:

```sh
AKARI_READY_FIXTURE="$PWD/captures/ready-<unix time>.json" \
  cargo test -p akari-core --test gateway -- --ignored captured_ready_decodes_completely
```

It fails if a guild that the payload doesn't mark unavailable decodes as unavailable, if
`merged_members` doesn't have one entry per guild, or if any user, private channel, member,
guild channel or thread is skipped. Its own messages name only guild IDs, and when a strict
part of READY fails, the panic shows only where: the error's category, line and column
(`DecodeError` keeps no serde message, so no value from the payload).

A second ignored test builds the state store from the same capture:

```sh
AKARI_READY_FIXTURE="$PWD/captures/ready-<unix time>.json" \
  cargo test -p akari-core --lib -- --ignored captured_ready_builds_the_store --nocapture
```

It fails if an available guild, channel or thread is missing from the store, if a guild
becomes unavailable that READY didn't mark, if a private channel's recipient doesn't
resolve through `Store::user`, if a guild has no member for the current user, or if
`Store::permissions` returns `None` for a guild channel or thread. Its messages name only
guild and channel IDs. With `--nocapture` it prints counts and nothing else: guilds,
channels, visible channels (with `VIEW_CHANNEL`), threads and DMs.

A captured READY contains secrets (`analytics_token`, `auth_session_id_hash`) and personal
data. `captures/` is git-ignored; still never commit or share a capture, and delete it when
you're done.
