use akari_core::model::{AttachmentId, ChannelId, GuildId, MessageId, Permissions, UserId};

uniffi::custom_type!(UserId, u64, {
    remote,
    lower: |id| id.get(),
    try_lift: |raw| Ok(UserId::new(raw)),
});

uniffi::custom_type!(GuildId, u64, {
    remote,
    lower: |id| id.get(),
    try_lift: |raw| Ok(GuildId::new(raw)),
});

uniffi::custom_type!(ChannelId, u64, {
    remote,
    lower: |id| id.get(),
    try_lift: |raw| Ok(ChannelId::new(raw)),
});

uniffi::custom_type!(MessageId, u64, {
    remote,
    lower: |id| id.get(),
    try_lift: |raw| Ok(MessageId::new(raw)),
});

uniffi::custom_type!(AttachmentId, u64, {
    remote,
    lower: |id| id.get(),
    try_lift: |raw| Ok(AttachmentId::new(raw)),
});

uniffi::custom_type!(Permissions, u64, {
    remote,
    lower: |permissions| permissions.0,
    try_lift: |raw| Ok(Permissions(raw)),
});
