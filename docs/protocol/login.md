# Login

How `akari_core::auth` logs in, and what it relies on. The login screen shows the
email/password form and the QR code side by side, so both flows can run at the same time
on one `DiscordClient` and each can be cancelled on its own.

Sources: [Authentication](https://docs.discord.food/authentication),
[CAPTCHA Handling](https://docs.discord.food/topics/captcha-handling),
[Rate Limits](https://docs.discord.food/topics/rate-limits),
[JSON error codes](https://docs.discord.food/api/codes) and
[Remote Authentication](https://docs.discord.food/remote-authentication/overview).

Everything here is **unverified** against real traffic unless it says otherwise. The QR code
login worked end to end against Discord on 2026-10-08 (`akari-cli login --qr`).

## Requests

REST goes to `https://discord.com/api/v9` (v9 is the client default,
[API versioning](https://docs.discord.food/reference#api-versioning)). Every request
carries the client headers from [client-properties.md](client-properties.md):
`User-Agent`, `X-Super-Properties` and `X-Discord-Locale`. Authenticated requests carry
the bare token in `Authorization`, with no `Bearer` prefix.

The REST client only speaks HTTPS and never follows redirects, so the `Authorization`
header can't be sent anywhere a response points to. A redirect reaches the caller as an
unexpected status. Response bodies are capped at 4 MiB; a larger one ends the request
with `UnexpectedResponse`. Plain `http://` and `ws://` endpoints need
`Endpoints::allow_plaintext`, which only local test servers set.

Requests before login also carry `X-Fingerprint`
([fingerprints](https://docs.discord.food/authentication#fingerprints)). The fingerprint
comes from `GET /experiments`, which returns one when the request has neither
authorization nor a fingerprint. `POST /auth/fingerprint` still works but is deprecated.
A `DiscordClient` fetches the fingerprint once and shares it between all its logins. A
failed fetch isn't cached, and the login goes ahead without one, since the reference says
"should", not "must". Discord hands out at most 3 valid fingerprints per IP every 2
minutes; beyond that it still returns fingerprints, but they aren't valid.

## Email and password

`PasswordLogin` is a step machine the UI drives. Each call returns the next `LoginStep`:
`Done`, `Captcha`, `Mfa` or `NewLocation`. The flow keeps the login and password only
until it finishes or is cancelled, because a captcha or a new login location makes it send
the login again.

```
submit(login, password) ── POST /auth/login ─┬─ token ─────────────────────► Done
                                             ├─ {mfa, ticket, …} ──────────► Mfa
                                             ├─ 400 captcha_key ───────────► Captcha
                                             ├─ ACCOUNT_LOGIN_VERIFICATION_EMAIL ► NewLocation(Email)
                                             └─ 70007 ─────────────────────► NewLocation(Phone)
solve_captcha(solution) ── same request again, with X-Captcha-* headers
submit_mfa(method, code) ── POST /auth/mfa/{totp|sms|backup|webauthn} ─► Done
send_mfa_sms() ── POST /auth/mfa/sms/send ─► Mfa with sms_sent_to
confirm_new_location(link) ── POST /auth/authorize-ip ─► login again
verify_phone(code) ── POST /phone-verifications/verify ─► authorize-ip ─► login again
```

`POST /auth/login` takes `{login, password, undelete: false}`. `login` is an email
address or an E.164 phone number. A success returns `{user_id, token, user_settings}`.
`required_actions: ["update_password"]` becomes `LoginSuccess::password_update_required`.

A step that fails with a recoverable error stays pending. For example, a wrong MFA code
(`InvalidMfaCode`) keeps the MFA step, and a wrong password lets the UI submit again. A
second call while one is running returns `Busy`. A call no step is waiting for returns
`NoPendingStep`. A pending step stays in the flow until the request that answers it
finishes, so dropping a call's future (a cancelled UI task, a timeout) doesn't lose the MFA
ticket or a verification token.

`cancel()` stops the running request and makes every later call return `Cancelled`. It
drops the login, password, MFA ticket and any pending code or verification token at once:
they are kept apart from the step state, so a running, stalled or dropped step can't hold
on to them. A request that was already sent keeps its own copy of the body until it ends
(see below on zeroing).

### Two-factor authentication

With MFA on, login returns `{user_id, mfa: true, ticket, login_instance_id, totp, sms,
backup, webauthn}`. `webauthn` is the stringified `PublicKeyCredentialRequestOptions`, or
`null`. `MfaChallenge::methods` lists the methods that are available. Codes go to
`POST /auth/mfa/{type}` as `{ticket, login_instance_id, code}`, which returns `{token}`.
`user_id` isn't part of that response, so Akari keeps it from the login response. SMS
needs `POST /auth/mfa/sms/send {ticket}` first, which returns the masked number
(`{phone: "+*******0085"}`). The `totp` endpoint also accepts backup codes. For WebAuthn,
`code` is the JSON of `PublicKeyCredential.toJSON()`.

### CAPTCHA

Any endpoint can demand a CAPTCHA: HTTP 400 with a `captcha_key` field. The reference
says to check for the field, not for particular values inside it. The response carries
`captcha_service` (`hcaptcha` for login), `captcha_sitekey` (dynamic, never hard-code it),
and optionally `captcha_rqdata`, `captcha_rqtoken`, `captcha_session_id` and
`should_serve_invisible`. A host renders the challenge in a web view; if `rqdata` is
present it must be passed to the challenge.

The solution goes back with the same request, retried with the `X-Captcha-Key` header,
plus `X-Captcha-Rqtoken` and `X-Captcha-Session-Id` when the challenge had them. The
reference calls the old `captcha_key`/`captcha_rqtoken` body fields deprecated, so Akari
never sends them. A rejected solution returns another challenge. `PasswordLogin` replays
whichever request asked: login, MFA, SMS, authorize-ip or phone verification. While a
CAPTCHA is pending for an MFA request, `submit_mfa` and `send_mfa_sms` still work with the
same ticket.

The apps can't show a CAPTCHA yet: a web view sheet for hCaptcha is a later milestone, and
until then they say so. A password login from a new device often gets one, so QR code login
is the reliable way in for now. After a new login location is confirmed, retrying the login
shouldn't need another CAPTCHA
([Login Account](https://docs.discord.food/authentication#login-account)).

### New login location

When an account without MFA logs in from a new location, Discord can refuse and ask for
confirmation:

- **Email login**: Discord emails a link that ends up at
  `https://discord.com/authorize-ip#token=…`. The reference doesn't show the rejected
  response. Akari treats a form error on `login` with the code
  `ACCOUNT_LOGIN_VERIFICATION_EMAIL` as this case (**unverified**). The link opens the
  official client, so the user pastes the address (or just the token) into Akari:
  `confirm_new_location` accepts `#token=…`, `?token=…` or a bare token, also when the
  pasted address lacks `https://` or the token is percent-encoded.
- **Phone login**: error `70007`, and Discord texts a code.
  `POST /phone-verifications/verify {phone, code}` returns `{token}`. Akari only treats
  70007 as this step when the login is an E.164 number (starts with `+`). For an email
  login it comes back as `LoginError::Discord`, since it can't be about verifying that
  login's phone number.

Either token goes to `POST /auth/authorize-ip {token}` (204), and then the login is sent
again. The reference says no new CAPTCHA should be needed then.

## QR code

`QrLogin` runs the [desktop side of remote
authentication](https://docs.discord.food/remote-authentication/desktop) in a background
task. The UI reads events with `next()`: `Code`, `Scanned`, `CancelledOnPhone`, `Captcha`
and `Done`. `DiscordClient::qr_login` spawns that task on the current Tokio runtime and
returns `LoginError::NoRuntime` when there is none.

### The remote auth gateway

Akari connects to `wss://remote-auth-gateway.discord.gg/?v=2`. The version parameter is
required; v1 is discontinued. The gateway rejects connections without an `Origin` of
`https://discord.com` (or the ptb or canary origins). Akari also sends its `User-Agent`,
and uses the same rustls config as everything else. Packets are flat JSON objects under
1 KiB, so the connection refuses messages and frames over 16 KiB. Each has a string `op`:

| Direction | `op` | Fields | What Akari does |
|---|---|---|---|
| ← | `hello` | `heartbeat_interval`, `timeout_ms` | Starts heartbeating and sends `init` |
| → | `init` | `encoded_public_key` | SPKI DER of a fresh RSA-2048 key, standard base64 |
| ← | `nonce_proof` | `encrypted_nonce` | Decrypts the nonce |
| → | `nonce_proof` | `nonce` | The decrypted nonce, base64url without padding |
| ← | `pending_remote_init` | `fingerprint` | Checks it, then emits `Code { url: "https://discord.com/ra/<fingerprint>" }` |
| ← | `pending_ticket` | `encrypted_user_payload` | Emits `Scanned`; an unreadable payload restarts the session |
| ← | `pending_login` | `ticket` | Exchanges the ticket for the token |
| ← | `cancel` | | Emits `CancelledOnPhone` and starts over |
| → / ← | `heartbeat` / `heartbeat_ack` | | As on the main gateway |

Before January 2026 the nonce proof was the SHA-256 of the nonce; now it's the nonce
itself (commit `c97d05b508` in the reference's repository). The fingerprint is the
base64url (no padding) SHA-256 of our SPKI DER. The reference's own example pair confirms
this, and Akari's tests check against it. If the fingerprint doesn't match, Akari closes
the connection and starts over, as the reference recommends.

All three ciphertexts (nonce, user payload, token) are RSA-OAEP with SHA-256 as both the
hash and the MGF1 hash, with no label. Akari uses aws-lc-rs for the RSA work: rustls
already builds it, and the `rsa` crate has an unpatched timing advisory (RUSTSEC-2023-0071).
Every session generates a new key on a blocking thread, so every new connection shows a
new QR code. The decrypted nonce and token are zeroed after use.

The user payload is `id:discriminator:avatar:username`, with `0` for no avatar. Akari
splits it into at most four parts, so a colon in the username survives.

### Ticket exchange

`POST /users/@me/remote-auth/login {ticket}` (unauthenticated, with the fingerprint)
returns `{encrypted_token}`. Like any endpoint it can demand a CAPTCHA: `QrLogin` then
emits `Captcha`, waits for `solve_captcha` and retries with the `X-Captcha-*` headers.
The ticket's lifetime isn't documented.

### Sessions end and restart

| Event | Close code | Akari |
|---|---|---|
| Finished or cancelled | 1000 | After `pending_login` or `cancel`; anything else is a failure, since the reference says 1000 can also mean a protocol error |
| Session timed out | 4003 | After a code was shown: starts over at once, and the UI shows the new code. The example `timeout_ms` is about 2.4 minutes. Before any code: a failure |
| Handshake failure | 4002 | Failure |
| Decode error | 4001 | Failure. The reference's prose still calls 4001 the handshake failure (left over from before the codes were swapped in December 2024), so both count the same |
| Invalid version | 4000 | Failure |
| No heartbeat ACK, broken connection | none | Failure before a code was shown, otherwise starts over |
| No `hello` within 10 s | none | Failure; without `hello` there is no heartbeat to notice a silent connection |

A failure means the session never got as far as showing a code. After three failures in a
row, `next()` returns the last error: `Network` for a connection error, otherwise
`RemoteAuth`.

A session that lived its normal few minutes starts over at once. One that ended within
30 s, whether it failed or showed a code, starts over with backoff: at once the first time,
then after 0.5–1 s, 1–2 s and so on, up to 30 s. That way a gateway that drops every code
right away can't make Akari reconnect in a tight loop with a new RSA key each time. A
cancel on the phone is a normal user action, not a quick restart: the next code shows at
once and doesn't count toward the backoff.

Events wait in a queue instead of a channel, so the session keeps heartbeating while the
UI isn't reading. A new code replaces one that hasn't been read yet, so only the newest
code ever reaches the UI. The queue holds at most eight events and drops the oldest. The
end of the login (`Done` or an error) is always the newest event, so it is never dropped.
`cancel()` or dropping the `QrLogin` closes the connection. If the background task ever
stops unexpectedly, `next()` returns `RemoteAuth` instead of waiting forever.

### Side by side with the password form

Both flows use the same `DiscordClient`, and so share its fingerprint. They don't share
any other state: cancelling one doesn't affect the other. The UI cancels the other flow
once one of them returns a token.

## Errors

`LoginError` is typed, and its messages never contain secrets:

| Discord sends | `LoginError` |
|---|---|
| 50035 with a field error on `login` or `password` | `InvalidCredentials { message }`: Discord's message. The field codes, such as `INVALID_LOGIN`, are undocumented |
| Other 50035 | `Discord { code, message }` with the first field error's message |
| 20013 / 20011 | `AccountDisabled` / `AccountScheduledForDeletion`. Akari doesn't offer `undelete` yet |
| 403 `{user_id, suspended_user_token}` | `AccountSuspended`. The suspended token is a credential and is dropped unread |
| 60008 | `InvalidMfaCode`. The MFA step stays pending |
| 60006 / 60009 | `Expired`: the ticket is invalid, start again |
| 60010 / 70003 | `SmsUnavailable` |
| 40333, 403 + 10008, 403 without JSON | `Blocked`: Cloudflare or anti-abuse |
| 429 | `RateLimited { retry_after, global }`, from `retry_after` (float seconds) in the body or the `Retry-After` header |
| Any other JSON error | `Discord { code, message }` |
| No JSON, unexpected body, redirect, body over 4 MiB | `UnexpectedResponse` |

`retry_after` is `None` when neither the body nor the headers give a delay. The reference
says not to retry such a 429 automatically. Akari never retries login requests on its own.
10,000 invalid requests (401, 403, 429) from one IP within 10 minutes get that IP banned
for 24 hours.

## Logout

`DiscordClient::logout(account)` loads the stored token, sends `POST /auth/logout {}` with
it, and then deletes it from the token store. The body's fields (`provider`, `token`,
`voip_provider`, `voip_token`) are all optional push-notification tokens, so `{}` is valid,
and Akari registers none ([Logout](https://docs.discord.food/authentication#logout)). What
happens to a gateway connection that is still open isn't documented (**unverified**: likely a
4004 close). The token is deleted even when Discord can't be reached. A 401
counts as logged out, since the session is gone either way. Only `LogoutError::Storage`
means the token is still stored.

`DiscordClient::end_session(token)` sends the same request for a token that isn't in the
store, such as one a new login just replaced. Neither call can end a session Discord
doesn't hear about. When Discord can't confirm the logout, the host should tell the user
that the session may still be active, and that it can be ended under User Settings →
Devices in an official client. akari-cli does this, and it also ends a replaced token's
session after logging in again, so no live session is left behind a deleted token.

## Token storage

`TokenStore` is implemented by the host: Keychain on Apple platforms, Android Keystore,
libsecret on Linux. It is keyed by user ID, so several accounts can be stored. Its methods
are synchronous and may block or show a system dialog, so `DiscordClient` calls them only
through `spawn_blocking`.

## Keeping secrets out of logs

- `Token` and `Secret` print `<redacted>` in `Debug`, have no `Display`, and zero their
  memory on drop. Login steps and flows never print credentials, tickets or tokens, and
  neither do the CAPTCHA solution and per-request headers.
- Zeroing covers `Token`, `Secret` and the decrypted remote auth buffers only. The JSON
  request bodies built from them and reqwest's response buffers are ordinary copies that
  aren't zeroed. This is accepted: they live only for the length of one request.
- Error messages never quote response bodies; a failed parse of a body that may hold a
  token reports only `UnexpectedResponse`. Transport errors keep their source but strip
  its URL.
- `Authorization` and the three `X-Captcha-*` headers are marked sensitive, so reqwest
  leaves their values out of its own `Debug` output.
- tungstenite logs every frame at `trace` level through the `log` crate, so the remote auth
  ticket and, on the gateway, Identify and Resume with the token end up there (see
  [gateway.md](gateway.md#keeping-the-token-out-of-logs)). Hosts must not forward `log`
  records at `trace` level for `tungstenite`. akari-cli doesn't forward `log` records at
  all.
