//! Free space of the drives that hold output folders, and the notification line that reports it.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use sysinfo::Disks;

const GIB: u64 = 1024 * 1024 * 1024;
/// Free space below this many GB counts as low, unless changed.
pub const DEFAULT_LOW_GB: u64 = 20;
/// Free space below this percentage of the drive counts as low, unless changed.
pub const DEFAULT_LOW_PERCENT: u64 = 10;

/// The user's disk-space display settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Limits {
    pub show_percent: bool,
    pub low_gb: u64,
    pub low_percent: u64,
}

impl Limits {
    const DEFAULT: Limits = Limits { show_percent: false, low_gb: DEFAULT_LOW_GB, low_percent: DEFAULT_LOW_PERCENT };
}

impl Default for Limits {
    fn default() -> Self {
        Limits::DEFAULT
    }
}

static LIMITS: RwLock<Limits> = RwLock::new(Limits::DEFAULT);

/// The limits in use.
pub fn limits() -> Limits {
    *LIMITS.read().unwrap_or_else(|e| e.into_inner())
}

/// Makes `l` the limits in use, for the header and for notifications.
pub fn set_limits(l: Limits) {
    *LIMITS.write().unwrap_or_else(|e| e.into_inner()) = l;
}

/// One drive (mount point) and its space.
#[derive(Debug, Clone, PartialEq)]
pub struct DiskInfo {
    pub mount: PathBuf,
    pub total: u64,
    pub free: u64,
    /// Labels of the output folders on this drive.
    pub used_by: Vec<String>,
    /// True when the user added the drive.
    pub extra: bool,
    /// False when an added drive is not mounted right now.
    pub present: bool,
}

impl DiskInfo {
    pub fn label(&self) -> String {
        mount_label(&self.mount)
    }

    pub fn is_low(&self) -> bool {
        self.present && is_low(self.total, self.free)
    }

    pub fn free_percent(&self) -> f64 {
        free_percent(self.total, self.free)
    }
}

/// The tracked drives, and the other drives that could be added.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    pub disks: Vec<DiskInfo>,
    pub others: Vec<DiskInfo>,
}

struct Mount {
    path: PathBuf,
    total: u64,
    free: u64,
}

/// `free` as a percentage of `total`.
pub fn free_percent(total: u64, free: u64) -> f64 {
    if total == 0 { 0.0 } else { free as f64 * 100.0 / total as f64 }
}

/// `"139 GB"`, or `"139 GB 9%"` when the limits ask for the percentage.
pub fn free_text(total: u64, free: u64) -> String {
    let size = format_size(free);
    if limits().show_percent { format!("{size} {:.0}%", free_percent(total, free)) } else { size }
}

/// True when `free` of `total` bytes is under the low-space limits `l`.
fn is_low_with(l: Limits, total: u64, free: u64) -> bool {
    total > 0 && (free < l.low_gb.saturating_mul(GIB) || free.saturating_mul(100) < total.saturating_mul(l.low_percent))
}

/// True when `free` of `total` bytes is under the low-space limits in use.
pub fn is_low(total: u64, free: u64) -> bool {
    is_low_with(limits(), total, free)
}

/// A drive's short name: `C:` on Windows, the mount path elsewhere.
pub fn mount_label(mount: &Path) -> String {
    let s = mount.to_string_lossy();
    let t = s.trim_end_matches(['/', '\\']);
    if t.is_empty() { "/".to_string() } else { t.to_string() }
}

/// A byte count as MB, GB or TB.
pub fn format_size(bytes: u64) -> String {
    let b = bytes as f64;
    let (mb, gb, tb) = (1024.0 * 1024.0, 1024.0 * 1024.0 * 1024.0, 1024.0 * 1024.0 * 1024.0 * 1024.0);
    if b >= tb {
        format!("{:.2} TB", b / tb)
    } else if b >= 100.0 * gb {
        format!("{:.0} GB", b / gb)
    } else if b >= gb {
        format!("{:.1} GB", b / gb)
    } else {
        format!("{:.0} MB", b / mb)
    }
}

