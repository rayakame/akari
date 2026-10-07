use std::process::Command;

use akari_core::properties::{Arch, DesktopOs, HostInfo};

pub fn host_info() -> Result<HostInfo, String> {
    let os = if cfg!(target_os = "macos") {
        DesktopOs::MacOs
    } else if cfg!(target_os = "linux") {
        DesktopOs::Linux
    } else {
        return Err("akari-cli runs on macOS and Linux only".to_owned());
    };
    let arch = Arch::current().ok_or("Discord has no desktop client for this CPU")?;
    Ok(HostInfo {
        os,
        os_version: kernel_release(),
        arch,
        system_locale: locale_from_lang(std::env::var("LANG").ok().as_deref()),
    })
}

fn kernel_release() -> String {
    Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|release| release.trim().to_owned())
        .filter(|release| !release.is_empty())
        .unwrap_or_else(|| "unknown".to_owned())
}

// `de_DE.UTF-8` → `de-DE`; `C` and `POSIX` mean no locale.
fn locale_from_lang(lang: Option<&str>) -> String {
    let tag = lang
        .and_then(|lang| lang.split(['.', '@']).next())
        .filter(|tag| !tag.is_empty() && *tag != "C" && *tag != "POSIX");
    match tag {
        Some(tag) => tag.replace('_', "-"),
        None => "en-US".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_locales_become_bcp47_tags() {
        assert_eq!(locale_from_lang(Some("de_DE.UTF-8")), "de-DE");
        assert_eq!(locale_from_lang(Some("pt_BR.UTF-8@euro")), "pt-BR");
        assert_eq!(locale_from_lang(Some("fr")), "fr");
    }

    #[test]
    fn missing_or_c_locales_fall_back_to_english() {
        assert_eq!(locale_from_lang(None), "en-US");
        assert_eq!(locale_from_lang(Some("")), "en-US");
        assert_eq!(locale_from_lang(Some("C")), "en-US");
        assert_eq!(locale_from_lang(Some("POSIX")), "en-US");
        assert_eq!(locale_from_lang(Some("C.UTF-8")), "en-US");
    }
}
