use reqwest::Url;

use super::{AnimatedFormat, ImageError, ImageKey};
use crate::model::StickerFormatType;
use crate::state::{Attachment, EmbedMedia, ImageHash};

// The sizes the official client asks for (canary 649848, module 776231).
pub(crate) const LADDER: [u32; 31] = [
    16, 20, 22, 24, 28, 32, 40, 44, 48, 56, 60, 64, 80, 96, 100, 128, 160, 240, 256, 300, 320, 480,
    512, 600, 640, 1024, 1280, 1536, 2048, 3072, 4096,
];
pub(crate) const MAX_SIZE: u32 = 4096;
// Proxy sizes start here, so nearby boxes share a file.
const PROXY_FLOOR: u32 = 128;
const IS_ANIMATED: u64 = 1 << 5;
const EXTERNAL_HOSTS: [&str; 2] = ["images-ext-1.discordapp.net", "images-ext-2.discordapp.net"];
// The proxy's own size and form parameters, replaced by Akari's.
const SIZING: [&str; 7] = [
    "width",
    "height",
    "format",
    "quality",
    "animated",
    "size",
    "passthrough",
];

/// The smallest size the CDN serves with at least `pixels`, at most 4096.
pub(crate) fn ladder_size(pixels: u32) -> u32 {
    LADDER
        .iter()
        .copied()
        .find(|&size| size >= pixels)
        .unwrap_or(MAX_SIZE)
}

/// The proxy's width and height for a box of `wanted` pixels over a `source`, or `None` for the
/// source as it is: the box covered, the longest side snapped up the ladder from 128, the other
/// side following the aspect, never above the source and at most 4096.
pub(crate) fn proxy_size(source: (u32, u32), wanted: (u32, u32)) -> Option<(u32, u32)> {
    let (width, height) = source;
    if width == 0 || height == 0 {
        return None;
    }
    let longest = width.max(height);
    let scale =
        (f64::from(wanted.0) / f64::from(width)).max(f64::from(wanted.1) / f64::from(height));
    let target = if scale >= 1.0 {
        longest
    } else {
        let needed = (f64::from(longest) * scale).ceil() as u32;
        LADDER
            .iter()
            .copied()
            .find(|&size| size >= needed.max(PROXY_FLOOR))
            .unwrap_or(MAX_SIZE)
    };
    let target = target.min(MAX_SIZE);
    if target >= longest {
        return None;
    }
    let factor = f64::from(target) / f64::from(longest);
    let other = |side: u32| ((f64::from(side) * factor).round() as u32).max(1);
    Some(if width >= height {
        (target, other(height))
    } else {
        (other(width), target)
    })
}

/// What a file is, besides its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Form {
    Still,
    WebP,
    Gif,
    Apng,
}

/// How to get an image: its URL, what a cached file must have to answer instead, and its key on
/// disk.
#[derive(Debug, Clone)]
pub(crate) struct Planned {
    pub(crate) disk: String,
    pub(crate) form: Form,
    /// The longest side a cached file needs; one of the source's own size answers anyway.
    pub(crate) needed: u32,
    /// The URL asks for the source's own size: nothing larger exists.
    pub(crate) full: bool,
    /// The source's longest side when it's known (attachments, embeds); an answer that large is
    /// the source. Without it, the CDN's rule: an answer under `needed` is.
    pub(crate) source: Option<u32>,
    pub(crate) url: Url,
    /// The signed CDN link whose signature `url` carries, refreshed when it's due.
    pub(crate) signed: Option<Url>,
}

/// Discord's image hosts: the CDN and the media proxy as the endpoints name them, and the
/// proxy's hosts for external images.
#[derive(Debug, Clone)]
pub(crate) struct Hosts {
    cdn: Url,
    media: Url,
}

impl Hosts {
    pub(crate) fn new(cdn: Url, media: Url) -> Self {
        Self { cdn, media }
    }

    /// Whether Akari fetches from `url`: only Discord's image hosts, each with its scheme and
    /// port.
    pub(crate) fn allows(&self, url: &Url) -> bool {
        let same = |host: &Url| {
            url.scheme() == host.scheme()
                && url.host_str() == host.host_str()
                && url.port_or_known_default() == host.port_or_known_default()
        };
        same(&self.cdn)
            || same(&self.media)
            || (url.scheme() == "https"
                && url.port_or_known_default() == Some(443)
                && url
                    .host_str()
                    .is_some_and(|host| EXTERNAL_HOSTS.contains(&host)))
    }

