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
| Spoiler | `\|\|a\|\|` | a dark cover until clicked, then a tinted background; hidden again after switching channels | cover drawn over the text and pills; a click reveals all its parts |
| Inline code | `` `a` `` (any number of backticks) | monospace on a pill, nothing inside parsed | monospaced 14 on a `codeBackground` pill |
| Code block | ```` ```lang ```` … ```` ``` ```` | a box, highlighted by language | a box with `codeBackground` and `codeBorder`; no highlighting yet |
| Quote | `> a`, `>>> rest` | a bar beside the lines | a 4 wide `quoteBar`, text indented 16 |
| Headings | `# a`, `## a`, `### a` | three large bold sizes | 24, 20 and 16 bold |
| Subtext | `-# a` | small muted text | 13 on 18 in `textMuted` |
| Lists | `- a`, `* a`, `1. a`; two spaces continue an item | bullets or numbers, nested 11 deep | a 20 wide marker column per level |
| Masked link | `[text](url "title")` | the text as a link; on hover the title, then the URL in parentheses; a click asks first, showing the full URL, with a checkbox to trust the domain | `textLink`; the same tooltip, and a dialog in Akari's words with trusted domains per account |
| URL | `https://…`, `<https://…>` | a link; brackets hide the embed | `textLink` |
| Channel or message link | `https://discord.com/channels/…` | a pill with the channel's name; a message link's pill ends in a message icon | a pill when Akari knows the channel (a click opens it), else a plain link |
| Email, phone | `<a@b.co>`, `<+1 555 010 0199>` | links (docs.discord.food) | links |
| Mentions | `<@id>`, `<@&id>`, `<#id>`, `@everyone`, `@here` | pills with the name; role color for roles | pills (`mentionText` on `mentionBackground`) |
| Slash command | `</name sub:id>` | a pill with `/name sub` | a pill |
| Custom emoji | `<:name:id>`, `<a:name:id>` | the image | `:name:` as text until the emoji milestone |
| Timestamp | `<t:seconds:style>`, styles `t T d D f F s S R` | the time in the viewer's locale (below); `R` counts | a pill; the full date as a tooltip |
| Navigation | `<id:customize>` and friends | a link into the server's pages | a pill |
| Escapes | `\*` | the character | the character |

A message of 1 to 27 emoji (Unicode or custom) and whitespace, nothing else, shows them large
(jumbo).

## Timestamps

Discord formats them in the viewer's locale; Akari uses the system locale's matching styles.
`<t:1800000000:…>` in German (Europe/Berlin), as the official client shows it:

| Style | Shows |
|---|---|
| `t` | `09:00` |
| `T` | `09:00:00` |
| `d` | `15.01.27` (the locale's short date) |
| `D` | `15. Januar 2027` |
| `f` | `15. Januar 2027 um 09:00` |
| `F` | `Freitag, 15. Januar 2027 um 09:00` |
| `s` | `15.01.27, 09:00` |
| `S` | `15.01.27, 09:00:00` |
| `R` | `in 3 Monaten` (from October 2026) |

## Copy

Cmd+C copies a selection's rendered text as plain text: from the middle of a bold word to the
end of a mention gives `bold and @name`, without markdown. The message menu's "Copy Text"
copies the message's markdown.

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

Discord shows a mention of a user it doesn't know as its raw text (`<@1>`), a role that's gone
as `@unknown-role`, and a channel it doesn't know as `#unknown`. A channel the reader can't see
reportedly shows "No Access" with a lock (not observed yet). Akari shows those only when it has
the server's lists; before they load it shows a neutral, muted `@role` or `#channel`, so nothing
reads "unknown" just because it hasn't loaded.

## Still to observe in the official client

The parser follows parse-discord where Discord's behavior isn't documented. The official client
agreed in every case checked so far (October 2026); the corpus still marks those cases
`observe:`. Still open:

- Role mentions: the role's color, and the highlight for a role the reader has.
- A channel the reader can't see.
- How slash commands and `<id:…>` navigation render, and what a click does.
- Timestamps: the hover text, and how often `R` changes.
- What Cmd+C copies from a hidden spoiler.
- Spaces at a message's start and end.
- The highlight when @everyone is suppressed.
- A masked link without a title, and a channel link with a trailing slash.
- That a plain link opens without the warning (Akari opens it directly).
