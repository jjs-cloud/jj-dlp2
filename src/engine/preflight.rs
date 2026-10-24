//! Setup checks for things yt-dlp needs but the app does not ship: ffmpeg and a Firefox profile.

use super::{exepath, hide_window, template};
use crate::api::{Notice, Site};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

/// Checks the environment against the configured sites and returns what the user must fix.
pub async fn run(sites: Vec<Site>) -> Vec<Notice> {
    let mut notices = Vec::new();
    if !ffmpeg_available(&sites).await {
        notices.push(ffmpeg_notice());
    }
    if uses_firefox_cookies(&sites) && !has_firefox_profile(&firefox_roots()) {
        notices.push(firefox_notice());
    }
    notices
}

/// The command that installs ffmpeg on this OS, when it is known.
pub fn install_command() -> Option<&'static str> {
    if cfg!(windows) {
        Some("winget install --id Gyan.FFmpeg --exact")
    } else if cfg!(target_os = "macos") {
        Some("brew install ffmpeg")
    } else {
        linux_install_command(&std::fs::read_to_string("/etc/os-release").unwrap_or_default())
    }
}

/// Picks a package-manager command from the ID and ID_LIKE values of an os-release file.
fn linux_install_command(os_release: &str) -> Option<&'static str> {
    let mut ids: Vec<String> = Vec::new();
    for line in os_release.lines() {
        if let Some((key, value)) = line.split_once('=') {
            if key == "ID" || key == "ID_LIKE" {
                ids.extend(value.trim_matches(['"', '\'']).split_whitespace().map(str::to_lowercase));
            }
        }
    }
    ids.iter().find_map(|id| match id.as_str() {
        "debian" | "ubuntu" | "linuxmint" | "pop" | "raspbian" => Some("sudo apt install ffmpeg"),
        "fedora" | "rhel" | "centos" | "rocky" | "almalinux" => Some("sudo dnf install ffmpeg-free"),
        "arch" | "manjaro" | "endeavouros" => Some("sudo pacman -S ffmpeg"),
        "opensuse" | "opensuse-leap" | "opensuse-tumbleweed" | "suse" | "sles" => Some("sudo zypper install ffmpeg"),
        "alpine" => Some("sudo apk add ffmpeg"),
        "void" => Some("sudo xbps-install ffmpeg"),
        _ => None,
    })
}

async fn ffmpeg_available(sites: &[Site]) -> bool {
    let sets_location = |s: &Site| s.checker_args.contains("--ffmpeg-location") || s.recorder_args.contains("--ffmpeg-location");
    if sites.iter().any(sets_location) {
        return true;
    }
    if cfg!(windows) && sites.iter().any(|s| ffmpeg_beside_ytdlp(&s.exe_path)) {
        return true;
    }
    let mut cmd = Command::new("ffmpeg");
    cmd.arg("-version").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).kill_on_drop(true);
    hide_window(&mut cmd);
    matches!(tokio::time::timeout(Duration::from_secs(10), cmd.status()).await, Ok(Ok(s)) if s.success())
}

fn ffmpeg_beside_ytdlp(exe_path: &str) -> bool {
    let exe = exepath::resolve(exe_path).exe;
    Path::new(&exe).parent().is_some_and(|dir| dir.join("ffmpeg.exe").is_file())
}

fn ffmpeg_notice() -> Notice {
    let command = install_command();
    let steps = if cfg!(windows) {
        "Recording needs ffmpeg. Install it with the command below, then restart jj-dlp.".to_string()
    } else if command.is_some() {
        "Recording needs ffmpeg. Install it with the command below, then press Re-check.".to_string()
    } else {
        "Recording needs ffmpeg. Install it with your package manager, then press Re-check.".to_string()
    };
    Notice { title: "ffmpeg was not found".to_string(), steps, command: command.map(str::to_string) }
}

fn firefox_notice() -> Notice {
    Notice {
        title: "Firefox has no profile yet".to_string(),
        steps: "Your sites read cookies from Firefox. Open Firefox once (log in to the site if you want its cookies), then press Re-check.".to_string(),
        command: None,
    }
}

