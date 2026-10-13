// Synthetic messages with every construct the list renders, for the table's tests and the
// probe's formatted mode. No real names, IDs or addresses.
enum FormattedSamples {
    static let contents: [String] = [
        "Hey, did anyone try the **new build**? It feels *much* faster on my old laptop.",
        "# Release 2.4.0\n## What's new\n- Faster startup on older machines\n- **Dark mode** for the settings window\n- Search now finds `#channels` by topic\n## Fixes\n1. Crash when pasting an empty image\n2. The unread marker sometimes stayed after reading\n-# Thanks to everyone who reported bugs!",
        "Hey, why does this panic? I thought `unwrap_or` was safe.\n```rust\nfn main() {\n    let v: Vec<u32> = Vec::new();\n    println!(\"{}\", v[0]);\n}\n```\nIt says *index out of bounds*.",
        "> why does this panic?\n`v[0]` indexes an empty vector. Use `v.first()` instead:\n```rs\nif let Some(x) = v.first() { println!(\"{x}\"); }\n```",
        "<@100000000000000002> can you look at <#300000000000000002>? The <@&500000000000000002> asked about it.",
        "Meeting moved to <t:1800000000:F> (<t:1800000000:R>). Bring notes!",
        "Spoilers for the finale: ||the ship was never real|| and ||everyone knew||.",
        "Docs are at https://example.com/docs and the [migration guide](https://example.com/guide \"Read this first\").",
        "😀 🎉",
        "__underlined__ ~~struck~~ ***both*** and a `code span` in one line with an escaped \\*star\\*.",
        "- one\n  - nested one\n    - deeper\n- two\n> a quote with a list:\n> - item\n> - item two",
        "@everyone the server restarts in ten minutes. </restart now:123456789012345678>",
        "A long line without breaks: https://example.com/" + String(repeating: "averyverylongpathsegment", count: 6),
        "Plain text that is long enough to wrap over several lines at any of the widths the tests use, "
            + "so the heights of formatted rows are checked on more than one line.",
        "Channel link https://discord.com/channels/200000000000000001/300000000000000002 and a message link https://discord.com/channels/200000000000000001/300000000000000002/400000000000000099",
    ]
}
