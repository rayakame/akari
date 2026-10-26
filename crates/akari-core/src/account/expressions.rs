use std::future::Future;

use super::Account;
use crate::RequestError;
use crate::error::{TransportError, TransportErrorKind};
use crate::model::{EmojiId, StickerId, StickerPackId};
use crate::state::{EmojiSource, SourceGuild, StickerDetails, StickerPack};

// What a custom emoji's or a sticker's popout shows, asked as the official client asks it.
impl Account {
    /// The server or app a custom emoji is from. Discord answers a server only while it's
    /// discoverable; for the user's own servers, [`crate::state::Store::emoji_guild`] knows.
    /// Given 5 s, as the official client gives it: a later lookup doesn't wait behind a slow one.
    pub async fn emoji_source(&self, emoji: EmojiId) -> Result<EmojiSource, RequestError> {
        let answer = self.within(self.shared.rest.emoji_source(emoji)).await;
        self.lookup("emoji_source", answer).map(EmojiSource::from)
    }

    /// A sticker, and its pack or server. Given 5 s like the emoji's source, as are the two below.
    pub async fn sticker(&self, sticker: StickerId) -> Result<StickerDetails, RequestError> {
        let answer = self.within(self.shared.rest.sticker(sticker)).await;
        self.lookup("sticker", answer).map(StickerDetails::from)
    }

    /// The discoverable server a sticker is from, with its stickers.
    pub async fn sticker_guild(&self, sticker: StickerId) -> Result<SourceGuild, RequestError> {
        let answer = self.within(self.shared.rest.sticker_guild(sticker)).await;
        self.lookup("sticker_guild", answer).map(SourceGuild::from)
    }

    /// One of Discord's sticker packs, with its stickers.
    pub async fn sticker_pack(&self, pack: StickerPackId) -> Result<StickerPack, RequestError> {
        let answer = self.within(self.shared.rest.sticker_pack(pack)).await;
        self.lookup("sticker_pack", answer).map(StickerPack::from)
    }

    // Dropping the request at the limit lets its rate-limit lane go.
    async fn within<T>(
        &self,
        request: impl Future<Output = Result<T, RequestError>>,
    ) -> Result<T, RequestError> {
        tokio::time::timeout(self.shared.lookup_timeout, request)
            .await
            .unwrap_or_else(|_| {
                Err(RequestError::Network(TransportError::new(
                    TransportErrorKind::Timeout,
                    "lookup took too long",
                )))
            })
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
