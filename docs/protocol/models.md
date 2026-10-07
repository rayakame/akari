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
