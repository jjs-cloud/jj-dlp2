//! Checks GitHub Releases for a newer build and installs it over the running exe.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

pub const CURRENT: &str = env!("CARGO_PKG_VERSION");
const LATEST_URL: &str = "https://api.github.com/repos/jjs-cloud/jj-dlp2/releases/latest";
const USER_AGENT: &str = concat!("jj-dlp/", env!("CARGO_PKG_VERSION"));
/// How many directories above the exe are searched for the installed `bin` folder.
const BIN_PARENTS: usize = 3;

#[cfg(all(windows, target_arch = "x86_64"))]
const TARGET: &str = "x86_64-pc-windows-msvc";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const TARGET: &str = "x86_64-unknown-linux-gnu";
#[cfg(not(any(all(windows, target_arch = "x86_64"), all(target_os = "linux", target_arch = "x86_64"))))]
const TARGET: &str = "";

#[cfg(windows)]
const ARCHIVE_EXT: &str = ".zip";
#[cfg(not(windows))]
const ARCHIVE_EXT: &str = ".tar.gz";

#[cfg(windows)]
const EXE_NAME: &str = "jj-dlp.exe";
#[cfg(not(windows))]
const EXE_NAME: &str = "jj-dlp";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
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

/// Looks up the latest release; returns it only if it is newer and has a build for this platform.
pub fn check() -> Result<Option<Available>, String> {
    if TARGET.is_empty() {
        return Ok(None);
    }
    let release: Release = agent()
        .get(LATEST_URL)
        .set("User-Agent", USER_AGENT)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(msg("update check failed"))?
        .into_json()
        .map_err(msg("update check returned bad data"))?;
    let latest = parse_version(&release.tag_name)?;
    let current = parse_version(CURRENT)?;
    if latest <= current {
        return Ok(None);
    }
    let archive_name = format!("jj-dlp-{TARGET}{ARCHIVE_EXT}");
    let find = |name: &str| release.assets.iter().find(|a| a.name == name).map(|a| a.browser_download_url.clone());
    let (Some(archive_url), Some(checksum_url)) = (find(&archive_name), find(&format!("{archive_name}.sha256"))) else {
        return Err(format!("release {} has no {archive_name} with a .sha256 file", release.tag_name));
    };
    Ok(Some(Available { version: latest.to_string(), archive_name, archive_url, checksum_url }))
}

/// Async wrapper so the GUI can run `check` off the UI thread.
pub async fn check_async() -> Result<Option<Available>, String> {
    tokio::task::spawn_blocking(check).await.map_err(|e| e.to_string())?
}

/// Async wrapper so the GUI can run `download` off the UI thread.
pub async fn download_async(a: Available) -> Result<Staged, String> {
    tokio::task::spawn_blocking(move || download(&a)).await.map_err(|e| e.to_string())?
}

/// Downloads the archive, verifies its SHA-256, and unpacks it into the update work folder.
pub fn download(a: &Available) -> Result<Staged, String> {
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
    let exe = find_file(&unpacked, EXE_NAME).ok_or_else(|| format!("{EXE_NAME} not found in the update"))?;
    let root = exe.parent().map(Path::to_path_buf).unwrap_or(unpacked);
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

/// Remembers a staged update to be installed by `finish_pending` once the engine has stopped.
pub fn set_pending(staged: Staged) {
    *PENDING.lock().unwrap() = Some(staged);
}

/// Installs the pending update, if any, and relaunches; call after all recorders have stopped.
pub fn finish_pending() {
    let Some(staged) = PENDING.lock().unwrap().take() else {
        return;
    };
    let Ok(exe) = std::env::current_exe() else {
        let _ = std::fs::write(error_file(), "update: cannot locate the running exe");
        return;
    };
    if let Err(e) = install(&staged, &exe) {
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

/// Replaces the running exe, then copies the bundled `bin` folder over the installed one.
fn install(staged: &Staged, exe: &Path) -> Result<(), String> {
    retry(|| self_replace::self_replace(staged.root.join(EXE_NAME)))?;
    let src_bin = staged.root.join("bin");
    if src_bin.is_dir() {
        let dest = installed_bin_dir(exe);
        retry(|| copy_dir(&src_bin, &dest))?;
    }
    Ok(())
}

/// The nearest `bin` folder at or above the exe's folder, else `bin` next to the exe.
fn installed_bin_dir(exe: &Path) -> PathBuf {
    let dir = exe.parent().unwrap_or(Path::new("."));
    dir.ancestors()
        .take(BIN_PARENTS + 1)
        .map(|d| d.join("bin"))
        .find(|b| b.is_dir())
        .unwrap_or_else(|| dir.join("bin"))
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
