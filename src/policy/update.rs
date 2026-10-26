//! Self-update: release lookup, download, verification and install.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
const RELEASES_URL: &str = "https://api.github.com/repos/jjs-cloud/jj-dlp2/releases?per_page=30";
const USER_AGENT: &str = concat!("jj-dlp/", env!("CARGO_PKG_VERSION"));

#[cfg(all(windows, target_arch = "x86_64"))]
const TARGET: &str = "x86_64-pc-windows-msvc";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const TARGET: &str = "x86_64-unknown-linux-gnu";
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const TARGET: &str = "x86_64-apple-darwin";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const TARGET: &str = "aarch64-apple-darwin";
#[cfg(not(any(
    all(windows, target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64")
)))]
const TARGET: &str = "";

#[cfg(windows)]
const ARCHIVE_EXT: &str = ".zip";
#[cfg(not(windows))]
const ARCHIVE_EXT: &str = ".tar.gz";

#[cfg(windows)]
const EXE_NAME: &str = "jj-dlp.exe";
#[cfg(not(windows))]
const EXE_NAME: &str = "jj-dlp";

/// Windows and Linux installs use the bundled yt-dlp `bin` folder; macOS uses yt-dlp from PATH.
const SHIPS_BIN: bool = cfg!(any(windows, target_os = "linux"));

/// Updater settings stored in update.json.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateConfig {
    /// Whether jj-dlp checks GitHub for new releases on its own.
    pub enabled: bool,
    /// Whether release candidates count as updates.
    pub include_rc: bool,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self { enabled: true, include_rc: false }
    }
}

fn config_path() -> PathBuf {
    crate::settings::config_dir().join("update.json")
}

