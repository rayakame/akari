use crate::{DiscordChannelLink, Link, LinkKind};

fn channel(url: &str) -> Option<DiscordChannelLink> {
    Link {
        url: url.into(),
        kind: LinkKind::Plain,
        suppress_embed: false,
    }
    .discord_channel()
}

#[test]
fn discord_channel_links_are_recognized() {
    let in_guild = DiscordChannelLink {
        guild_id: Some(1),
        channel_id: 2,
        message_id: None,
    };
    for host in [
        "discord.com",
        "ptb.discord.com",
        "canary.discord.com",
        "discordapp.com",
    ] {
        assert_eq!(
            channel(&format!("https://{host}/channels/1/2")),
            Some(in_guild),
            "{host}"
        );
    }
    assert_eq!(
        channel("https://discord.com/channels/1/2/3"),
        Some(DiscordChannelLink {
            message_id: Some(3),
            ..in_guild
        })
    );
    assert_eq!(
        channel("https://discord.com/channels/@me/2"),
        Some(DiscordChannelLink {
            guild_id: None,
            ..in_guild
        })
    );
    assert_eq!(
        channel("https://discord.com/channels/1/2/?x=1#y"),
        Some(in_guild)
    );
    for other in [
        "https://example.com/channels/1/2",
        "https://discord.com/channel/1/2",
        "https://discord.com/channels/1",
        "https://discord.com/channels/1/2/3/4",
        "https://discord.com/channels/a/2",
        "https://discord.com/channels/1/99999999999999999999999",
        "https://evil.discord.com.example/channels/1/2",
        "ftp://discord.com/channels/1/2",
        "https://discord.gg/abc",
        "https://discord.com//channels/1/2",
        "https://discord.com/channels//1/2",
        "https://discord.com/channels/1//2",
        "https://discord.com/channels/1/2//3",
        "https://discord.com/channels/1/2//",
    ] {
        assert_eq!(channel(other), None, "{other}");
    }
}
