//! The client properties ("super properties") Akari sends in Identify and in the
//! `X-Super-Properties` header. See `docs/protocol/client-properties.md`.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::Serialize;

/// The official desktop client build Akari presents itself as.
///
/// [`ClientBuild::current`] holds the pinned defaults; a host can override any value at
/// runtime, for example from remote configuration, without a new Akari release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientBuild {
    pub client_build_number: u32,
    /// Version of the desktop host app, such as `0.0.415`.
    pub client_version: String,
    pub electron_version: String,
    pub chrome_version: String,
}

impl ClientBuild {
    /// The defaults, last refreshed on 2026-10-07.
    pub fn current(os: DesktopOs) -> Self {
        Self {
            client_build_number: 631_730,
            client_version: match os {
                DesktopOs::MacOs => "0.0.415",
                DesktopOs::Linux => "1.0.161",
            }
            .to_owned(),
            electron_version: "42.11.10".to_owned(),
            chrome_version: "148.0.7778.280".to_owned(),
        }
    }
}

/// The machine Akari runs on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostInfo {
    pub os: DesktopOs,
    /// Kernel release, as `uname -r` prints it.
    pub os_version: String,
    pub arch: Arch,
    /// BCP 47 tag such as `en-US`.
    pub system_locale: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopOs {
    MacOs,
    Linux,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    Arm64,
    X64,
}

impl Arch {
    /// The architecture this binary was built for, if Discord ships a client for it.
    pub fn current() -> Option<Self> {
        Self::from_rust(std::env::consts::ARCH)
    }

    fn from_rust(arch: &str) -> Option<Self> {
        match arch {
            "aarch64" => Some(Self::Arm64),
            "x86_64" => Some(Self::X64),
            _ => None,
        }
    }

    fn discord_name(self) -> &'static str {
        match self {
            Self::Arm64 => "arm64",
            Self::X64 => "x64",
        }
    }
}

/// Client properties as Discord expects them.
///
/// `browser_user_agent` is also sent as the `User-Agent` header of every HTTP and
/// WebSocket request, so the two always match.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClientProperties {
    pub os: String,
    pub browser: String,
    pub release_channel: String,
    pub client_version: String,
    pub os_version: String,
    pub os_arch: String,
    pub app_arch: String,
    pub system_locale: String,
    pub has_client_mods: bool,
    pub browser_user_agent: String,
    pub browser_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_sdk_version: Option<String>,
    pub client_build_number: u32,
    pub native_build_number: Option<u32>,
    pub client_event_source: Option<String>,
}

impl ClientProperties {
    /// The properties of the official desktop client `build` running on `host`.
    pub fn desktop(host: &HostInfo, build: &ClientBuild) -> Self {
        let (os, platform) = match host.os {
            DesktopOs::MacOs => ("Mac OS X", "Macintosh; Intel Mac OS X 10_15_7"),
            DesktopOs::Linux => (
                "Linux",
                match host.arch {
                    Arch::Arm64 => "X11; Linux aarch64",
                    Arch::X64 => "X11; Linux x86_64",
                },
            ),
        };
        let user_agent = format!(
            "Mozilla/5.0 ({platform}) AppleWebKit/537.36 (KHTML, like Gecko) discord/{} \
             Chrome/{} Electron/{} Safari/537.36",
            build.client_version, build.chrome_version, build.electron_version
        );
        let arch = host.arch.discord_name();
        Self {
            os: os.to_owned(),
            browser: "Discord Client".to_owned(),
            release_channel: "stable".to_owned(),
            client_version: build.client_version.clone(),
            os_version: host.os_version.clone(),
            os_arch: arch.to_owned(),
            app_arch: arch.to_owned(),
            system_locale: host.system_locale.clone(),
            has_client_mods: false,
            browser_user_agent: user_agent,
            browser_version: build.electron_version.clone(),
            os_sdk_version: match host.os {
                DesktopOs::MacOs => host.os_version.split('.').next().map(str::to_owned),
                DesktopOs::Linux => None,
            },
            client_build_number: build.client_build_number,
            native_build_number: None,
            client_event_source: None,
        }
    }

    pub(crate) fn super_properties(&self) -> String {
        // Serializing plain strings, numbers and options can't fail.
        let json = serde_json::to_vec(self).unwrap_or_default();
        STANDARD.encode(json)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn mac() -> HostInfo {
        HostInfo {
            os: DesktopOs::MacOs,
            os_version: "25.0.0".to_owned(),
            arch: Arch::Arm64,
            system_locale: "de-DE".to_owned(),
        }
    }

    #[test]
    fn desktop_properties_have_the_official_macos_shape() {
        let build = ClientBuild {
            client_build_number: 123_456,
            client_version: "0.0.400".to_owned(),
            electron_version: "40.1.2".to_owned(),
            chrome_version: "140.0.1.2".to_owned(),
        };

        let properties = ClientProperties::desktop(&mac(), &build);

        assert_eq!(
            serde_json::to_value(&properties).unwrap(),
            json!({
                "os": "Mac OS X",
                "browser": "Discord Client",
                "release_channel": "stable",
                "client_version": "0.0.400",
                "os_version": "25.0.0",
                "os_arch": "arm64",
                "app_arch": "arm64",
                "system_locale": "de-DE",
                "has_client_mods": false,
                "browser_user_agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
                    AppleWebKit/537.36 (KHTML, like Gecko) discord/0.0.400 \
                    Chrome/140.0.1.2 Electron/40.1.2 Safari/537.36",
                "browser_version": "40.1.2",
                "os_sdk_version": "25",
                "client_build_number": 123_456,
                "native_build_number": null,
                "client_event_source": null,
            })
        );
    }

    #[test]
    fn linux_user_agent_names_x11_and_the_cpu() {
        let host = HostInfo {
            os: DesktopOs::Linux,
            os_version: "6.8.0-45-generic".to_owned(),
            arch: Arch::X64,
            system_locale: "en-US".to_owned(),
        };

        let properties = ClientProperties::desktop(&host, &ClientBuild::current(DesktopOs::Linux));

        assert_eq!(properties.os, "Linux");
        assert_eq!(properties.os_arch, "x64");
        assert_eq!(properties.os_sdk_version, None);
        assert!(
            properties
                .browser_user_agent
                .starts_with("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36"),
            "{}",
            properties.browser_user_agent
        );
    }

    #[test]
    fn current_build_is_the_pinned_default() {
        let build = ClientBuild::current(DesktopOs::MacOs);
        let properties = ClientProperties::desktop(&mac(), &build);

        assert_eq!(properties.client_build_number, build.client_build_number);
        assert!(
            properties
                .browser_user_agent
                .contains(&format!("discord/{} ", build.client_version))
        );
    }

    #[test]
    fn super_properties_are_base64_json() {
        let properties = ClientProperties::desktop(&mac(), &ClientBuild::current(DesktopOs::MacOs));

        let decoded = STANDARD.decode(properties.super_properties()).unwrap();
        let value: Value = serde_json::from_slice(&decoded).unwrap();

        assert_eq!(value, serde_json::to_value(&properties).unwrap());
    }

    #[test]
    fn arch_maps_rust_names() {
        assert_eq!(Arch::from_rust("aarch64"), Some(Arch::Arm64));
        assert_eq!(Arch::from_rust("x86_64"), Some(Arch::X64));
        assert_eq!(Arch::from_rust("riscv64"), None);
    }
}
