# Data models

How the serde models in `akari_core::model` map the objects Discord sends: what we took
from the reference, how it maps to Rust, and where the reference disagrees with its own
examples. The field tables themselves live in the reference.

Everything here is **unverified** against real traffic. The fixtures in
`crates/akari-core/tests/fixtures/` are built from the reference's tables and examples,
with fake IDs and names.

## Wire layer only

`akari_core::model` and the payload types in `akari_core::gateway` (`Ready`,
`GatewayGuild`, `Hello` and so on) mirror what Discord sends and nothing else. They are
never handed to a UI and never stored in the SQLite cache. The state layer
(`akari_core::state`, see [dispatches.md](dispatches.md#the-state-store)) has its own
memory-efficient types and converts from the wire models, so wire types carry no UI or
storage concerns: no display helpers, no cache keys, no derives for storage. The modules are `pub` for now because `gateway::decode` returns these types
and the integration tests read them; the boundary is a rule, not yet enforced by
visibility.

Value types are the exception: `Snowflake<M>` with its markers and the ID aliases
(`UserId`, `GuildId`, `ChannelId`, `MessageId`, …), `Timestamp`, `Permissions` and the
integer enums (`ChannelType`, `MessageType`, `OverwriteType`, `PremiumType`,
`MessageReferenceType`, `StickerFormatType`) are shared vocabulary the state layer reuses,
so its IDs stay typed too. `Snowflake` wraps a plain `u64` because the wire format can carry
ID 0 (reportedly in read states; **unverified**). The state layer keeps it: a
`NonZeroU64`-backed ID would save 8 bytes per `Option` (about 100 KB at 10,000 channels) but
needs a second ID type.

## Conventions

The reference marks a field that may be absent with `?` after the name, and a field that
may be `null` with `?` before the type
([reference](https://docs.discord.food/reference#nullable-and-optional-resource-fields)).

| Reference | Rust |
|---|---|
| `field?: type`, `field: ?type`, `field?: ?type` | `Option<T>`; a missing key and `null` both become `None` |
| `field?: boolean`, `field?: array[…]`, flags | `#[serde(default)]`: `false`, empty, `0` |
| `field: type` | `T`, unless the reference's own examples omit the field; then `Option<T>` or a default |

- Unknown keys are ignored. Discord adds fields all the time, so no model uses
  `deny_unknown_fields`.
- Only the fields Akari needs are modeled. Each section lists what is left out.
- The models only implement `Deserialize`; nothing is sent back to Discord yet.
- **Snowflakes** (`Snowflake<M>`) are typed by what they identify: `Snowflake<UserMarker>`,
  `Snowflake<GuildMarker>`, `Snowflake<ChannelMarker>` and so on, so a channel ID can't be
  passed where a guild ID is expected. Permission overwrites, whose ID is a role or a
  member depending on `type`, use `Snowflake<GenericMarker>`; `Snowflake::cast` converts
  deliberately, for example a guild ID into its `@everyone` role ID. On the wire IDs are
  strings, but they also parse from JSON integers: when an ID was sent as an integer,
  Discord can echo it back as one
  ([ID serialization](https://docs.discord.food/reference#id-serialization)).
- **Timestamps** (`Timestamp`) are RFC 3339 strings with no or six fractional digits, such
  as `2023-02-17T19:52:19.184000+00:00`
  ([ISO8601](https://docs.discord.food/reference#iso8601-datetime)).
  `Timestamp::unix_millis` returns milliseconds since the Unix epoch.
- **Permissions** (`Permissions`) are decimal strings because they outgrow 53 bits. The
  highest documented bit is `1 << 53`, so `u64` is enough for now
  ([permissions](https://docs.discord.food/topics/permissions)). `Permissions` has
  constants for the bits Akari uses and the bit operators; unknown bits are kept.
- **Flags** are JSON integers and stay raw `u64`. User flags already reach bit 51.
- **Integer enums** (channel type, message type, …) get an `Unknown(u16)` variant, so a
  value Discord adds later doesn't fail the payload around it. The reference lists removed
  values that still turn up, and new ones keep appearing.
- **String enums** that Discord extends often (embed type, guild features) stay `String`.

## User

Sources: [user object](https://docs.discord.food/resources/user#user-structure),
[partial user](https://docs.discord.food/resources/user#partial-user-structure).

- `User` is the partial user: message authors, mentions, DM recipients, members and
  READY's `users`. `CurrentUser` is the full user object READY sends as `user`. It
  flattens a `User` and adds `premium_type`, `nsfw_allowed`, `mfa_enabled`, `verified`
  and `flags`.
- READY's `user` is parsed strictly, so a failure there fails READY. `CurrentUser`
  therefore requires only `id` and `username`, which every user object has. Everything
  else falls back to a default; `premium_type` becomes `None` (no Nitro).
- `discriminator` defaults to an empty string: recipients of an invite's channel carry only
  `id`, `username` and `avatar`
  ([partial channel](https://docs.discord.food/resources/channel#partial-channel-structure)).
  Migrated users have `"0"`, webhook authors `"0000"`.
- `avatar_decoration_data.expires_at` is Unix seconds, not ISO 8601
  ([avatar decoration data](https://docs.discord.food/resources/user#avatar-decoration-data-structure)).
- `primary_guild` is the guild tag shown next to the name. Its fields are `null` until the
  user reaffirms their identity after a tag change
  ([primary guild](https://docs.discord.food/resources/user#primary-guild-structure)).
- `PremiumType`: 0 none, 1 Nitro Classic, 2 Nitro, 3 Nitro Basic
  ([premium type](https://docs.discord.food/resources/user#premium-type)).
- The reference marks `age_verification_status` as required, but its own example user
  doesn't have it. It isn't modeled.
- Not modeled: `collectibles`, `display_name_styles`, `premium_state`, `bio`, `pronouns`,
  `purchased_flags`, `premium_usage_flags`. `email` and `phone` are left out on purpose, so
  personal data can't reach logs through `Debug`.

## Channel

Source: [channel object](https://docs.discord.food/resources/channel#channel-structure).

- One `Channel` struct covers every channel type, as the reference does. Which fields are
  set depends on `kind`: DMs have `recipients` and no `guild_id`, `name` or `position`;
  group DMs add `name`, `icon` and `owner_id`; threads have `parent_id`, `owner_id` and
  `thread_metadata`. The reference has no per-type table; this comes from its examples.
- `ChannelType` follows the [channel type table](https://docs.discord.food/resources/channel#channel-type).
  The removed types 7–9 fall into `Unknown`. Thread types 10–12 need API v9 or later.
- `recipients` vs `recipient_ids`: with `DEDUPE_USER_OBJECTS`, READY replaces recipient
  user objects with IDs. Footnote 6 of the
  [Ready structure](https://docs.discord.food/gateway/gateway-events#ready-structure)
  calls the field `recipient_ids`, the
  [capability table](https://docs.discord.food/gateway/using-gateway#list-of-capabilities)
  says `recipient_id`, and the channel table lists neither. We parse `recipient_ids` as an
  array. **Unverified.**
- Permission overwrites: `allow` and `deny` are permission strings, `type` is 0 for a role
  and 1 for a member
  ([permission overwrite](https://docs.discord.food/resources/channel#permission-overwrite-structure)).
- `last_message_id` is a `Snowflake<GenericMarker>`: in forum and media channels it is the
  last thread created, in directory channels the last entry, otherwise the last message. It
  may point to something that no longer exists.
- `thread_metadata.create_timestamp` is missing for threads created before 2022-01-09
  ([thread metadata](https://docs.discord.food/resources/channel#thread-metadata-structure)).
- Not modeled: forum and media fields (`available_tags`, `applied_tags`,
  `default_reaction_emoji`, sort order, layout), voice fields (`rtc_region`,
  `video_quality_mode`, `status`), group DM `nicks`, `safety_warnings`, the thread `member`.

## Guild

Sources: [guild object](https://docs.discord.food/resources/guild#guild-structure),
[role](https://docs.discord.food/resources/guild#role-structure),
[guild member](https://docs.discord.food/resources/guild#guild-member-structure).

- `Guild` is the guild object without `roles`, `emojis`, `stickers` and
  `premium_subscription_count`. That is what READY puts in a gateway guild's `properties`
  under `CLIENT_STATE_V2` (see [ready.md](ready.md)); the rest sits next to it.
- Only `id` and `name` are required; a guild is meaningless without them. Everything else
  has a default, so one missing field can't make a whole guild unavailable in READY:
  `owner_id` and `afk_timeout` become `None` (the reference documents no default for the
  timeout), `preferred_locale` becomes `en-US` (its documented default), the notification
  level `AllMessages`, the NSFW level `Default` and the boost tier `None`.
- `features` stays `Vec<String>`. The [feature list](https://docs.discord.food/resources/guild#guild-features)
  is "subject to arbitrary change", and payloads still carry removed features such as
  `THREADS_ENABLED`.
- The reference marks `owner_configured_content_level`,
  `premium_progress_bar_enabled_user_updated_at` and `official_message_color` as required,
  but its example guild omits them. None of them is modeled.
- `MessageNotificationLevel` 2 and 3 only occur in user guild settings
  ([notification level](https://docs.discord.food/resources/guild#message-notification-level)).
- Roles require only `id`, `position` and `permissions`. `name` defaults to empty,
  `hoist`, `managed` and `mentionable` to `false`, and colors to 0.
- `colors` supersedes the deprecated `color`, but the role in the reference's example
  guild has no `colors`, so it is an `Option`. `tags` isn't modeled yet. A tag key that is
  present with a `null` value means `true`, and a missing key means `false`
  ([role tags](https://docs.discord.food/resources/guild#role-tags-structure)), so it will
  need a custom deserializer.
- Members: `user` is missing on the member inside MESSAGE_CREATE, and deduplicated READY
  payloads replace it with `user_id`. `GuildMember` carries both as `Option`. `joined_at`
  is an `Option` too, as a precaution, although the reference marks it required.
- Not modeled: emojis, stickers, soundboard sounds, verification and MFA levels, the
  explicit content filter, system channel flags, member `avatar_decoration_data`,
  `collectibles`, `display_name_styles`, `bio`, `unusual_dm_activity_until`, `permissions`.

## Message

Sources: [message object](https://docs.discord.food/resources/message#message-structure),
[MESSAGE_CREATE extra fields](https://docs.discord.food/gateway/gateway-events#message-object-extra-fields).

- One `Message` struct parses REST responses and MESSAGE_CREATE. MESSAGE_UPDATE has its
  own partial model (see [dispatches.md](dispatches.md#partial-updates)). The gateway adds
  `guild_id`, `member` (without `user`), `channel_type` and a `member` key on each mention;
  only `guild_id` and `member` are modeled.
- Everything except `id`, `channel_id`, `author`, `timestamp` and `type` falls back to a
  default when missing.
- `attachments`, `embeds`, `mentions`, `mention_roles`, `sticker_items` and `reactions` skip
  entries that fail to parse, so one odd embed doesn't drop the whole message. Like gateway
  guilds, a `Message` therefore decodes from JSON text only (`serde_json::from_str`/
  `from_slice`), not from a `serde_json::Value`.
- `author` isn't a real user when `webhook_id` is set. Webhook authors have discriminator
  `"0000"`.
- `referenced_message` has three states (footnote 5 of the message table): a missing key
  means Discord didn't try to fetch it, `null` means the message was deleted, and an
  object is the referenced message. The model uses `Option<Option<Box<Message>>>`.
- `message_reference.type` is 0 (`DEFAULT`) when missing; 1 (`FORWARD`) comes with
  `message_snapshots`, which isn't modeled yet
  ([message reference](https://docs.discord.food/resources/message#message-reference-structure)).
- `MessageType` has every type of the
  [message type table](https://docs.discord.food/resources/message#message-type) that
  isn't struck out. The removed types 13, 33, 34, 43, 45, 56, 57 and 63 fall into
  `Unknown`.
- Embed `type` stays a string: 15 kinds are documented, and some are system-only
  ([embed type](https://docs.discord.food/resources/message#embed-type)). `image`,
  `thumbnail` and `video` share one media structure.
- Reactions: `count` is normal and burst reactions together, and `burst_colors` are hex
  strings like `"#f0ca59"`
  ([reaction](https://docs.discord.food/resources/message#reaction-structure)). A deleted
  custom emoji has `name: null`.
- `nonce` (an integer or a string) isn't modeled; matching our sends to their echoes comes
  with the REST client.
- Not modeled: `message_snapshots`, `thread`, `components`, `poll`, `call`, `activity`,
  `application`, `interaction_metadata`, `resolved`, `role_subscription_data`, `stickers`,
  `mention_channels`, `potions`, `shared_client_theme`.

## Open points

Not implemented yet; each belongs to a later milestone.

- **Bitflag types.** Flags are raw `u64`, and `Permissions` only has constants for the bits
  Akari uses. Both could become `bitflags` types built with `from_bits_retain`, so bits
  Akari doesn't know yet survive.
- **`Serialize` for outgoing payloads.** Identify, heartbeats, message sends and every other
  payload Akari sends need `Serialize`; the models are deserialize-only today.