/// Reads update.json, defaulting when the file is missing or invalid.
pub fn load_config() -> UpdateConfig {
    std::fs::read_to_string(config_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Writes update.json.
pub fn save_config(cfg: &UpdateConfig) -> std::io::Result<()> {
    std::fs::create_dir_all(crate::settings::config_dir())?;
    let json = serde_json::to_string_pretty(cfg).map_err(std::io::Error::other)?;
    std::fs::write(config_path(), json)
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

/// A newer release that has a build for this platform.
#[derive(Debug, Clone)]
pub struct Available {
    pub version: String,
    pub is_rc: bool,
    archive_name: String,
    archive_url: String,
    checksum_url: String,
}

/// A downloaded, verified and unpacked update waiting to be installed.
#[derive(Debug, Clone)]
pub struct Staged {
    pub version: String,
    root: PathBuf,
}

static PENDING: Mutex<Option<Staged>> = Mutex::new(None);

fn work_dir() -> PathBuf {
    crate::settings::config_dir().join("update")
}

fn error_file() -> PathBuf {
    crate::settings::config_dir().join("update-error.txt")
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .build()
}

fn msg<E: std::fmt::Display>(what: &'static str) -> impl FnOnce(E) -> String {
    move |e| format!("{what}: {e}")
}

fn parse_version(tag: &str) -> Result<semver::Version, String> {
    semver::Version::parse(tag.trim().trim_start_matches('v')).map_err(msg("bad release tag"))
}

/// Returns the newest release above the running version that has a build for this platform.
pub fn check(include_rc: bool) -> Result<Option<Available>, String> {
    if TARGET.is_empty() {
        return Ok(None);
    }
    let releases: Vec<Release> = agent()
        .get(RELEASES_URL)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(msg("update check failed"))?
        .into_json()
        .map_err(msg("update check returned bad data"))?;
    let current = parse_version(CURRENT)?;
    let archive_name = format!("jj-dlp-{TARGET}{ARCHIVE_EXT}");
    let checksum_name = format!("{archive_name}.sha256");
    let mut best: Option<(semver::Version, Available)> = None;
    for r in releases {
        if r.draft {
            continue;
        }
        let Ok(v) = parse_version(&r.tag_name) else {
            continue;
        };
        let is_rc = r.prerelease || !v.pre.is_empty();
        if (is_rc && !include_rc) || v <= current {
            continue;
        }
        let find = |name: &str| r.assets.iter().find(|a| a.name == name).map(|a| a.browser_download_url.clone());
        let (Some(archive_url), Some(checksum_url)) = (find(&archive_name), find(&checksum_name)) else {
            continue;
        };
        if best.as_ref().is_none_or(|(b, _)| v > *b) {
            let found = Available {
                version: v.to_string(),
                is_rc,
                archive_name: archive_name.clone(),
                archive_url,
                checksum_url,
            };
            best = Some((v, found));
        }
    }
    Ok(best.map(|(_, a)| a))
}

/// Refuses unless the running exe is a packaged install: not in a cargo target folder, bin beside it.
fn check_installable(exe: &Path) -> Result<(), String> {
    let dir = exe.parent().ok_or("cannot locate the jj-dlp folder")?;
    if dir.parent().and_then(Path::file_name).is_some_and(|n| n == "target") {
        return Err("this is a development build; updates only install into a release archive copy".to_string());
    }
    if SHIPS_BIN && !dir.join("bin").is_dir() {
        return Err("no bin folder next to jj-dlp; run the copy extracted from a release archive to update".to_string());
    }
    Ok(())
}

/// Downloads the archive, verifies its SHA-256, and unpacks it into the update work folder.
pub fn download(a: &Available) -> Result<Staged, String> {
    let exe = std::env::current_exe().map_err(msg("cannot locate jj-dlp"))?;
    check_installable(&exe)?;

    let work = work_dir();
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).map_err(msg("cannot create update folder"))?;

    let sum_text = agent()
        .get(&a.checksum_url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(msg("checksum download failed"))?
        .into_string()
        .map_err(msg("checksum read failed"))?;
    let expected = sum_text.split_whitespace().next().unwrap_or("").to_lowercase();

    let archive = work.join(&a.archive_name);
    let actual = fetch_file(&a.archive_url, &archive)?;
    if expected.is_empty() || actual != expected {
        return Err("downloaded update failed its checksum check".to_string());
    }

    let unpacked = work.join("unpacked");
    extract(&archive, &unpacked)?;
    let found = find_file(&unpacked, EXE_NAME).ok_or_else(|| format!("{EXE_NAME} not found in the update"))?;
    let root = found.parent().map(Path::to_path_buf).unwrap_or(unpacked);
    Ok(Staged { version: a.version.clone(), root })
}

/// Streams a URL to `dest` and returns the lowercase hex SHA-256 of what was written.
fn fetch_file(url: &str, dest: &Path) -> Result<String, String> {
    let resp = agent().get(url).set("User-Agent", USER_AGENT).call().map_err(msg("download failed"))?;
    let mut reader = resp.into_reader();
    let mut file = std::fs::File::create(dest).map_err(msg("cannot create download file"))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(msg("download interrupted"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(msg("cannot write download file"))?;
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[cfg(windows)]
fn extract(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(msg("cannot open archive"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(msg("bad zip archive"))?;
    zip.extract(dest).map_err(msg("cannot unpack archive"))
}

#[cfg(not(windows))]
fn extract(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(msg("cannot open archive"))?;
    tar::Archive::new(flate2::read::GzDecoder::new(file)).unpack(dest).map_err(msg("cannot unpack archive"))
}

/// Depth-first search for a file with the given name.
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        }
    }
    None
}

/// Remembers a staged update for `finish_pending` to install once the engine has stopped.
pub fn set_pending(staged: Staged) {
    *PENDING.lock().unwrap() = Some(staged);
}

/// Installs the pending update, if any, and relaunches; call after all recorders have stopped.
pub fn finish_pending() {
    let Some(staged) = PENDING.lock().unwrap().take() else {
        return;
    };
    let Ok(exe) = std::env::current_exe() else {
        let _ = std::fs::write(error_file(), "update failed: cannot locate the running exe");
        return;
    };
    if let Err(e) = install_now(&staged, &exe) {
        let _ = std::fs::write(error_file(), format!("update to v{} failed: {e}", staged.version));
    }
    let _ = std::fs::remove_dir_all(work_dir());
    let _ = std::process::Command::new(&exe).args(std::env::args_os().skip(1)).spawn();
}

/// Returns and clears the error left by a failed install on the previous run.
pub fn take_last_error() -> Option<String> {
    let text = std::fs::read_to_string(error_file()).ok()?;
    let _ = std::fs::remove_file(error_file());
    Some(text)
}

/// Replaces the running exe, then copies the bundled `bin` folder next to it where one ships.
fn install_now(staged: &Staged, exe: &Path) -> Result<(), String> {
    check_installable(exe)?;
    retry(|| self_replace::self_replace(staged.root.join(EXE_NAME)))?;
    let src_bin = staged.root.join("bin");
    if SHIPS_BIN && src_bin.is_dir() {
        let dest = exe.parent().unwrap_or(Path::new(".")).join("bin");
        retry(|| copy_dir(&src_bin, &dest))?;
    }
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

/// Runs `f` up to five times, pausing between attempts (files may still be locked briefly).
fn retry<T, E: std::fmt::Display>(mut f: impl FnMut() -> Result<T, E>) -> Result<T, String> {
    let mut last = String::new();
    for _ in 0..5 {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => {
                last = e.to_string();
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    }
    Err(last)
}

/// Counts other running processes with this exe's file name.
fn other_instances() -> usize {
    let Some(name) = std::env::current_exe().ok().and_then(|p| p.file_name().map(|n| n.to_os_string())) else {
        return 0;
    };
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let me = std::process::id();
    sys.processes().values().filter(|p| p.pid().as_u32() != me && p.name() == name.as_os_str()).count()
}

/// The `--update` command: installs the latest release and returns the process exit code.
pub fn run_cli(include_rc: bool) -> i32 {
    if TARGET.is_empty() {
        eprintln!("no release builds exist for this platform");
        return 1;
    }
    let mut cfg = load_config();
    cfg.include_rc |= include_rc;
    let others = other_instances();
    if others > 0 {
        eprintln!("jj-dlp is already running ({others} process(es)); close it before updating.");
        return 1;
    }
    let found = match check(cfg.include_rc) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    let Some(a) = found else {
        println!("jj-dlp v{CURRENT} is up to date.");
        return 0;
    };
    println!("Updating v{CURRENT} -> v{}...", a.version);
    let staged = match download(&a) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("update failed: {e}");
            return 1;
        }
    };
    let result = match std::env::current_exe() {
        Ok(exe) => install_now(&staged, &exe),
        Err(e) => Err(format!("cannot locate jj-dlp: {e}")),
    };
    let _ = std::fs::remove_dir_all(work_dir());
    match result {
        Ok(()) => {
            println!("Updated to v{}.", staged.version);
            0
        }
        Err(e) => {
            eprintln!("update failed: {e}");
            1
        }
    }
}