    /// Avatars, icons, emoji, stickers and default avatars; `wanted` is the box's longest side
    /// in pixels.
    pub(crate) fn identity(
        &self,
        key: &ImageKey,
        wanted: u32,
        animated: Option<AnimatedFormat>,
    ) -> Result<Planned, ImageError> {
        let size = ladder_size(wanted);
        let cdn = |path: String, disk: String, hash: &ImageHash, lossless: bool| {
            let motion = animated.filter(|_| hash.is_animated());
            self.sized(&self.cdn, path, disk, size, lossless, motion)
        };
        match key {
            ImageKey::Avatar { user, hash } => Ok(cdn(
                format!("avatars/{}/{hash}", user.get()),
                format!("avatar/{}/{hash}", user.get()),
                hash,
                false,
            )),
            ImageKey::MemberAvatar { guild, user, hash } => Ok(cdn(
                format!("guilds/{}/users/{}/avatars/{hash}", guild.get(), user.get()),
                format!("member/{}/{}/{hash}", guild.get(), user.get()),
                hash,
                false,
            )),
            ImageKey::GuildIcon { guild, hash } => Ok(cdn(
                format!("icons/{}/{hash}", guild.get()),
                format!("icon/{}/{hash}", guild.get()),
                hash,
                true,
            )),
            ImageKey::RoleIcon { role, hash } => Ok(cdn(
                format!("role-icons/{}/{hash}", role.get()),
                format!("role/{}/{hash}", role.get()),
                hash,
                true,
            )),
            // The client always asks a group's icon as a still PNG.
            ImageKey::GroupIcon { channel, hash } => {
                let mut url = join(
                    &self.cdn,
                    &format!("channel-icons/{}/{hash}.png", channel.get()),
                )?;
                url.query_pairs_mut().append_pair("size", &size.to_string());
                Ok(Planned {
                    disk: format!("group/{}/{hash}", channel.get()),
                    form: Form::Still,
                    needed: size,
                    full: false,
                    source: None,
                    url,
                    signed: None,
                })
            }
            ImageKey::Emoji {
                id,
                animated: moves,
            } => Ok(self.sized(
                &self.cdn,
                format!("emojis/{}", id.get()),
                format!("emoji/{}", id.get()),
                size,
                false,
                animated.filter(|_| *moves),
            )),
            ImageKey::DefaultAvatar { index } if *index <= 5 => Ok(Planned {
                disk: format!("default/{index}"),
                form: Form::Still,
                needed: 0,
                full: true,
                source: None,
                url: join(&self.cdn, &format!("embed/avatars/{index}.png"))?,
                signed: None,
            }),
            ImageKey::DefaultAvatar { .. } => Err(ImageError::UnknownKey),
            ImageKey::Sticker { id, format } => self.sticker(id.get(), *format, size, animated),
            ImageKey::Attachment { .. } | ImageKey::EmbedMedia { .. } => {
                Err(ImageError::UnknownKey)
            }
        }
    }

    /// The original of an identity image, for Open in Browser and Copy Link: PNG, or GIF for an
    /// animated one, at the CDN's largest size.
    pub(crate) fn original(&self, key: &ImageKey) -> Result<Url, ImageError> {
        let still_or_gif = |animated: bool| if animated { "gif" } else { "png" };
        let (host, path, sized) = match key {
            ImageKey::Avatar { user, hash } => (
                &self.cdn,
                format!(
                    "avatars/{}/{hash}.{}",
                    user.get(),
                    still_or_gif(hash.is_animated())
                ),
                true,
            ),
            ImageKey::MemberAvatar { guild, user, hash } => (
                &self.cdn,
                format!(
                    "guilds/{}/users/{}/avatars/{hash}.{}",
                    guild.get(),
                    user.get(),
                    still_or_gif(hash.is_animated())
                ),
                true,
            ),
            ImageKey::GuildIcon { guild, hash } => (
                &self.cdn,
                format!(
                    "icons/{}/{hash}.{}",
                    guild.get(),
                    still_or_gif(hash.is_animated())
                ),
                true,
            ),
            ImageKey::GroupIcon { channel, hash } => (
                &self.cdn,
                format!("channel-icons/{}/{hash}.png", channel.get()),
                true,
            ),
            ImageKey::RoleIcon { role, hash } => (
                &self.cdn,
                format!("role-icons/{}/{hash}.png", role.get()),
                true,
            ),
            ImageKey::Emoji { id, animated } => (
                &self.cdn,
                format!("emojis/{}.{}", id.get(), still_or_gif(*animated)),
                false,
            ),
            ImageKey::Sticker { id, format } => {
                let extension = match format {
                    StickerFormatType::Png | StickerFormatType::Apng => "png",
                    StickerFormatType::Gif => "gif",
                    StickerFormatType::Lottie | StickerFormatType::Unknown(_) => {
                        return Err(ImageError::Unsupported);
                    }
                };
                (
                    &self.media,
                    format!("stickers/{}.{extension}", id.get()),
                    false,
                )
            }
            ImageKey::DefaultAvatar { index } if *index <= 5 => {
                (&self.cdn, format!("embed/avatars/{index}.png"), false)
            }
            _ => return Err(ImageError::UnknownKey),
        };
        let mut url = join(host, &path)?;
        if sized {
            url.query_pairs_mut()
                .append_pair("size", &MAX_SIZE.to_string());
        }
        Ok(url)
    }

