# Disk cache

`akari_core::cache` keeps what a cold start draws before the gateway connects: the guilds,
channels, roles and the current member (for permissions), the DM recipients, the current
user, the user's settings (server folders and order, mutes, notification levels), the read
states (where the user stopped reading each channel, and its mentions), and the newest
messages of recently opened channels. The badges are computed from them at the restore, so a
cold start shows the last session's unread dots and counts. A launch shows the last session
from it at once; READY then brings only what changed. Linux and Android get it with the core.

## What is never cached

- The token. Nor READY's `analytics_token` or `auth_session_id_hash`: the state layer doesn't
  keep them, so nothing can write them.
- Pending and failed messages (the outbox), drafts, slowmode cooldowns and the gateway
  session (no resume across launches). Each holds what the user typed or needs the exact state
  at the end of a session, so each is its own decision for later.
- Images. Akari loads none yet.
- Blocked and ignored users. They only decide which live messages count as mentions, and
  none arrive before READY brings them again.

## One file per account

The host passes a directory to `DiscordClient::with_cache` (FFI: the constructor's
`cache_directory`); without one nothing is written. Each account has `<user id>.sqlite` there,
with SQLite's `-wal` and `-shm` beside it (and a `-journal` if a file was ever left outside WAL
mode).

| App | Directory |
|---|---|
| macOS | `Application Support/Akari/Cache` in the app's container, excluded from backups so message text doesn't spread into Time Machine or iCloud |
| iOS, Android, Linux | later |

## The file

SQLite through rusqlite. Apple targets link the system's `libsqlite3` (macOS 14's is newer
than the 3.34.1 rusqlite needs), which keeps about 3.5 MB of SQLite out of the universal app;
every other target builds rusqlite's bundled copy.

