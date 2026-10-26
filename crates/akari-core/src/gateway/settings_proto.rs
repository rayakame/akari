//! The fields Akari reads from Discord's `PreloadedUserSettings` proto: `versions` (1),
//! `text_and_images` (6) and `guild_folders` (14). Everything else is skipped unread.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;

use crate::model::{GuildId, Snowflake};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PreloadedSettings {
    /// `None` when the proto has no `versions`.
    pub(crate) data_version: Option<u32>,
    /// `None` when the proto has no `guild_folders`.
    pub(crate) guild_folders: Option<GuildFolders>,
    /// `None` when the proto has no `text_and_images`.
    pub(crate) text_and_images: Option<TextAndImages>,
}

/// `text_and_images` (6) as sent: an absent field is `None` and means Discord's default.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct TextAndImages {
    /// StringValue 4: ALWAYS, ON_CLICK or IF_MODERATOR.
    pub(crate) render_spoilers: Option<String>,
    pub(crate) view_image_descriptions: Option<bool>,
    pub(crate) inline_attachment_media: Option<bool>,
    pub(crate) inline_embed_media: Option<bool>,
    pub(crate) gif_auto_play: Option<bool>,
    pub(crate) render_embeds: Option<bool>,
    pub(crate) animate_emoji: Option<bool>,
    /// UInt32Value 15: 0 always, 1 on interaction, 2 never.
    pub(crate) animate_stickers: Option<u32>,
    /// UInt32Value 19: 0 disabled, 1 non-friends, 2 all.
    pub(crate) explicit_content_filter: Option<u32>,
    pub(crate) explicit_content: Option<Redactions>,
    pub(crate) gore: Option<Redactions>,
    pub(crate) self_harm: Option<Redactions>,
}

/// An `ExplicitContentSettings` message: a redaction per context, 0 unset, 1 show, 2 blur,
/// 3 block.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Redactions {
    pub(crate) guilds: u32,
    pub(crate) friend_dm: u32,
    pub(crate) non_friend_dm: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct GuildFolders {
    pub(crate) folders: Vec<GuildFolder>,
}

/// A folder, or a single guild's place when it has no `id`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct GuildFolder {
    pub(crate) guild_ids: Vec<GuildId>,
    pub(crate) id: Option<i64>,
    pub(crate) name: Option<String>,
    /// 0xRRGGBB.
    pub(crate) color: Option<u64>,
}

/// Offsets count bytes after base64 decoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ProtoError {
    #[error("not base64")]
    Base64,
    #[error("ends early at byte {offset}")]
    Truncated { offset: usize },
    #[error("malformed at byte {offset}")]
    Malformed { offset: usize },
}

pub(crate) fn decode_preloaded(base64: &str) -> Result<PreloadedSettings, ProtoError> {
    let bytes = STANDARD.decode(base64).map_err(|_| ProtoError::Base64)?;
    let mut reader = Reader::new(&bytes, 0);
    let mut settings = PreloadedSettings::default();
    while let Some((number, wire, at)) = reader.tag()? {
        match (number, wire) {
            (1, Wire::Len) => {
                let version = settings.data_version.unwrap_or(0);
                settings.data_version = Some(versions(reader.message()?, version)?);
            }
            // A message field that appears twice is merged, which appends the folders.
            (14, Wire::Len) => guild_folders(
                reader.message()?,
                settings.guild_folders.get_or_insert_with(Default::default),
            )?,
            (6, Wire::Len) => text_and_images(
                reader.message()?,
                settings
                    .text_and_images
                    .get_or_insert_with(Default::default),
            )?,
            (1 | 6 | 14, _) => return Err(ProtoError::Malformed { offset: at }),
            (_, wire) => reader.skip(wire)?,
        }
    }
    Ok(settings)
}

fn versions(mut reader: Reader<'_>, mut data_version: u32) -> Result<u32, ProtoError> {
    while let Some((number, wire, at)) = reader.tag()? {
        match (number, wire) {
            // A uint32 that doesn't fit is truncated, as protobuf does.
            (3, Wire::Varint) => data_version = reader.varint()? as u32,
            (1..=3, Wire::Varint) => reader.skip(wire)?,
            (1..=3, _) => return Err(ProtoError::Malformed { offset: at }),
            (_, wire) => reader.skip(wire)?,
        }
    }
    Ok(data_version)
}

