# akari-markdown

Discord-flavored markdown, parsed the way Discord's clients parse message content. It has no
dependency on `akari-core`: the core stores raw content and each host parses it for display.

## What it parses

- Styles: bold, italic (`*` and `_`), underline, strikethrough, spoilers, and any mix of them.
- Code: inline code with any number of backticks, and code blocks with a language tag.
- Line rules: quotes (`> ` and `>>> `), headings (`#` to `###`), subtext (`-# `), and bulleted
  and numbered lists, eleven deep, with Discord's November 2025 indentation rules.
- Links: masked links with Discord's checks on their text and title, plain `http(s)` URLs,
  `<url>` without an embed, `<user@example.com>` and `<+1 555 010 0199>`.
- Tokens: user, role and channel mentions, `@everyone` and `@here`, slash commands, custom
  emoji, timestamps in all nine styles, and `<id:…>` server navigation.
- Escapes, Unicode emoji (for jumbo messages and link checks), and `:shortcodes:`, which stay
  text.

`parse` returns a flat `Document`: blocks (paragraph, heading, subtext, code block) inside
their containers (quotes, list items), each with inline runs that carry their styles, spoiler,
link and source range. `source_range` maps a selection back to its source without cutting a
token, and `utf16::Utf16Map` converts offsets for hosts that count UTF-16. `Cache` keeps
parsed documents by key.

An embed's text has rules of its own (`parse_embed`): a title or field name keeps styles, emoji
and timestamps but no code blocks, quotes, lists, headings, subtext or mentions, and no links when
the title is itself one; a description or field value has the message's rules, lists and
headings only for messages after 2023-03-22 and never a heading in a field. `plain` is a non-rich
embed's text as Discord shows it, its line breaks kept. A `Document` says whether it's one link
and nothing else (`is_single_link`) and whether a link sits in a spoiler (`has_spoilered_link`),
which decide an embed's link text and cover.

## Which behavior counts

Most of Discord's rules aren't documented. The rules follow
[parse-discord](https://github.com/LyricLy/parse-discord) at `eca0540c54f2`, which was built by
testing against the official client; docs.discord.food decides where it has nothing; an
observation in the official client wins over both. The corpus marks the cases still to observe
with `observe:`.

A local differential run compares Akari with parse-discord on generated input (150,000
inputs for the first version). The differences that remain are deliberate:

- **parse-discord crashes** on a heading with no text (`# `) and on spaces before a newline in
  link text. Akari shows an empty heading and drops the spaces.
- **`<url>` with a trailing `)`**: parse-discord trims it as for plain URLs and loses a
  character; Akari keeps the URL whole.
- **Python's regex classes**: parse-discord's `\d` reads other scripts' digits as list numbers
  and its `\w` takes combining marks into `:shortcode:` names. Akari uses ASCII digits, and
  letters and digits for names (Discord's own regexes are JavaScript, where both are ASCII).
- **IDNA**: rust-url refuses some hosts ada accepts (the bidi rule, code points its tables
  don't assign). Such URLs stay text, but in a masked link's text they still count as URLs, so
  the link is refused (stricter than parse-discord where neither parser takes the host).
- **Styles and code in a masked link's text**: the text is checked again with `*`, `_`, `~~`,
  `||` and backticks removed, read without nested rules, so
  `[ht**tps://example.com**](https://other.example)` can't pass for another address. Discord
  and parse-discord accept it.
- **Shortcodes** stay text (`:c:` isn't ©), commands, navigation, email and phone links and
  the `s`/`S` timestamp styles follow docs.discord.food, and IDs beyond `u64` stay text.

## Limits

Every input parses. Rules nest at most 64 deep (a delimiter that would open the 65th level stays
text) and lists 11 deep. A masked link's text is checked with at most 8 levels left, because each
check parses the text again. Spoilers and links get at most 65,535 ids. The parser's work is
linear: with the `fuzzing` feature, `parse_with_steps` counts it, and every corpus, property and
fuzz input stays within `STEPS_PER_BYTE × (length + 1)` steps (128; the worst case, 64 nested
italics, takes about 100), also when nested deep inside links, styles and quotes.

Hosts parse on their own threads, so the stack is bounded too: a rule hands its content back to
the parser instead of parsing it itself, so each level of nesting costs one frame, and building
the document doesn't recurse. In a debug build every corpus, adversarial and fuzz-found input
parses on a 256 KB thread; the deepest need about 85 KB there, and far less in a release build.

## Tests

```sh
cargo test -p akari-markdown                       # corpus, properties, bounds, units
cargo test --release -p akari-markdown -- --ignored parse_cost --nocapture
cargo test -p akari-markdown -- --ignored stack_peak --nocapture   # with the local fuzz corpus
```

- `tests/corpus/*.txt` holds the cases by area, in the format described in
  `src/tests/corpus.rs`: an input, `--`, and the expected outline, one line per block.
  `samples.txt` has realistic messages, `parse-discord.txt` parse-discord's own test inputs.
- `tests/fuzz-found` holds the fuzzer's inputs that nest deepest, for the stack test.
- Fuzzing needs cargo-fuzz; the fuzz crate has its own workspace and pins a nightly:

```sh
cd crates/akari-markdown/fuzz
cargo fuzz run parse -O -a corpus/parse ../tests/corpus -- -max_total_time=600
cargo fuzz run source_range -O -a corpus/source_range ../tests/corpus -- -max_total_time=600
```

CI fuzzes each target for two minutes, with debug assertions, on pull requests that change this
crate, and keeps what a failure leaves in `fuzz/artifacts` as the run's `fuzz-artifacts`.
