//! Walks the output folders and tracks per-file size changes.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A file counts as WRITING until its size has stayed the same this long.
pub const IDLE_AFTER: Duration = Duration::from_secs(3);

/// One output folder with the label shown for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Root {
    pub label: String,
    pub dir: PathBuf,
}

/// One file found by a scan.
#[derive(Debug, Clone, PartialEq)]
pub struct Scanned {
    pub path: PathBuf,
    pub size: u64,
    pub mtime: u64,
    pub root: PathBuf,
}

/// Everything one scan found.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ScanResult {
    pub files: Vec<Scanned>,
    /// Current size of the in-progress move file, if any.
    pub moving: Option<u64>,
}

/// A file's last seen state and write rate.
#[derive(Debug, Clone, PartialEq)]
pub struct FileRec {
    pub size: u64,
    pub mtime: u64,
    pub rate: f64,
    pub last_change: Instant,
    pub last_poll: Instant,
    pub root: PathBuf,
}

impl FileRec {
    /// True while the file's size changed within `IDLE_AFTER`.
    pub fn writing(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last_change) < IDLE_AFTER
    }
}

fn unix_secs(t: Option<SystemTime>) -> u64 {
    t.and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs())
}

fn walk(dir: &Path, root: &Path, skip: Option<&Path>, out: &mut Vec<Scanned>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            walk(&path, root, skip, out);
            continue;
        }
        if skip.is_some_and(|s| s == path) {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&path) else { continue };
        if meta.is_file() {
            out.push(Scanned { path, size: meta.len(), mtime: unix_secs(meta.modified().ok()), root: root.to_path_buf() });
        }
    }
}

/// Lists every file under the roots, leaving out the in-progress move file.
pub fn scan(roots: &[Root], moving: Option<&Path>) -> ScanResult {
    let mut files = Vec::new();
    for r in roots {
        walk(&r.dir, &r.dir, moving, &mut files);
    }
    let moving = moving.and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len());
    ScanResult { files, moving }
}

/// Folds a scan into the tracked records, updating write rates and dropping vanished files.
pub fn merge(records: &mut HashMap<PathBuf, FileRec>, scanned: Vec<Scanned>, now: Instant) {
    {
        let live: HashSet<&Path> = scanned.iter().map(|s| s.path.as_path()).collect();
        records.retain(|p, _| live.contains(p.as_path()));
    }
    for s in scanned {
        if let Some(r) = records.get_mut(&s.path) {
            if s.size != r.size {
                let dt = now.saturating_duration_since(r.last_poll).as_secs_f64().max(0.001);
                r.rate = (s.size as f64 - r.size as f64) / dt;
                r.size = s.size;
                r.last_change = now;
            }
            r.last_poll = now;
            r.mtime = s.mtime;
            r.root = s.root;
            continue;
        }
        let rec = FileRec { size: s.size, mtime: s.mtime, rate: 0.0, last_change: now, last_poll: now, root: s.root };
        records.insert(s.path, rec);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sc(path: &str, size: u64) -> Scanned {
        Scanned { path: PathBuf::from(path), size, mtime: 0, root: PathBuf::from("/r") }
    }

    #[test]
    fn merge_tracks_growth_and_removal() {
        let t0 = Instant::now();
        let mut recs = HashMap::new();
        merge(&mut recs, vec![sc("/r/a", 100), sc("/r/b", 5)], t0);
        assert_eq!(recs[Path::new("/r/a")].rate, 0.0);
        let t1 = t0 + Duration::from_secs(2);
        merge(&mut recs, vec![sc("/r/a", 300)], t1);
        assert!(!recs.contains_key(Path::new("/r/b")));
        let a = &recs[Path::new("/r/a")];
        assert!((a.rate - 100.0).abs() < 1e-6);
        assert!(a.writing(t1));
        assert!(!a.writing(t1 + IDLE_AFTER));
    }

    #[test]
    fn scan_lists_files_and_skips_moving() {
        let dir = std::env::temp_dir().join(format!("jjdlp_scan_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.ts"), b"12345").unwrap();
        std::fs::write(dir.join("sub").join("b.ts"), b"1").unwrap();
        let tmp = dir.join("m.moving.ts");
        std::fs::write(&tmp, b"123").unwrap();
        let roots = [Root { label: "x".into(), dir: dir.clone() }];
        let res = scan(&roots, Some(&tmp));
        assert_eq!(res.files.len(), 2);
        assert_eq!(res.moving, Some(3));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