fn skipped_fs(fs: &str) -> bool {
    matches!(
        fs,
        "tmpfs" | "devtmpfs" | "squashfs" | "overlay" | "proc" | "sysfs" | "cgroup" | "cgroup2" | "ramfs" | "efivarfs" | "devpts"
            | "autofs" | "fusectl" | "debugfs" | "tracefs" | "securityfs" | "configfs" | "pstore" | "bpf" | "mqueue" | "hugetlbfs"
            | "binfmt_misc" | "nsfs" | "fuse.portal" | "fuse.gvfsd-fuse" | "iso9660" | "udf"
    )
}

fn skipped_path(path: &Path) -> bool {
    ["/proc", "/sys", "/dev", "/run/user", "/run/snapd", "/snap", "/System/Volumes", "/private/var/vm"]
        .iter()
        .any(|p| path.starts_with(p))
}

/// The real drives the system reports, without duplicates.
fn mounts() -> Vec<Mount> {
    let mut seen = HashSet::new();
    Disks::new_with_refreshed_list()
        .iter()
        .filter_map(|d| {
            let (path, total) = (d.mount_point(), d.total_space());
            let fs = d.file_system().to_string_lossy();
            let keep = total > 0 && !skipped_fs(&fs) && !skipped_path(path) && seen.insert(path.to_path_buf());
            keep.then(|| Mount { path: path.to_path_buf(), total, free: d.available_space() })
        })
        .collect()
}

/// `path` made absolute and resolved through its nearest existing parent.
fn normalize(path: &Path) -> PathBuf {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut probe = abs.as_path();
    loop {
        if let Ok(real) = probe.canonicalize() {
            return strip_verbatim(real);
        }
        match probe.parent() {
            Some(p) => probe = p,
            None => return abs,
        }
    }
}

/// Drops the `\\?\` prefix Windows adds to canonical paths.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = s.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path
    }
}

/// The deepest mount that contains `folder`.
fn mount_for<'a>(mounts: &'a [Mount], folder: &Path) -> Option<&'a Mount> {
    mounts.iter().filter(|m| folder.starts_with(&m.path)).max_by_key(|m| m.path.components().count())
}

fn info(m: &Mount, used_by: Vec<String>, extra: bool) -> DiskInfo {
    DiskInfo { mount: m.path.clone(), total: m.total, free: m.free, used_by, extra, present: true }
}

/// Builds the snapshot from already-resolved folders and the user's added drives.
fn build(mounts: &[Mount], folders: &[(String, PathBuf)], extras: &[PathBuf]) -> Snapshot {
    let mut disks: Vec<DiskInfo> = Vec::new();
    for (label, folder) in folders {
        let Some(m) = mount_for(mounts, folder) else { continue };
        match disks.iter_mut().find(|d| d.mount == m.path) {
            Some(d) => {
                if !d.used_by.contains(label) {
                    d.used_by.push(label.clone());
                }
            }
            None => disks.push(info(m, vec![label.clone()], false)),
        }
    }
    for extra in extras {
        match disks.iter_mut().find(|d| d.mount == *extra) {
            Some(d) => d.extra = true,
            None => disks.push(match mounts.iter().find(|m| m.path == *extra) {
                Some(m) => info(m, Vec::new(), true),
                None => DiskInfo { mount: extra.clone(), total: 0, free: 0, used_by: Vec::new(), extra: true, present: false },
            }),
        }
    }
    let mut others: Vec<DiskInfo> =
        mounts.iter().filter(|m| !disks.iter().any(|d| d.mount == m.path)).map(|m| info(m, Vec::new(), false)).collect();
    others.sort_by(|a, b| a.mount.cmp(&b.mount));
    Snapshot { disks, others }
}

/// The drives holding the labelled output folders, plus the added drives; blocks, so call it off the UI thread.
pub fn snapshot(folders: &[(String, PathBuf)], extras: &[PathBuf]) -> Snapshot {
    let resolved: Vec<(String, PathBuf)> = folders.iter().map(|(l, f)| (l.clone(), normalize(f))).collect();
    build(&mounts(), &resolved, extras)
}