- **Settings at open:** `journal_mode = WAL`, `synchronous = NORMAL`, `secure_delete = ON` (a
  deleted row's text is overwritten in the file, not left in a free page), `auto_vacuum =
  INCREMENTAL` (set before the first table), 2 MiB of page cache, a 500 ms busy timeout.
- **Tables:** `meta` (the owner's user ID), `current_user`, `users` (DM recipients), `guilds`
  (with their roles), `unavailable_guilds`, `members` (the current user's, per guild),
  `channels`, `windows` (a cached channel and when it was last viewed), `messages` (per
  channel and ID), `guild_settings` (per guild; `guild_id` 0 holds the DM settings),
  `user_settings` (per key: `folders`, absent when the settings proto had none, and
  `notification_flags`) and `read_states` (per channel: the acknowledged message and the
  mention count). Each row is its key columns plus a `data` column of JSON in the cache's own
  row types, never the wire structs or the state types, so either can change without breaking
  files already written. A field added later reads old rows through its default.
- **Snowflakes** are stored as SQLite integers; timestamps as Unix nanoseconds, which keeps
  the microseconds Discord sends.
- **Version:** `PRAGMA user_version`, 3 since the read states and threads' `joined` (older
  files are deleted once, so the first launch after that update starts empty; a version 2 file
  would restore without read states, and every channel since the join would look unread until
  READY). A file with any other version is
  deleted and started empty, never migrated. The cache refills from Discord, so a migration
  would only add risk.

### Opening, and when a file is thrown away

| Found | Outcome | Log |
|---|---|---|
| Not a database, or `quick_check` fails | Deleted, started empty | `cache discarded reason=corrupt` |
| Another schema version | Deleted, started empty | `reason=other_schema` |
| Another account's file at this name | Deleted, started empty | `reason=other_user` |
| Any other SQLite error | Deleted, started empty | `reason=unreadable` |
| Another process holds it | No cache this session; the file stays | `cache unavailable reason=busy` |
| SQLite refuses WAL mode | Used as it is | `cache not in WAL mode reason=journal_mode` |
| The directory or file can't be created | No cache this session | `reason=io` |

None of these fails the launch or makes anything wait: the app goes on from the network.

## Threads

Each open file has its own thread (`akari-cache`), and every SQLite call runs there, never on
the runtime's two workers. Callers send it commands and await the reply. The gateway's pump
never touches it: it only bumps a change counter in the store.

## Writes

- **When:** at most every 10 s while the store's change counter moves, and at once on
  `Account::disconnect()` (sleep), `close()` and `flush_cache()` (quit). Nothing is written
  before the store holds a session (a READY or a restore), and nothing after `close()`.
- **What:** a snapshot of the store's entities (`Arc` clones under the read lock) and of every
  window that a page has filled, that is at the present, not loading and not stale: its newest
  50 sent messages and when it was viewed. Windows are written only once a READY has
  confirmed the session; before it, a restore's entities are all the file gets. The cache thread compares it with what it last wrote (`Arc::ptr_eq`, then `==`) and
  writes only the difference in one transaction. A READY that rebuilds every value with the
  same content writes nothing. The first comparison is against what the file held at open.
- **Settings** are diffed the same way: one row per guild whose settings changed, the folders
  and the notification flags only when they did. Category collapse isn't cached (the official
  clients don't sync it; Akari keeps it in the app).
- **Caps:** the newest 50 messages of the 50 most recently viewed channels. A window that
  drops out of memory keeps its rows in the file; past the 50th channel the least recently
  viewed lose theirs, and so do channels that are gone. Free pages past 256 go back to the
  file system. After every write that changed something the WAL is checkpointed and
  truncated: the app quits without closing the file, and the WAL would otherwise keep rows
  that `secure_delete` already cleared from the database. A checkpoint that a reader or writer
  kept from finishing (`wal_checkpoint`'s busy column) counts as a failed write, and every
  later flush tries it again until it completes.
- **A failed write** is logged and rolled back; the next flush tries everything again.

## Cold start

1. With a last account, the host starts the warm-up and `preload(user)` while it reads the
   token; the cache thread opens the file and reads the entities.
2. `account(token, user)` takes the preloaded thread (or starts one) and returns at once. As
   soon as the entities are read, the store takes them before the first READY
   (`StoreEvent::Restored`: read everything, as after READY).
3. The host shows the rail (in the user's order, with folders), the channel list and the last
   channel from them.
4. The first READY is compared with the restored state like a new session, so only real
   changes become events. Windows loaded between the restore and that READY are marked stale
   and refreshed after it. A READY that arrives before the cache was read, or is being
   prepared when it is, wins, and the cache's entities aren't used (`cache not used
   reason=ready_first`).

A channel's cached messages are read only when it opens: the first page's request and the
cache read start together. If the cache answers first, its messages fill the empty window as
stale ("Catching up…"), and the page then reconciles them like a refresh (edits, deletes and
new messages within its range) or, when it can't reach them, replaces them with the present.
Cached messages older than the page's first one are dropped, since nothing confirmed them; the
reader is at the bottom then, and the list keeps their place.
A page never waits for the disk. If the page fails, e.g. offline, the cached messages still
show and stay stale: before the first READY, READY's refresh takes them; after it, a refresh
starts at once, with its retries, and a window it can't refresh leaves the present (live
messages aren't held back behind it).

## The account READY says

The host names the account (it keys the token store by it), but READY decides. If READY's
user differs, a host bug, the cache moves to that user's file and leaves the other one as it
was. Nothing is read from the other file once the session's user is known: neither its
entities (`cache not used reason=other_user`) nor its windows.

## Deleting

- `forget_account(user)` deletes the token, then the cache, also while the account is open
  (its thread deletes the files and stops; later flushes write nothing). Only opening the
  account again, `account()` or `preload()`, brings a file back.
- `logout(user)` does the same after Discord's answer.
- When Discord ends the session (gateway close 4004 or a 401), the core deletes the cache
  itself as the account closes.
- `clear_caches()` deletes every account's files that no open account is using; the apps call
  it at a launch without an account, which removes what a crash between deleting a token and
  its cache left behind.

## Logs

Target `akari_core::cache`, `info`, numbers only, never content, names or IDs:

- `cache opened guilds=… channels=… windows=… messages=… skipped=… rebuilt=… read_ms=… file_kib=…`
- `cache written guilds=… channels=… windows=… messages=… other=… deleted=… snapshot_ms=… write_ms=… file_kib=…`
- `cache discarded reason=…`, `cache unavailable reason=…`, `cache not used reason=…`
  (`ready_first`, `other_user`), `cache not in WAL mode reason=journal_mode`,
  `cache write failed reason=…`

`skipped` counts rows that didn't decode; they're left out of the restore. `other` counts the
current user, DM recipients, memberships, unavailable guilds and settings rows. `snapshot_ms` (to a
tenth) is how long taking the state from the store took. For close's snapshot that time is
spent on the thread that called `close()`, the main thread on logout and quit, so close's
write is logged even when it has nothing to write.

## Later

- iOS: a file protection class for the files, so the cache stays readable while the app runs
  in the background but not while the device is locked before its first unlock.
- The other apps' directories.
