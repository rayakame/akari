# Message formatting

How Discord formats message content, and how Akari shows it. The parser is
`crates/akari-markdown` (its README lists the rules' sources); the macOS rendering comes with
the message formatting milestone's third part. Until then Akari shows content as plain text.

## How Discord reads markdown

Discord's markdown isn't CommonMark. At each position the first rule that matches wins, a
style ends at its first valid closing delimiter, and its inside is read again. Line rules
(quotes, headings, subtext, lists) only start at a line's start, but that can be inside a
style that starts there: `*> a*` is an italic quote.

| Construct | Written | Discord shows | Akari (planned) |
|---|---|---|---|
| Bold, italic, underline, strikethrough | `**a**` `*a*` `_a_` `__a__` `~~a~~` | the style; styles combine | font traits, single lines |
| Spoiler | `\|\|a\|\|` | a dark cover until clicked, then a tinted background | cover drawn over the text and pills; a click reveals all its parts |
| Inline code | `` `a` `` (any number of backticks) | monospace on a pill, nothing inside parsed | monospaced 14 on a `codeBackground` pill |
| Code block | ```` ```lang ```` … ```` ``` ```` | a box, highlighted by language | a box with `codeBackground` and `codeBorder`; no highlighting yet |
| Quote | `> a`, `>>> rest` | a bar beside the lines | a 4 wide `quoteBar`, text indented 16 |
| Headings | `# a`, `## a`, `### a` | three large bold sizes | 24, 20 and 16 bold |
| Subtext | `-# a` | small muted text | 13 on 18 in `textMuted` |
| Lists | `- a`, `* a`, `1. a`; two spaces continue an item | bullets or numbers, nested 11 deep | a 20 wide marker column per level |
| Masked link | `[text](url "title")` | the text as a link; a warning with the real URL before opening | `textLink`; a sheet showing the URL first |
| URL | `https://…`, `<https://…>` | a link; brackets hide the embed | `textLink` |
| Email, phone | `<a@b.co>`, `<+1 555 010 0199>` | links (docs.discord.food) | links |
| Mentions | `<@id>`, `<@&id>`, `<#id>`, `@everyone`, `@here` | pills with the name; role color for roles | pills (`mentionText` on `mentionBackground`) |
| Slash command | `</name sub:id>` | a pill with `/name sub` | a pill |
| Custom emoji | `<:name:id>`, `<a:name:id>` | the image | `:name:` as text until the emoji milestone |
| Timestamp | `<t:seconds:style>`, styles `t T d D f F s S R` | the time in the viewer's locale; `R` counts | a pill; the full date as a tooltip |
| Navigation | `<id:customize>` and friends | a link into the server's pages | a pill |
| Escapes | `\*` | the character | the character |

A message of 1 to 27 emoji (Unicode or custom) and whitespace, nothing else, shows them large
(jumbo).

## Where Discord differs from CommonMark

- Bold has no whitespace rule (`** a**` is bold); asterisk italics need no space after the
  opener, and up to two spaces before the closer are dropped.
- Underscore italics only check their right edge: `a_b_` is italic, `snake_case` isn't.
- No code-span precedence: whatever starts first wins, so bold that starts before a backtick
  ends at its own closer, even where CommonMark would see code.
- A code block in the middle of a line still is a block.
- Every newline is a line break; blank lines are kept; spaces before a newline aren't shown.
- Lists need no blank line before them and end at the first line that doesn't continue them.
- No tables, images, horizontal rules, setext headings, HTML, `####`, task lists, `+` bullets
  or `~~~` fences.

## Names that can't be resolved

Discord shows `@unknown-user` for a user it doesn't know, `@deleted-role` for a role that's
gone, and "No Access" with a lock for a channel the reader can't see (reported, to observe).
Akari shows those only when it has the server's lists; before they load it shows a neutral,
muted `@role` or `#channel`, so nothing reads "deleted" just because it hasn't loaded.

## Still to observe in the official client

The parser follows parse-discord where Discord's behavior isn't documented. These cases still
need an observation in the official client; until then Akari does what the corpus says.

- Headings: `#  # a`, ` # a` after a line, `- # a`, `#` and a newline, an empty heading.
- Code: `` `a``b` ``, a language followed by spaces, a fence holding only a newline, an empty
  line after a code block, an uppercase language tag.
- Lists and quotes after leading spaces.
- Masked links: escapes in the text, invite and Discord URLs as text, the warning's wording.
- Mentions without a word boundary (`@everyonee`, `email@here.com`), unknown and deleted
  names, role colors, slash commands, navigation, game profiles.
- Timestamps: each style's exact text, the `R` wording and update rate, the range's edges.
- Jumbo with a mention, a timestamp, a spoiler or a style; 27 and 28 emoji; Unicode 16 emoji.
- `steam://` and `discord://` as text; `<url)>`; Discord channel and message links as pills.
- Email and phone links.
- Copying a selection (rendered text or markdown), a spoiler's state after switching channels,
  spaces at a message's start and end, the highlight with suppressed @everyone.
