# The emoji picker's data

What akari-core gives the emoji picker for reacting: Unicode's emoji and their names, the user's
servers' emoji and which of them may be used where, the recently used reactions, the hover
bar's quick reactions and search. The rules are Discord's client's as read in canary 671549
(the reactions read, §2.1, §3.8 and §5), not observed. Departures are marked *Akari:*. How the
picker looks and behaves is in [../ui](../ui).

## Unicode emoji

`akari_core::emoji` takes Unicode's emoji from the `emojis` crate (0.9.1): Unicode 18.0's
`emoji-test.txt` order and groups, CLDR's English names, GitHub's gemoji short codes, skin tones
and emoji versions. There is no table of Akari's own and no generator (plan amendment 282); the
notices are in [THIRD-PARTY-NOTICES.md](../../THIRD-PARTY-NOTICES.md).

- `unicode_groups(newest)` lists 1,923 emoji in the nine CLDR groups (Smileys & Emotion through
  Flags), each group in Unicode's order, without skin-toned variants and without components.
- **Names.** `UnicodeEmoji::name` is the short name Discord's users type: gemoji's word
  (`thumbsup`, not `+1`), a flag as `flag_de`, skin tones as `_tone1` to `_tone5` (two for a
  pair), else Unicode's name with underscores (`cracking_face`, `t_rex`). akari-markdown's
  `emoji_shortcode`, which names a reaction's emoji in tooltips, has the same rule; akari-core
  can't depend on akari-markdown, so a test in akari-ffi checks every emoji and skin tone against
  both. Only ©, ® and ™ differ: the picker calls them `copyright`, `registered` and `tm`, while
  `emoji_shortcode` has no name for them, as Discord leaves them as text in a message.
  `shortcodes()` are gemoji's other codes (`+1`), `cldr_name()` Unicode's name (`thumbs up`).
- **What the host draws.** Every read that returns Unicode emoji takes `newest`, the newest emoji
  version the host's emoji font draws, and leaves out newer ones. *Akari:* Discord draws its own
  images (Twemoji) and lists every emoji; Akari draws the system's font, so the app decides the
  version for the system it runs on.
- **What a pick sends** (`PickerEmoji::reaction`): the emoji itself, fully qualified. Discord
  applies the user's default skin tone; Akari has none (skin tones are in the backlog).

## The user's servers' emoji

The store keeps each server's custom emoji (name, animated, available, managed, the roles that
may use it; [models.md](models.md)), sorted by ID, from READY, GUILD_CREATE and
GUILD_EMOJIS_UPDATE, with `GuildEmojisChanged` when a server's list changes
([dispatches.md](dispatches.md)). They are in the disk cache's guild row ([cache.md](cache.md)).

`Store::emoji_access(channel, emoji)` says whether the user may react with one in a channel, by
the REACTION intention of Discord's emoji filter:

| `EmojiAccess` | When |
|---|---|
| `External` | In a server's channel, another server's emoji, and the user lacks Use External Emojis there (a timeout takes it away too) |
| `RoleLocked` | The emoji is for some roles, and the user's member in its server has none of them |
| `Unavailable` | `available` is false: the server lost the boosts the emoji needs |
| `NitroLocked` | Without Nitro (any tier): an animated emoji, or another server's unless it is an integration's (`managed`). In a DM or group DM every server's emoji is another's, and no permission applies |
| `Usable` | Otherwise |

*Akari:* the order the reasons are checked in isn't from the read. Akari checks the reasons the
picker hides first, so an unavailable emoji of another server in a channel without external
emoji is hidden, not dimmed.

`Store::custom_emoji_sections(channel)` is the picker's list of servers: the channel's own
server first, then the rail's order (folders in their place), each server's emoji by ID (upload
order). *Akari:* Discord's picker hides what its filter rejects and dims what is locked; which
reason does which isn't in the read. Akari leaves out `External` and `RoleLocked` and lists
`Unavailable` and `NitroLocked` with their reason for the app to dim (D126: fewer emoji the user
can't use). A server left without any is left out. An unknown channel has none.

## Recently used reactions

*Akari:* Discord keeps the emoji a user reacts with in the frecency of the
FRECENCY_AND_FAVORITES_SETTINGS proto, synced across devices, with up to ten recent uses per
emoji, beside the favourites. Akari doesn't read or write that proto: it counts on its own, per
account, from its first reaction on, and has no favourites.

- A use is counted when `Account::react` adds the user's reaction (the store showed it as not
  theirs), wherever the click was: a pill, a quick reaction, the picker. Taking one back, a
  click on what is the user's already and a click the message's actions refuse count nothing; a
  reaction Discord refuses afterwards stays counted.
- Each emoji keeps its count and its last use: a Unicode emoji by its fully qualified form (❤ and
  ❤️ count as one), a custom one by ID with the name it last came with. At most 42
  (`RECENT_REACTIONS`, the official client's 42 frequently used); a new emoji in a full list takes
  the place of the weakest other one, so a use always shows.
- **Rank:** the count times a weight for the last use's age, Discord's frecency weights: 100
  within 3 days, 70 within 15, 50 within 30, 30 within 45, 10 within 80, and 1 after that (Discord
  drops to 0); at the same score the later use first.
- They are kept in the disk cache (`user_settings`, key `reaction_uses`). READY doesn't carry
  them; a READY before the restore adds its session's uses to the cached ones.

`Store::recent_reactions(channel, newest)` is the picker's Frequently Used there, best first, by
the same filter as the servers' lists, without custom emoji none of the user's servers has and
without Unicode emoji newer than `newest`. No event: the picker reads it when it opens.

## Quick reactions

`Store::quick_reactions(channel, newest)` are the hover bar's three: the recently used reactions
the user may use there (`Usable`), filled from 💯, 😆 and 💖 (`100`, `laughing`,
`sparkling_heart`), each emoji once (research §3.8). *Akari:* Discord leaves out what its
filter rejects; Akari keeps only what a click can add, so a locked emoji never fills a place. No
event: the bar reads them when it shows.

## Search

`Store::search_emoji(channel, query, newest, limit)`:

- **The query:** spaces and colons around it are left out (`:smile:`), case doesn't matter, and
  it is compared both as typed (`+1`) and with its words joined by underscores and other
  punctuation dropped (`flag: germany` is `flag_germany`, as Unicode's names are compared).
- **What it finds:** a Unicode emoji by its short name, its other short codes or Unicode's name;
  a custom emoji by its name, among those the servers' lists show there.
- **Order:** those the user may use first; then how well the best name matches: the whole name,
  its start, a word's start (after `_`), anywhere; then the channel's own server, Unicode, the
  other servers; then the picker's own order.
- **Speed:** about 0.5 to 0.9 ms in a release build for Unicode's 1,923 emoji and 2,000 custom
  emoji on a loaded Mac. The store's lock is held only while the custom emoji are matched;
  akari-ffi's `search_emoji` runs on Akari's runtime.
- *Akari:* no CLDR keywords (Discord finds 😀 by "happy"; the `emojis` crate has no keywords), no
  boost for frequently used emoji, and Discord's own ranking wasn't read.

## Not done

Skin tones, favourites, Discord's frecency proto, CLDR keywords, the picker in the composer and
`+:name:` are in the backlog.
