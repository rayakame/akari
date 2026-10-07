# Data models

How the serde models in `akari_core::model` map the objects Discord sends: what we took
from the reference, how it maps to Rust, and where the reference disagrees with its own
examples. The field tables themselves live in the reference.

Everything here is **unverified** against real traffic. The fixtures in
`crates/akari-core/tests/fixtures/` are built from the reference's tables and examples,
with fake IDs and names.

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
- **Snowflakes** (`Snowflake`) are strings, but also parse from JSON integers: when an ID
  was sent as an integer, Discord can echo it back as one
  ([ID serialization](https://docs.discord.food/reference#id-serialization)).
- **Timestamps** (`Timestamp`) are RFC 3339 strings with no or six fractional digits, such
  as `2023-02-17T19:52:19.184000+00:00`
  ([ISO8601](https://docs.discord.food/reference#iso8601-datetime)).
  `Timestamp::unix_millis` returns milliseconds since the Unix epoch.
- **Permissions** (`Permissions`) are decimal strings because they outgrow 53 bits. The
  highest documented bit is `1 << 53`, so `u64` is enough for now
  ([permissions](https://docs.discord.food/topics/permissions)).
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
- `last_message_id` may point to a message that no longer exists.
- `thread_metadata.create_timestamp` is missing for threads created before 2022-01-09
  ([thread metadata](https://docs.discord.food/resources/channel#thread-metadata-structure)).
- Not modeled: forum and media fields (`available_tags`, `applied_tags`,
  `default_reaction_emoji`, sort order, layout), voice fields (`rtc_region`,
  `video_quality_mode`, `status`), group DM `nicks`, `safety_warnings`, the thread `member`.
