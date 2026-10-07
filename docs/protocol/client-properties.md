# Client properties

The "super properties" describe the client. Akari sends the same object as `properties`
in gateway Identify and, base64-encoded, as the `X-Super-Properties` header on every REST
request. `akari_core::properties::ClientProperties` builds it.

Source: [Client Properties](https://docs.discord.food/reference#client-properties). The
reference says no field is strictly required. It calls the header "highly recommended due
to its significance in anti-abuse systems", and says experimental features need a recent
`client_build_number`.

## Which client Akari claims to be

Akari presents itself as the **official desktop client** of the host OS: `browser:
"Discord Client"`, `release_channel: "stable"`, the desktop host version and the Electron
User-Agent. A different identity, such as a browser name of "Akari", would stand out to
anti-abuse systems and draw captchas. Decided on 2026-10-07.

`browser_user_agent` is also the `User-Agent` header of every HTTP request and WebSocket
upgrade. The reference says the two must match, and building both from one field means
they can't drift apart.

## Fields

`ClientProperties::desktop(&HostInfo, &ClientBuild)` follows the reference's macOS and
Linux examples:

| Field | Value |
|---|---|
| `os` | `Mac OS X` or `Linux` |
| `browser` | `Discord Client` |
| `release_channel` | `stable` |
| `client_version` | Desktop host version (`ClientBuild::client_version`) |
| `os_version` | Kernel release (`uname -r`) |
| `os_arch`, `app_arch` | `arm64` or `x64` |
| `system_locale` | BCP 47 tag; also the `X-Discord-Locale` header |
| `has_client_mods` | `false` |
| `browser_user_agent` | `Mozilla/5.0 (<platform>) AppleWebKit/537.36 (KHTML, like Gecko) discord/<client_version> Chrome/<chrome> Electron/<electron> Safari/537.36` |
| `browser_version` | Electron version |
| `os_sdk_version` | macOS only: the Darwin major version |
| `client_build_number` | Web build number |
| `native_build_number`, `client_event_source` | `null` |

`<platform>` is `Macintosh; Intel Mac OS X 10_15_7` on macOS, as in every Chromium on a
Mac, and `X11; Linux x86_64` or `X11; Linux aarch64` on Linux.

Not sent (decision, effect on anti-abuse **unverified**): the per-launch UUIDs
`client_launch_id`, `launch_signature` and `client_heartbeat_session_id`, the
Identify-only `installation_id`, and the Linux-only `window_manager` and `distro`.

## Current values

`ClientBuild::current(os)` holds the defaults, last refreshed on 2026-10-07:

| Value | Default | Where it came from |
|---|---|---|
| `client_build_number` | 631730 | `GLOBAL_ENV.BUILD_NUMBER` in the HTML of `https://discord.com/app` |
| `client_version` (macOS) | 0.0.415 | `name` in `GET https://discord.com/api/updates/stable?platform=osx` |
| `client_version` (Linux) | 1.0.161 | `name` in `GET https://discord.com/api/updates/stable?platform=linux` |
| Electron | 42.11.10 | `CFBundleVersion` of `Electron Framework.framework/Resources/Info.plist` in the macOS app from the `url` of the osx update response |
| Chrome | 148.0.7778.280 | `chrome` of that Electron version in `https://releases.electronjs.org/releases.json` |

The Linux app is assumed to ship the same Electron version as the macOS app
(**unverified**: its archive is a tar.gz without an index, so the version can't be read
without downloading all of it).

## Refreshing them

Refresh when Discord ships a new desktop host or Electron version, or every few weeks;
an old build number makes experiment-gated features disappear. Update
`ClientBuild::current` and the table above:

```sh
curl -s https://discord.com/app | grep -o 'BUILD_NUMBER":"[0-9]*'
curl -s 'https://discord.com/api/updates/stable?platform=osx'
curl -s 'https://discord.com/api/updates/stable?platform=linux'
# Electron: open Discord.app/Contents/Frameworks/Electron Framework.framework/Resources/Info.plist
# from the downloaded zip (CFBundleVersion), then look up its "chrome" entry:
curl -s https://releases.electronjs.org/releases.json | jq -r '.[] | select(.version=="<electron>") | .chrome'
```

## Overriding at runtime

A host app can change any value without a new Akari release, for example from remote
configuration. It can pass its own `ClientBuild` to `ClientProperties::desktop`, or edit
the public fields of the result before creating the `DiscordClient`. The `DiscordClient`
then sends those values everywhere: in Identify, `X-Super-Properties`, `User-Agent` and
`X-Discord-Locale`.
