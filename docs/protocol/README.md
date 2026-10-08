# Protocol notes

Notes on the Discord user-account API as Akari uses it: the gateway (connecting,
identify and resume, zstd-stream compression, opcodes, events), REST (endpoints, rate
limits, error codes), data models, and observed behavior the reference doesn't cover.

The main reference is [Discord Userdoccers](https://docs.discord.food). These notes
don't copy it. They record what Akari relies on, the decisions we made, and the gaps or
surprises we found.

## Files

- [client-properties.md](client-properties.md): the client identity Akari sends and how to refresh it
- [gateway.md](gateway.md): payload envelope, opcodes, Hello
- [login.md](login.md): email/password and QR code login, logout, token storage
- [ready.md](ready.md): READY, gateway capabilities, gateway guilds
- [models.md](models.md): users, guilds, channels and messages, and how they map to Rust

## Conventions

- One file per topic, e.g. `gateway.md`, `rate-limits.md`, `read-states.md`.
- Link the relevant docs.discord.food page for every claim, or say how it was observed.
- Mark anything not yet confirmed against real traffic as **unverified**.
- Never include tokens, cookies, real user/guild/channel IDs, message content or other
  personal data. Use obviously fake values in examples.
