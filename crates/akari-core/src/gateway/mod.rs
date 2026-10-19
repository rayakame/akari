//! Gateway payloads as Discord sends them, and the connection that receives them.

mod connection;
mod decompress;
mod dispatch;
mod guild;
mod hello;
mod limiter;
mod outgoing;
mod partial;
mod payload;
mod read_state;
mod ready;
mod relationship;
pub(crate) mod session;
mod settings;
pub(crate) mod settings_proto;

#[cfg(test)]
pub(crate) use connection::fake;
pub use connection::{ConnectionEvent, DisconnectReason, Gateway, GatewayError, SendError};
pub use dispatch::{
    ChannelDelete, GuildDelete, GuildRoleDelete, GuildRoleEvent, MessageDelete, MessageDeleteBulk,
    ReadySupplemental, SupplementalGuild, ThreadListSync, ThreadMemberUpdate, ThreadMembersUpdate,
};
pub use guild::{AvailableGuild, GatewayGuild, UnavailableGuild};
pub use hello::Hello;
pub use outgoing::{GatewayCommand, GuildSubscription, MemberLists, PresenceStatus};
pub use partial::{ChannelUpdate, GuildMemberUpdate, GuildUpdate, MessageUpdate, UserUpdate};
pub use payload::{DecodeError, DispatchEvent, GatewayEvent, decode};
pub use read_state::{
    ChannelUnread, ChannelUnreadUpdate, MessageAck, PassiveUpdateV2, ReadStateEntry,
};
pub use ready::Ready;
pub use relationship::{Relationship, RelationshipRemove, RelationshipUpdate};
pub use settings::{
    ChannelOverride, MuteConfig, NotificationSettings, SettingsProto, SettingsProtoUpdate,
    UserGuildSettings, Versioned,
};