/// The browser named by `--cookies-from-browser` in the tokens, lowercased.
fn cookies_browser(tokens: &[String]) -> Option<String> {
    let mut it = tokens.iter();
    while let Some(t) = it.next() {
        let value = if t == "--cookies-from-browser" {
            it.next().map(String::as_str)
        } else {
            t.strip_prefix("--cookies-from-browser=")
        };
        if let Some(v) = value {
            return Some(v.to_lowercase());
        }
    }
    None
}

fn uses_firefox_cookies(sites: &[Site]) -> bool {
    sites.iter().any(|s| {
        [s.checker_args.as_str(), s.recorder_args.as_str()]
            .into_iter()
            .any(|args| cookies_browser(&template::tokenize(args)).as_deref() == Some("firefox"))
    })
}

/// Folders where Firefox keeps its profiles on this OS.
fn firefox_roots() -> Vec<PathBuf> {
    let Some(base) = directories::BaseDirs::new() else { return Vec::new() };
    let home = base.home_dir();
    if cfg!(windows) {
        vec![base.config_dir().join("Mozilla").join("Firefox").join("Profiles")]
    } else if cfg!(target_os = "macos") {
        vec![home.join("Library/Application Support/Firefox/Profiles")]
    } else {
        [
            ".mozilla/firefox",
            ".config/mozilla/firefox",
            ".var/app/org.mozilla.firefox/.mozilla/firefox",
            ".var/app/org.mozilla.firefox/config/mozilla/firefox",
            "snap/firefox/common/.mozilla/firefox",
        ]
        .iter()
        .map(|p| home.join(p))
        .collect()
    }
}

fn has_firefox_profile(roots: &[PathBuf]) -> bool {
    roots.iter().any(|r| contains_file(r, "cookies.sqlite", 3))
}

fn contains_file(dir: &Path, name: &str, depth: u32) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else { return false };
    entries.flatten().any(|e| {
        let path = e.path();
        if path.is_dir() {
            depth > 0 && contains_file(&path, name, depth - 1)
        } else {
            e.file_name().to_str() == Some(name)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("jj-preflight-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn picks_install_command_from_os_release() {
        assert_eq!(linux_install_command("ID=ubuntu\nID_LIKE=debian\n"), Some("sudo apt install ffmpeg"));
        assert_eq!(linux_install_command("ID=linuxmint\nID_LIKE=\"ubuntu debian\"\n"), Some("sudo apt install ffmpeg"));
        assert_eq!(linux_install_command("ID=\"fedora\"\n"), Some("sudo dnf install ffmpeg-free"));
        assert_eq!(linux_install_command("ID=cachyos\nID_LIKE=arch\n"), Some("sudo pacman -S ffmpeg"));
        assert_eq!(linux_install_command("ID=plan9\n"), None);
        assert_eq!(linux_install_command(""), None);
    }

    #[test]
    fn reads_cookie_browser_from_args() {
        let t = |s: &str| template::tokenize(s);
        assert_eq!(cookies_browser(&t("--cookies-from-browser Firefox --dump-json {urls}")).as_deref(), Some("firefox"));
        assert_eq!(cookies_browser(&t("--cookies-from-browser=chrome {url}")).as_deref(), Some("chrome"));
        assert_eq!(cookies_browser(&t("--dump-json {urls}")), None);
    }

    #[test]
    fn finds_firefox_cookie_database() {
        let root = temp("ff");
        let missing = root.join("nope");
        let profiles = root.join("firefox");
        std::fs::create_dir_all(profiles.join("abc.default-release")).unwrap();
        assert!(!has_firefox_profile(&[missing.clone(), profiles.clone()]));
        std::fs::write(profiles.join("abc.default-release").join("cookies.sqlite"), b"x").unwrap();
        assert!(has_firefox_profile(&[missing, profiles]));
        let _ = std::fs::remove_dir_all(root);
    }
}
