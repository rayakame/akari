# Message formatting

How Discord formats message content, and how Akari shows it. The parser is
`crates/akari-markdown` (its README lists the rules' sources); the macOS app renders its
document (`apps/macos/README.md`). Colors are the theme's tokens ([theme.md](theme.md)).

## How Discord reads markdown

Discord's markdown isn't CommonMark. At each position the first rule that matches wins, a
style ends at its first valid closing delimiter, and its inside is read again. Line rules
(quotes, headings, subtext, lists) only start at a line's start, but that can be inside a
style that starts there: `*> a*` is an italic quote.

| Construct | Written | Discord shows | Akari |
|---|---|---|---|
| Bold, italic, underline, strikethrough | `**a**` `*a*` `_a_` `__a__` `~~a~~` | the style; styles combine | font traits, single lines |
| Spoiler | `\|\|a\|\|` | a dark cover until clicked, then a tinted background; hidden again after switching channels | a `spoilerHidden` cover over the text and pills (over the whole box for a code block); a click reveals all its parts, on `spoilerRevealed`, until the channel is left |
| Inline code | `` `a` `` (any number of backticks) | monospace on a pill, nothing inside parsed | monospaced 14 on a `codeBackground` pill with a `codeBorder` line |
| Code block | ```` ```lang ```` … ```` ``` ```` | a box, highlighted by language | a box with `codeBackground`, a `codeBorder` line and 8 of padding, monospaced 14 on 18, long lines wrapped; the language is hidden; no highlighting yet |
| Quote | `> a`, `>>> rest` | a bar beside the lines | a 4 wide `quoteBar`, text indented 16 |
| Headings | `# a`, `## a`, `### a` | three large bold sizes | 24, 20 and 16 bold on 32, 28 and 22; 8 above (not first), 4 below |
| Subtext | `-# a` | small muted text | 13 on 18 in `textMuted` |
| Lists | `- a`, `* a`, `1. a`; two spaces continue an item | bullets or numbers, nested 11 deep | a 20 wide marker column per level: `•`, then `◦` deeper; numbers right-aligned |
| Masked link | `[text](url "title")` | the text as a link; on hover the title, then the URL in parentheses; a click asks first, showing the full URL, with a checkbox to trust the domain | `textLink`; the same tooltip, and a dialog in Akari's words (below) |
| URL | `https://…`, `<https://…>` | a link; brackets hide the embed | `textLink`; opens in the browser |
| Channel or message link | `https://discord.com/channels/…` | a pill with the channel's name; a message link's pill ends in a message icon | a pill when Akari knows the channel, with its kind's symbol (a message link's ends in a bubble); a click opens the channel; an unknown channel's link is a plain link |
| Email, phone | `<a@b.co>`, `<+1 555 010 0199>` | links (docs.discord.food) | links |
| Mentions | `<@id>`, `<@&id>`, `<#id>`, `@everyone`, `@here` | pills with the name; role color for roles | medium-weight pills, `mentionText` on `mentionBackground`; a role with a color in that color on 10% of it; a channel with its kind's symbol, and a click opens it |
| Slash command | `</name sub:id>` | a pill with `/name sub` | a pill; a click does nothing yet |
| Custom emoji | `<:name:id>`, `<a:name:id>` | the image | `:name:` as text until the emoji milestone |
| Timestamp | `<t:seconds:style>`, styles `t T d D f F s S R` | the time in the viewer's locale (below); `R` counts | a `timestampBackground` pill; the full date as a tooltip; `R` counts while on screen |
| Navigation | `<id:customize>` and friends | a link into the server's pages | a pill in Akari's words (Channels & Roles, Browse Channels, Server Guide, Linked Roles); a click does nothing yet |
| Escapes | `\*` | the character | the character |

A message of 1 to 27 emoji (Unicode or custom) and whitespace, nothing else, shows them large
(jumbo). Akari shows Unicode emoji at 48 on 56; a message with a custom emoji isn't jumbo until
custom emoji are images.

A message that mentions the reader is highlighted ([message-list.md](message-list.md)).

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
copies the message's markdown. In Akari a hidden spoiler copies its text, a channel's symbol
isn't copied (a channel mention copies as its name), and a list item copies with its marker
between tabs.

## Links

A plain URL opens in the browser without asking (the dialog was observed on masked links
only). A masked link asks first:

| | Discord | Akari |
|---|---|---|
| Title | Leaving Discord | Open This Link? |
| Text | This link takes you to the following website | This link goes to the following website: |
| Checkbox | Trust links from example.com from now on | Trust links to example.com from now on |
| Buttons | Back, Visit Site | Back, Open Link |

The full URL shows under the text; a link without a host (mailto:, tel:, sms:) has no checkbox,
since trust is by host. A trusted host's masked links open without asking. Akari
keeps the trusted hosts per account and matches the exact host (a subdomain isn't trusted with
its parent), on any port, in its punycode spelling and without a trailing dot; logging out or
an expired session forgets them and closes a confirmation still open as Back would, and
there's no way to untrust one yet. A masked link to a
channel Akari knows opens the channel without asking.

Akari opens only the schemes the parser makes links of: http, https, discord, steam, mailto,
tel and sms. Any other URL that reaches the link handler opens nothing.

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
reportedly shows "No Access" with a lock (not observed yet). Akari does the same, as pills
except the raw user mention, but only when it has the server's lists; before they load it
shows a muted `@user`, `@role` or `#channel` without a pill, so nothing reads "unknown" just
because it hasn't loaded, and the name replaces it when the server arrives.

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
- The highlight when @everyone is suppressed: Akari leaves it off, as the official client
  shows such a message like a plain one; the tint is still to compare side by side.
- A masked link without a title, and a channel link with a trailing slash.
- That a plain link opens without the warning (Akari opens it directly).
