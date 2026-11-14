use std::process::ExitCode;

use akari_core::emoji::{EmojiVersion, unicode_groups};
use akari_core::model::ChannelId;
use akari_core::state::{EmojiAccess, PickerEmoji};

use crate::printable;
use crate::session::Session;

/// A version as `--newest` takes it: `15.1`, or `16` for `16.0`.
pub fn parse_version(text: &str) -> Result<EmojiVersion, String> {
    let (major, minor) = text.split_once('.').unwrap_or((text, "0"));
    match (major.parse::<u16>(), minor.parse::<u16>()) {
        (Ok(major), Ok(minor)) => Ok(EmojiVersion::new(major, minor)),
        _ => Err(format!(
            "`{}` is no emoji version: give it as 15.1 or 16.",
            printable(text)
        )),
    }
}

fn access_words(access: EmojiAccess) -> &'static str {
    match access {
        EmojiAccess::Usable => "usable",
        EmojiAccess::Unavailable => "unavailable: the server lost the boosts it needs",
        EmojiAccess::RoleLocked => "only for roles you don't have",
        EmojiAccess::External => "another server's, and this channel allows none",
        EmojiAccess::NitroLocked => "needs Nitro",
    }
}

fn custom_line(name: &str, id: u64, animated: bool, access: EmojiAccess) -> String {
    let animated = if animated { " animated" } else { "" };
    format!(
        ":{}:  {id}{animated}  {}",
        printable(name),
        access_words(access)
    )
}

pub(crate) fn emoji_line(emoji: &PickerEmoji) -> String {
    match emoji {
        PickerEmoji::Unicode(unicode) => format!("{}  :{}:", unicode.as_str(), unicode.name()),
        PickerEmoji::Custom(custom) => custom_line(
            &custom.emoji.name,
            custom.emoji.id.get(),
            custom.emoji.animated,
            custom.access,
        ),
    }
}

fn print_all(emojis: &[PickerEmoji]) {
    if emojis.is_empty() {
        println!("  (none)");
    }
    for emoji in emojis {
        println!("  {}", emoji_line(emoji));
    }
}

pub struct EmojiArgs {
    pub query: Option<String>,
    pub newest: EmojiVersion,
    pub limit: usize,
}

/// With a query, what the picker's search finds in the channel; without, what the picker lists
/// there: the quick reactions, the recently used ones, each server's emoji and how many Unicode
/// emoji.
pub fn run(session: &Session, channel: ChannelId, args: &EmojiArgs) -> ExitCode {
    let store = session.account.store();
    if store.channel(channel).is_none() {
        eprintln!("You have no channel with that ID; `akari-cli channels <guild>` lists them.");
        return ExitCode::FAILURE;
    }
    if let Some(query) = &args.query {
        let found = store.search_emoji(channel, query, args.newest, args.limit);
        if found.is_empty() {
            println!("(nothing)");
        }
        for emoji in &found {
            println!("{}", emoji_line(emoji));
        }
        return ExitCode::SUCCESS;
    }
    println!("Quick reactions:");
    print_all(&store.quick_reactions(channel, args.newest));
    println!("Recently used:");
    print_all(&store.recent_reactions(channel, args.newest));
    for section in store.custom_emoji_sections(channel) {
        let name = store
            .guild(section.guild_id)
            .map(|guild| printable(&guild.name).into_owned())
            .unwrap_or_default();
        println!("{name}  {}", section.guild_id.get());
        for custom in &section.emojis {
            println!("  {}", emoji_line(&PickerEmoji::Custom(custom.clone())));
        }
    }
    let groups = unicode_groups(args.newest);
    let count: usize = groups.iter().map(|group| group.emojis.len()).sum();
    println!(
        "Unicode: {count} emoji in {} groups, up to {}.{}",
        groups.len(),
        args.newest.major,
        args.newest.minor
    );
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use akari_core::emoji::UnicodeEmoji;

    use super::*;

    #[test]
    fn a_version_is_major_and_minor() {
        assert_eq!(parse_version("15.1"), Ok(EmojiVersion::new(15, 1)));
        assert_eq!(parse_version("16"), Ok(EmojiVersion::new(16, 0)));
        for text in ["", "x", "15.x", "1.2.3", "-1"] {
            assert!(parse_version(text).is_err(), "{text}");
        }
    }

    #[test]
    fn a_unicode_emoji_shows_with_its_name() {
        let up = PickerEmoji::Unicode(UnicodeEmoji::get("👍").unwrap());

        assert_eq!(emoji_line(&up), "👍  :thumbsup:");
    }

    #[test]
    fn a_custom_emoji_says_whether_it_may_be_used() {
        assert_eq!(
            custom_line("wave", 7, false, EmojiAccess::Usable),
            ":wave:  7  usable"
        );
        assert_eq!(
            custom_line("dance", 8, true, EmojiAccess::NitroLocked),
            ":dance:  8 animated  needs Nitro"
        );
        assert_eq!(
            custom_line("x\u{1b}[2J", 9, false, EmojiAccess::External),
            ":x\\u{1b}[2J:  9  another server's, and this channel allows none"
        );
    }
}