fn guild_folders(mut reader: Reader<'_>, into: &mut GuildFolders) -> Result<(), ProtoError> {
    while let Some((number, wire, at)) = reader.tag()? {
        match (number, wire) {
            (1, Wire::Len) => into.folders.push(guild_folder(reader.message()?)?),
            (1, _) => return Err(ProtoError::Malformed { offset: at }),
            (_, wire) => reader.skip(wire)?,
        }
    }
    Ok(())
}

fn guild_folder(mut reader: Reader<'_>) -> Result<GuildFolder, ProtoError> {
    let mut folder = GuildFolder::default();
    while let Some((number, wire, at)) = reader.tag()? {
        match (number, wire) {
            (1, Wire::Len) => {
                let mut packed = reader.message()?;
                while !packed.at_end() {
                    folder.guild_ids.push(Snowflake::new(packed.fixed64()?));
                }
            }
            (1, Wire::Fixed64) => folder.guild_ids.push(Snowflake::new(reader.fixed64()?)),
            // Int64Value: two's complement, not zigzag.
            (2, Wire::Len) => folder.id = Some(wrapped_varint(reader.message()?)? as i64),
            (3, Wire::Len) => folder.name = Some(wrapped_string(reader.message()?)?),
            (4, Wire::Len) => folder.color = Some(wrapped_varint(reader.message()?)?),
            (1..=4, _) => return Err(ProtoError::Malformed { offset: at }),
            (_, wire) => reader.skip(wire)?,
        }
    }
    Ok(folder)
}

fn text_and_images(mut reader: Reader<'_>, into: &mut TextAndImages) -> Result<(), ProtoError> {
    while let Some((number, wire, at)) = reader.tag()? {
        let known = matches!(number, 4 | 7 | 9..=12 | 14 | 15 | 19 | 29 | 32 | 35);
        if known && wire != Wire::Len {
            return Err(ProtoError::Malformed { offset: at });
        }
        // A repeated sub-message merges into the earlier one, as protobuf reads it.
        match number {
            4 => into.render_spoilers = Some(wrapped_string(reader.message()?)?),
            7 => merge_bool(&mut into.view_image_descriptions, reader.message()?)?,
            9 => merge_bool(&mut into.inline_attachment_media, reader.message()?)?,
            10 => merge_bool(&mut into.inline_embed_media, reader.message()?)?,
            11 => merge_bool(&mut into.gif_auto_play, reader.message()?)?,
            12 => merge_bool(&mut into.render_embeds, reader.message()?)?,
            14 => merge_bool(&mut into.animate_emoji, reader.message()?)?,
            15 => merge_u32(&mut into.animate_stickers, reader.message()?)?,
            19 => merge_u32(&mut into.explicit_content_filter, reader.message()?)?,
            29 => merge_redactions(&mut into.explicit_content, reader.message()?)?,
            32 => merge_redactions(&mut into.gore, reader.message()?)?,
            35 => merge_redactions(&mut into.self_harm, reader.message()?)?,
            _ => reader.skip(wire)?,
        }
    }
    Ok(())
}

fn merge_bool(into: &mut Option<bool>, reader: Reader<'_>) -> Result<(), ProtoError> {
    let prior = into.map(u64::from);
    *into = Some(wrapped_varint_over(reader, prior)? != 0);
    Ok(())
}

// A uint32 that doesn't fit is truncated, as protobuf does.
fn merge_u32(into: &mut Option<u32>, reader: Reader<'_>) -> Result<(), ProtoError> {
    let prior = into.map(u64::from);
    *into = Some(wrapped_varint_over(reader, prior)? as u32);
    Ok(())
}

fn merge_redactions(
    into: &mut Option<Redactions>,
    mut reader: Reader<'_>,
) -> Result<(), ProtoError> {
    // Three enum fields, plain varints; absent means 0 (unset).
    let mut value = into.unwrap_or_default();
    while let Some((number, wire, at)) = reader.tag()? {
        match (number, wire) {
            (1, Wire::Varint) => value.guilds = reader.varint()? as u32,
            (2, Wire::Varint) => value.friend_dm = reader.varint()? as u32,
            (3, Wire::Varint) => value.non_friend_dm = reader.varint()? as u32,
            (1..=3, _) => return Err(ProtoError::Malformed { offset: at }),
            (_, wire) => reader.skip(wire)?,
        }
    }
    *into = Some(value);
    Ok(())
}

// Int64Value and UInt64Value: field 1, absent meaning 0.
fn wrapped_varint(reader: Reader<'_>) -> Result<u64, ProtoError> {
    wrapped_varint_over(reader, None)
}

