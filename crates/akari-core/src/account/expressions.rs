use super::Account;
use crate::RequestError;
use crate::model::{EmojiId, StickerId, StickerPackId};
use crate::state::{EmojiSource, SourceGuild, StickerDetails, StickerPack};

// What a custom emoji's or a sticker's popout shows, asked as the official client asks it.
impl Account {
    /// The server or app a custom emoji is from. Discord answers a server only while it's
    /// discoverable; for the user's own servers, [`crate::state::Store::emoji_guild`] knows.
    pub async fn emoji_source(&self, emoji: EmojiId) -> Result<EmojiSource, RequestError> {
        self.lookup("emoji_source", self.shared.rest.emoji_source(emoji).await)
            .map(EmojiSource::from)
    }

    /// A sticker, and its pack or server.
    pub async fn sticker(&self, sticker: StickerId) -> Result<StickerDetails, RequestError> {
        self.lookup("sticker", self.shared.rest.sticker(sticker).await)
            .map(StickerDetails::from)
    }

    /// The discoverable server a sticker is from, with its stickers.
    pub async fn sticker_guild(&self, sticker: StickerId) -> Result<SourceGuild, RequestError> {
        self.lookup(
            "sticker_guild",
            self.shared.rest.sticker_guild(sticker).await,
        )
        .map(SourceGuild::from)
    }

    /// One of Discord's sticker packs, with its stickers.
    pub async fn sticker_pack(&self, pack: StickerPackId) -> Result<StickerPack, RequestError> {
        self.lookup("sticker_pack", self.shared.rest.sticker_pack(pack).await)
            .map(StickerPack::from)
    }

    fn lookup<T>(
        &self,
        what: &'static str,
        answer: Result<T, RequestError>,
    ) -> Result<T, RequestError> {
        if let Err(err) = &answer {
            self.shared.failed(err);
            tracing::debug!(target: "akari_core::load", lookup = what, error = err.kind(), "lookup failed");
        }
        answer
    }
}
