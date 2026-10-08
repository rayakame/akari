use serde::de::{self, Deserialize, Deserializer};
use serde_json::value::RawValue;

use crate::lenient::{JsonErrorSummary, skip_invalid};
use crate::model::{Channel, Guild, GuildMarker, GuildMember, Role, Snowflake, Timestamp};

/// A guild in READY or GUILD_CREATE.
///
/// A guild that fails to parse becomes `Unavailable` instead of failing the whole payload.
/// Decodes only from JSON text (`serde_json::from_str`/`from_slice`), not from a
/// `serde_json::Value`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayGuild {
    Available(Box<AvailableGuild>),
    /// Down because of an outage, or blocked in the user's region.
    Unavailable(UnavailableGuild),
}

impl GatewayGuild {
    pub fn id(&self) -> Snowflake<GuildMarker> {
        match self {
            Self::Available(guild) => guild.properties.id,
            Self::Unavailable(guild) => guild.id,
        }
    }
}

/// A guild in the layout the `CLIENT_STATE_V2` capability produces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvailableGuild {
    pub properties: Guild,
    pub channels: Vec<Channel>,
    /// Only threads the user has joined.
    pub threads: Vec<Channel>,
    pub roles: Vec<Role>,
    /// GUILD_CREATE's members: the current user's on join. Empty in READY, which has
    /// `merged_members` instead.
    pub members: Vec<GuildMember>,
    pub member_count: Option<u32>,
    pub joined_at: Option<Timestamp>,
    pub large: bool,
    /// Number of boosts.
    pub premium_subscription_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnavailableGuild {
    pub id: Snowflake<GuildMarker>,
    pub geo_restricted: bool,
}

#[derive(serde::Deserialize)]
struct RawGatewayGuild {
    id: Option<Snowflake<GuildMarker>>,
    #[serde(default)]
    unavailable: bool,
    #[serde(default)]
    geo_restricted: bool,
    properties: Option<Guild>,
    #[serde(default, deserialize_with = "skip_invalid")]
    channels: Vec<Channel>,
    #[serde(default, deserialize_with = "skip_invalid")]
    threads: Vec<Channel>,
    #[serde(default)]
    roles: Vec<Role>,
    #[serde(default, deserialize_with = "skip_invalid")]
    members: Vec<GuildMember>,
    member_count: Option<u32>,
    joined_at: Option<Timestamp>,
    #[serde(default)]
    large: bool,
    #[serde(default)]
    premium_subscription_count: u32,
}

impl TryFrom<RawGatewayGuild> for GatewayGuild {
    type Error = &'static str;

    fn try_from(raw: RawGatewayGuild) -> Result<Self, Self::Error> {
        if raw.unavailable {
            let id = raw.id.ok_or("unavailable guild has no `id`")?;
            return Ok(Self::Unavailable(UnavailableGuild {
                id,
                geo_restricted: raw.geo_restricted,
            }));
        }
        let properties = raw
            .properties
            .ok_or("guild has no `properties`; was CLIENT_STATE_V2 requested?")?;
        Ok(Self::Available(Box::new(AvailableGuild {
            properties,
            channels: raw.channels,
            threads: raw.threads,
            roles: raw.roles,
            members: raw.members,
            member_count: raw.member_count,
            joined_at: raw.joined_at,
            large: raw.large,
            premium_subscription_count: raw.premium_subscription_count,
        })))
    }
}

impl<'de> Deserialize<'de> for GatewayGuild {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = <&RawValue>::deserialize(deserializer)?;
        let parsed = serde_json::from_str::<RawGatewayGuild>(raw.get())
            .map_err(|err| JsonErrorSummary(&err).to_string())
            .and_then(|guild| Self::try_from(guild).map_err(str::to_owned));
        match parsed {
            Ok(guild) => Ok(guild),
            Err(err) => {
                let id = fallback_id(raw).ok_or_else(|| {
                    de::Error::custom(format!("guild without an id failed to parse: {err}"))
                })?;
                tracing::warn!(
                    guild_id = id.get(),
                    error = %err,
                    "guild failed to parse, treating it as unavailable"
                );
                Ok(Self::Unavailable(UnavailableGuild {
                    id,
                    geo_restricted: false,
                }))
            }
        }
    }
}

// The gateway guild table doesn't document a top-level `id`, so `properties.id` backs it up.
fn fallback_id(raw: &RawValue) -> Option<Snowflake<GuildMarker>> {
    #[derive(serde::Deserialize)]
    struct TopLevel {
        id: Snowflake<GuildMarker>,
    }

    #[derive(serde::Deserialize)]
    struct Nested {
        properties: TopLevel,
    }

    serde_json::from_str::<TopLevel>(raw.get())
        .map(|guild| guild.id)
        .or_else(|_| serde_json::from_str::<Nested>(raw.get()).map(|guild| guild.properties.id))
        .ok()
}