// A wrapper merged over an earlier value of the same field: field 1 replaces it when present.
fn wrapped_varint_over(mut reader: Reader<'_>, prior: Option<u64>) -> Result<u64, ProtoError> {
    let mut value = prior.unwrap_or(0);
    while let Some((number, wire, at)) = reader.tag()? {
        match (number, wire) {
            (1, Wire::Varint) => value = reader.varint()?,
            (1, _) => return Err(ProtoError::Malformed { offset: at }),
            (_, wire) => reader.skip(wire)?,
        }
    }
    Ok(value)
}

// StringValue: field 1, absent meaning "".
fn wrapped_string(mut reader: Reader<'_>) -> Result<String, ProtoError> {
    let mut value = String::new();
    while let Some((number, wire, at)) = reader.tag()? {
        match (number, wire) {
            (1, Wire::Len) => {
                let offset = reader.offset();
                let bytes = reader.bytes()?;
                value = std::str::from_utf8(bytes)
                    .map_err(|_| ProtoError::Malformed { offset })?
                    .to_owned();
            }
            (1, _) => return Err(ProtoError::Malformed { offset: at }),
            (_, wire) => reader.skip(wire)?,
        }
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wire {
    Varint,
    Fixed64,
    Len,
    Fixed32,
}

// Over one message's bytes; `base` is where they start in the whole proto, for offsets.
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
    base: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8], base: usize) -> Self {
        Self {
            bytes,
            pos: 0,
            base,
        }
    }

    fn offset(&self) -> usize {
        self.base + self.pos
    }

    fn at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    /// The next field's number, wire type and offset; `None` at the end.
    fn tag(&mut self) -> Result<Option<(u64, Wire, usize)>, ProtoError> {
        if self.at_end() {
            return Ok(None);
        }
        let at = self.offset();
        let tag = self.varint()?;
        let wire = match tag & 7 {
            0 => Wire::Varint,
            1 => Wire::Fixed64,
            2 => Wire::Len,
            5 => Wire::Fixed32,
            // 3 and 4 are groups, deprecated since proto2; 6 and 7 don't exist.
            _ => return Err(ProtoError::Malformed { offset: at }),
        };
        match tag >> 3 {
            0 => Err(ProtoError::Malformed { offset: at }),
            number => Ok(Some((number, wire, at))),
        }
    }

    fn varint(&mut self) -> Result<u64, ProtoError> {
        let at = self.offset();
        let mut value = 0;
        for shift in (0..64).step_by(7) {
            let Some(&byte) = self.bytes.get(self.pos) else {
                return Err(ProtoError::Truncated { offset: at });
            };
            self.pos += 1;
            // The tenth byte holds only the 64th bit.
            if shift == 63 && byte > 1 {
                return Err(ProtoError::Malformed { offset: at });
            }
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(ProtoError::Malformed { offset: at })
    }

    fn fixed64(&mut self) -> Result<u64, ProtoError> {
        let at = self.offset();
        let bytes = self.take(8, at)?;
        <[u8; 8]>::try_from(bytes)
            .map(u64::from_le_bytes)
            .map_err(|_| ProtoError::Malformed { offset: at })
    }

    fn bytes(&mut self) -> Result<&'a [u8], ProtoError> {
        let at = self.offset();
        let len =
            usize::try_from(self.varint()?).map_err(|_| ProtoError::Truncated { offset: at })?;
        self.take(len, at)
    }

    fn message(&mut self) -> Result<Reader<'a>, ProtoError> {
        let bytes = self.bytes()?;
        Ok(Reader::new(bytes, self.offset() - bytes.len()))
    }

    fn skip(&mut self, wire: Wire) -> Result<(), ProtoError> {
        let at = self.offset();
        match wire {
            Wire::Varint => self.varint().map(drop),
            Wire::Fixed64 => self.take(8, at).map(drop),
            Wire::Len => self.bytes().map(drop),
            Wire::Fixed32 => self.take(4, at).map(drop),
        }
    }

    // `at` is where the value that needs these bytes starts.
    fn take(&mut self, len: usize, at: usize) -> Result<&'a [u8], ProtoError> {
        let bytes = self
            .pos
            .checked_add(len)
            .and_then(|end| self.bytes.get(self.pos..end))
            .ok_or(ProtoError::Truncated { offset: at })?;
        self.pos += len;
        Ok(bytes)
    }
}

#[cfg(test)]
pub(crate) mod encode;
#[cfg(test)]
mod tests;