/// The notification line for the drive holding `folder`; `None` when it cannot be found.
pub fn free_line(folder: &Path) -> Option<String> {
    let mounts = mounts();
    let m = mount_for(&mounts, &normalize(folder))?;
    let (free, label) = (free_text(m.total, m.free), mount_label(&m.path));
    Some(if is_low(m.total, m.free) {
        format!("\u{26A0}\u{FE0F} {free} free on {label}")
    } else {
        format!("\u{1F4BE} {free} free on {label}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mount(path: &str, total_gib: u64, free_gib: u64) -> Mount {
        Mount { path: PathBuf::from(path), total: total_gib * GIB, free: free_gib * GIB }
    }

    #[test]
    fn low_below_twenty_gib_or_ten_percent() {
        let d = Limits::default();
        assert!(is_low_with(d, 2000 * GIB, 19 * GIB));
        assert!(!is_low_with(d, 2000 * GIB, 221 * GIB));
        assert!(is_low_with(d, 2000 * GIB, 150 * GIB));
        assert!(!is_low_with(d, 500 * GIB, 60 * GIB));
        assert!(!is_low_with(d, 0, 0));
        assert!(is_low_with(Limits { low_gb: 100, ..d }, 500 * GIB, 60 * GIB));
    }

    #[test]
    fn deepest_mount_wins() {
        let mounts = vec![mount("/", 100, 50), mount("/mnt/rec", 100, 50)];
        assert_eq!(mount_for(&mounts, Path::new("/mnt/rec/twitch")).unwrap().path, PathBuf::from("/mnt/rec"));
        assert_eq!(mount_for(&mounts, Path::new("/home/me")).unwrap().path, PathBuf::from("/"));
    }

    #[test]
    fn build_groups_folders_and_adds_extras() {
        let mounts = vec![mount("/", 100, 50), mount("/mnt/a", 100, 50), mount("/mnt/b", 100, 50)];
        let folders = vec![
            ("Global".to_string(), PathBuf::from("/home/me/rec")),
            ("Posts: X".to_string(), PathBuf::from("/home/me/rec/posts")),
            ("Twitch".to_string(), PathBuf::from("/mnt/a/rec")),
        ];
        let extras = vec![PathBuf::from("/mnt/b"), PathBuf::from("/mnt/gone"), PathBuf::from("/mnt/a")];
        let snap = build(&mounts, &folders, &extras);
        let mounts_of: Vec<_> = snap.disks.iter().map(|d| d.mount.to_str().unwrap()).collect();
        assert_eq!(mounts_of, ["/", "/mnt/a", "/mnt/b", "/mnt/gone"]);
        assert_eq!(snap.disks[0].used_by, ["Global", "Posts: X"]);
        assert!(snap.disks[1].extra && !snap.disks[1].used_by.is_empty());
        assert!(snap.disks[2].present && !snap.disks[3].present);
        assert!(snap.others.is_empty());
    }

    #[test]
    fn untracked_mounts_are_offered() {
        let mounts = vec![mount("/", 100, 50), mount("/mnt/b", 100, 50)];
        let snap = build(&mounts, &[("Global".to_string(), PathBuf::from("/x"))], &[]);
        assert_eq!(snap.others.len(), 1);
        assert_eq!(snap.others[0].mount, PathBuf::from("/mnt/b"));
    }

    #[test]
    fn labels_and_sizes() {
        assert_eq!(mount_label(Path::new("/")), "/");
        assert_eq!(mount_label(Path::new("/mnt/rec/")), "/mnt/rec");
        assert_eq!(mount_label(Path::new(r"C:\")), "C:");
        assert_eq!(format_size(512 * 1024 * 1024), "512 MB");
        assert_eq!(format_size(12 * GIB + GIB / 2), "12.5 GB");
        assert_eq!(format_size(412 * GIB), "412 GB");
        assert_eq!(format_size(3 * 1024 * GIB), "3.00 TB");
    }

    #[test]
    fn verbatim_prefix_is_stripped() {
        assert_eq!(strip_verbatim(PathBuf::from(r"\\?\C:\rec")), PathBuf::from(r"C:\rec"));
        assert_eq!(strip_verbatim(PathBuf::from(r"\\?\UNC\srv\share")), PathBuf::from(r"\\srv\share"));
        assert_eq!(strip_verbatim(PathBuf::from("/mnt/a")), PathBuf::from("/mnt/a"));
    }
}
