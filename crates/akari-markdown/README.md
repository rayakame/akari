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
  don't assign); such URLs stay text.
- **Shortcodes** stay text (`:c:` isn't ©), commands, navigation, email and phone links and
  the `s`/`S` timestamp styles follow docs.discord.food, and IDs beyond `u64` stay text.

## Limits

Every input parses. Rules nest at most 64 deep (a delimiter that would open the 65th level stays
text) and lists 11 deep. Spoilers and links get at most 65,535 ids. The parser's work is linear:
with the `fuzzing` feature, `parse_with_steps` counts it, and every corpus, property and fuzz
input stays within `STEPS_PER_BYTE × (length + 1)` steps (256; the worst case, a chain of 64
nested masked-link checks, takes about 200).

## Tests

```sh
cargo test -p akari-markdown                       # corpus, properties, bounds, units
cargo test --release -p akari-markdown -- --ignored parse_cost --nocapture
```

- `tests/corpus/*.txt` holds the cases by area, in the format described in
  `src/tests/corpus.rs`: an input, `--`, and the expected outline, one line per block.
  `samples.txt` has realistic messages, `parse-discord.txt` parse-discord's own test inputs.
- Fuzzing needs cargo-fuzz; the fuzz crate has its own workspace and pins a nightly:

```sh
cd crates/akari-markdown/fuzz
cargo fuzz run parse -O corpus/parse ../tests/corpus -- -max_total_time=600
cargo fuzz run source_range -O corpus/source_range ../tests/corpus -- -max_total_time=600
```

CI fuzzes each target for two minutes on pull requests that change this crate.
