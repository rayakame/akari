use serde::Deserialize;

use crate::model::{Channel, Guild, Role, Snowflake, Timestamp};

/// A guild in READY or GUILD_CREATE.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(try_from = "RawGatewayGuild")]
pub enum GatewayGuild {
    Available(Box<AvailableGuild>),
    /// Down because of an outage, or blocked in the user's region.
    Unavailable(UnavailableGuild),
}

impl GatewayGuild {
    pub fn id(&self) -> Snowflake {
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
    pub member_count: Option<u32>,
    pub joined_at: Option<Timestamp>,
    pub large: bool,
    /// Number of boosts.
    pub premium_subscription_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnavailableGuild {
    pub id: Snowflake,
    pub geo_restricted: bool,
}

#[derive(Deserialize)]
struct RawGatewayGuild {
    id: Option<Snowflake>,
    #[serde(default)]
    unavailable: bool,
    #[serde(default)]
    geo_restricted: bool,
    properties: Option<Guild>,
    #[serde(default)]
    channels: Vec<Channel>,
    #[serde(default)]
    threads: Vec<Channel>,
    #[serde(default)]
    roles: Vec<Role>,
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
            member_count: raw.member_count,
            joined_at: raw.joined_at,
            large: raw.large,
            premium_subscription_count: raw.premium_subscription_count,
        })))
    }
}
