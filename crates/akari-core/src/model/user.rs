use serde::Deserialize;

use super::int_enum::int_enum;
use super::snowflake::{GuildMarker, SkuMarker, Snowflake, UserMarker};

/// Another user as Discord sends them in messages, DMs and member lists.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct User {
    pub id: Snowflake<UserMarker>,
    pub username: String,
    /// `"0"` for users on the new username system.
    #[serde(default)]
    pub discriminator: String,
    /// The display name.
    pub global_name: Option<String>,
    pub avatar: Option<String>,
    pub avatar_decoration_data: Option<AvatarDecorationData>,
    /// The guild tag shown next to the name.
    pub primary_guild: Option<PrimaryGuild>,
    #[serde(default)]
    pub bot: bool,
    #[serde(default)]
    pub system: bool,
    pub banner: Option<String>,
    pub accent_color: Option<u32>,
    #[serde(default)]
    pub public_flags: u64,
}

/// The logged-in user, with the fields Discord only sends about yourself.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CurrentUser {
    #[serde(flatten)]
    pub user: User,
    #[serde(default)]
    pub premium_type: PremiumType,
    /// `None` while Discord doesn't know the user's age.
    pub nsfw_allowed: Option<bool>,
    #[serde(default)]
    pub mfa_enabled: bool,
    #[serde(default)]
    pub verified: bool,
    #[serde(default)]
    pub flags: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AvatarDecorationData {
    pub asset: String,
    pub sku_id: Snowflake<SkuMarker>,
    /// Unix seconds, not an ISO 8601 string.
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PrimaryGuild {
    pub identity_enabled: Option<bool>,
    pub identity_guild_id: Option<Snowflake<GuildMarker>>,
    pub tag: Option<String>,
    pub badge: Option<String>,
}

int_enum! {
    /// The user's Nitro subscription.
    #[derive(Default)]
    pub enum PremiumType {
        #[default]
        None = 0,
        /// Nitro Classic.
        Tier1 = 1,
        /// Nitro.
        Tier2 = 2,
        /// Nitro Basic.
        Tier3 = 3,
    }
}