    // Discord's query order: size, quality, animated.
    fn sized(
        &self,
        host: &Url,
        path: String,
        disk: String,
        size: u32,
        lossless: bool,
        animated: Option<AnimatedFormat>,
    ) -> Planned {
        let extension = match animated {
            Some(AnimatedFormat::Gif) => "gif",
            _ => "webp",
        };
        let mut url = host.clone();
        url.set_path(&format!("{}{path}.{extension}", host.path()));
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("size", &size.to_string());
            if lossless {
                query.append_pair("quality", "lossless");
            }
            if animated == Some(AnimatedFormat::WebP) {
                query.append_pair("animated", "true");
            }
        }
        Planned {
            disk,
            form: match animated {
                None => Form::Still,
                Some(AnimatedFormat::WebP) => Form::WebP,
                Some(AnimatedFormat::Gif) => Form::Gif,
            },
            needed: size,
            full: false,
            source: None,
            url,
            signed: None,
        }
    }

    // On the media host, in the forms the client asks for each format (module 378058).
    fn sticker(
        &self,
        id: u64,
        format: StickerFormatType,
        size: u32,
        animated: Option<AnimatedFormat>,
    ) -> Result<Planned, ImageError> {
        let (extension, form, extra): (&str, Form, &[(&str, &str)]) = match (format, animated) {
            (StickerFormatType::Png, _) => ("webp", Form::Still, &[("quality", "lossless")]),
            (StickerFormatType::Apng, Some(_)) => ("png", Form::Apng, &[]),
            (StickerFormatType::Apng, None) => ("png", Form::Still, &[("passthrough", "false")]),
            (StickerFormatType::Gif, Some(_)) => ("gif", Form::Gif, &[]),
            (StickerFormatType::Gif, None) => ("webp", Form::Still, &[]),
            (StickerFormatType::Lottie | StickerFormatType::Unknown(_), _) => {
                return Err(ImageError::Unsupported);
            }
        };
        let mut url = join(&self.media, &format!("stickers/{id}.{extension}"))?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("size", &size.to_string());
            for (name, value) in extra {
                query.append_pair(name, value);
            }
        }
        Ok(Planned {
            disk: format!("sticker/{id}"),
            form,
            needed: size,
            full: false,
            source: None,
            url,
            signed: None,
        })
    }

    /// An attachment's image, or a video's poster, through the media proxy at the box's
    /// `wanted` pixels.
    pub(crate) fn attachment(
        &self,
        attachment: &Attachment,
        wanted: (u32, u32),
        animated: Option<AnimatedFormat>,
    ) -> Result<Planned, ImageError> {
        let (Some(width), Some(height)) = (attachment.width, attachment.height) else {
            return Err(ImageError::NotAnImage);
        };
        if width == 0 || height == 0 {
            return Err(ImageError::NotAnImage);
        }
        let extension = extension(&attachment.filename);
        let kind = Kind::of(&extension).ok_or(ImageError::NotAnImage)?;
        let proxy = Url::parse(&attachment.proxy_url).map_err(|_| ImageError::NotAnImage)?;
        if !self.allows(&proxy) {
            return Err(ImageError::Unsupported);
        }
        let signed = Url::parse(&attachment.url)
            .ok()
            .filter(|url| self.allows(url));
        let form = match (kind, animated) {
            (Kind::Image, Some(_)) if extension == "gif" => Form::Gif,
            (Kind::Image, Some(_))
                if matches!(extension.as_str(), "webp" | "avif")
                    && attachment.flags & IS_ANIMATED != 0 =>
            {
                Form::WebP
            }
            _ => Form::Still,
        };
        Ok(Self::proxied(
            proxy,
            format!("attachment/{}", attachment.id.get()),
            (width, height),
            wanted,
            form,
            &extension,
            signed,
        ))
    }

    /// An embed's image or thumbnail through the proxy, keyed by its proxy path.
    pub(crate) fn embed(
        &self,
        media: &EmbedMedia,
        wanted: (u32, u32),
        animated: Option<AnimatedFormat>,
    ) -> Result<Planned, ImageError> {
        let (Some(width), Some(height)) = (media.width, media.height) else {
            return Err(ImageError::NotAnImage);
        };
        if width == 0 || height == 0 {
            return Err(ImageError::NotAnImage);
        }
        let source = media.proxy_url.as_deref().unwrap_or(&media.url);
        let proxy = Url::parse(source).map_err(|_| ImageError::NotAnImage)?;
        if !self.allows(&proxy) {
            return Err(ImageError::Unsupported);
        }
        let extension = extension(proxy.path());
        let form = match animated {
            Some(_) if extension == "gif" => Form::Gif,
            _ => Form::Still,
        };
        // An embedded attachment link: its CDN link is the one to refresh.
        let signed = Url::parse(&media.url)
            .ok()
            .filter(|url| self.allows(url) && is_attachment_link(url))
            .or_else(|| Some(proxy.clone()).filter(is_attachment_link));
        Ok(Self::proxied(
            proxy.clone(),
            embed_disk_key(&proxy),
            (width, height),
            wanted,
            form,
            &extension,
            signed,
        ))
    }

    /// An embed's author or footer icon, as Discord gives it.
    pub(crate) fn embed_icon(&self, url: &str) -> Result<Planned, ImageError> {
        let url = Url::parse(url).map_err(|_| ImageError::NotAnImage)?;
        if !self.allows(&url) {
            return Err(ImageError::Unsupported);
        }
        Ok(Planned {
            disk: embed_disk_key(&url),
            form: Form::Still,
            needed: 0,
            full: true,
            source: None,
            signed: Some(url.clone()).filter(is_attachment_link),
            url,
        })
    }

    // The client's form (module 803316): the signature kept, then format, quality, size.
    fn proxied(
        mut url: Url,
        disk: String,
        source: (u32, u32),
        wanted: (u32, u32),
        form: Form,
        extension: &str,
        signed: Option<Url>,
    ) -> Planned {
        let size = proxy_size(source, wanted);
        let kept: Vec<(String, String)> = url
            .query_pairs()
            .filter(|(name, _)| !SIZING.contains(&name.as_ref()))
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
        url.set_query(None);
        {
            let mut query = url.query_pairs_mut();
            for (name, value) in &kept {
                query.append_pair(name, value);
            }
            match form {
                Form::Gif | Form::Apng => {}
                Form::WebP => {
                    query.append_pair("format", "webp");
                    query.append_pair("animated", "true");
                }
                Form::Still => {
                    query.append_pair("format", "webp");
                    if matches!(extension, "png" | "avif") {
                        query.append_pair("quality", "lossless");
                    }
                }
            }
            if let Some((width, height)) = size {
                query.append_pair("width", &width.to_string());
                query.append_pair("height", &height.to_string());
            }
        }
        if url.query() == Some("") {
            url.set_query(None);
        }
        let longest = source.0.max(source.1);
        let (needed, full) = match size {
            None => (longest.min(MAX_SIZE), true),
            Some((width, height)) => {
                let side = width.max(height);
                (side, side >= MAX_SIZE)
            }
        };
        Planned {
            disk,
            form,
            needed,
            full,
            source: Some(longest),
            url,
            signed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Image,
    Video,
}

impl Kind {
    // By extension, as the official client decides (module 127038); SVGs are files.
    fn of(extension: &str) -> Option<Self> {
        match extension {
            "png" | "jpg" | "jpeg" | "jfif" | "webp" | "gif" | "heic" | "heif" | "dng" | "avif" => {
                Some(Self::Image)
            }
            "mp4" | "webm" | "mov" | "qt" => Some(Self::Video),
            _ => None,
        }
    }
}

fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

fn join(base: &Url, path: &str) -> Result<Url, ImageError> {
    base.join(path).map_err(|_| ImageError::UnknownKey)
}

// Signed links point at an attachment on Discord's own hosts.
pub(crate) fn is_attachment_link(url: &Url) -> bool {
    let path = url.path();
    path.starts_with("/attachments/") || path.starts_with("/ephemeral-attachments/")
}

// The proxy's path names the external image; the query only sizes it.
fn embed_disk_key(url: &Url) -> String {
    let digest = aws_lc_rs::digest::digest(
        &aws_lc_rs::digest::SHA256,
        format!("{}{}", url.host_str().unwrap_or_default(), url.path()).as_bytes(),
    );
    let hex: String = digest.as_ref()[..16]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("embed/{hex}")
}

#[cfg(test)]
mod tests;
