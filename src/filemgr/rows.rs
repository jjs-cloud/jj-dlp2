//! Sorting and the folder/file row layout of the Files tab.

use super::scan::{FileRec, Root};
use super::{file_name, natural_cmp};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Status,
    Modified,
    Size,
    Rate,
}

impl SortKey {
    pub fn name(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Status => "status",
            SortKey::Modified => "modified",
            SortKey::Size => "size",
            SortKey::Rate => "rate",
        }
    }

    pub fn from_name(s: &str) -> SortKey {
        match s {
            "status" => SortKey::Status,
            "modified" => SortKey::Modified,
            "size" => SortKey::Size,
            "rate" => SortKey::Rate,
            _ => SortKey::Name,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sort {
    pub key: SortKey,
    pub asc: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Sort { key: SortKey::Name, asc: true }
    }
}

impl Sort {
    /// The sort after a header click: the same column flips direction, a new one starts ascending.
    pub fn click(self, key: SortKey) -> Sort {
        if self.key == key { Sort { key, asc: !self.asc } } else { Sort { key, asc: true } }
    }
}

/// What is selected in the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sel {
    Folder(PathBuf),
    File(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Folder { path: PathBuf, name: String, count: usize, collapsed: bool, writing: bool, depth: usize },
    File { path: PathBuf, prefix: String, name: String, depth: usize },
    Empty,
}

impl Row {
    pub fn sel(&self) -> Option<Sel> {
        match self {
            Row::Folder { path, .. } => Some(Sel::Folder(path.clone())),
            Row::File { path, .. } => Some(Sel::File(path.clone())),
            Row::Empty => None,
        }
    }
}

type Recs = HashMap<PathBuf, FileRec>;

/// What ordering files needs: the records, the sort and the current time for WRITING status.
#[derive(Clone, Copy)]
struct Cx<'a> {
    recs: &'a Recs,
    sort: Sort,
    now: Instant,
}

/// Ascending order of two files for `key`; ties fall back to the natural name order.
fn base_cmp(key: SortKey, now: Instant, ap: &Path, a: &FileRec, bp: &Path, b: &FileRec) -> Ordering {
    let primary = match key {
        SortKey::Name => Ordering::Equal,
        SortKey::Status => a.writing(now).cmp(&b.writing(now)),
        SortKey::Modified => (a.mtime / 60).cmp(&(b.mtime / 60)),
        SortKey::Size => a.size.cmp(&b.size),
        SortKey::Rate => a.rate.partial_cmp(&b.rate).unwrap_or(Ordering::Equal),
    };
    primary.then_with(|| natural_cmp(&file_name(ap), &file_name(bp)))
}

fn cmp_paths(cx: Cx, a: &PathBuf, b: &PathBuf) -> Ordering {
    match (cx.recs.get(a), cx.recs.get(b)) {
        (Some(x), Some(y)) => {
            let o = base_cmp(cx.sort.key, cx.now, a, x, b, y);
            if cx.sort.asc { o } else { o.reverse() }
        }
        _ => Ordering::Equal,
    }
}

/// True when any of `files` is being written.
fn any_writing(cx: Cx, files: &[PathBuf]) -> bool {
    files.iter().any(|p| cx.recs.get(p).is_some_and(|r| r.writing(cx.now)))
}

/// Orders groups by their first file under the sort; groups without files go last.
fn order_groups(cx: Cx, groups: Vec<(PathBuf, Vec<PathBuf>)>) -> Vec<(PathBuf, Vec<PathBuf>)> {
    let mut keyed: Vec<(Option<PathBuf>, (PathBuf, Vec<PathBuf>))> =
        groups.into_iter().map(|g| (g.1.iter().min_by(|a, b| cmp_paths(cx, a, b)).cloned(), g)).collect();
    keyed.sort_by(|a, b| match (&a.0, &b.0) {
        (Some(x), Some(y)) => cmp_paths(cx, x, y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });
    keyed.into_iter().map(|(_, g)| g).collect()
}

fn emit_folder(dir: &Path, depth: usize, files: &[PathBuf], cx: Cx, collapsed: &HashSet<PathBuf>, rows: &mut Vec<Row>) {
    let mut direct: Vec<&PathBuf> = files.iter().filter(|p| p.parent() == Some(dir)).collect();
    direct.sort_by(|a, b| cmp_paths(cx, a, b));
    for p in direct {
        rows.push(Row::File { path: p.clone(), prefix: String::new(), name: file_name(p).into_owned(), depth });
    }
    let mut children: Vec<PathBuf> = Vec::new();
    for p in files {
        let Ok(rel) = p.strip_prefix(dir) else { continue };
        let mut comps = rel.components();
        if let (Some(first), Some(_)) = (comps.next(), comps.next()) {
            let child = dir.join(first.as_os_str());
            if !children.contains(&child) {
                children.push(child);
            }
        }
    }
    let groups: Vec<(PathBuf, Vec<PathBuf>)> = children
        .into_iter()
        .map(|c| {
            let inside: Vec<PathBuf> = files.iter().filter(|p| p.starts_with(&c)).cloned().collect();
            (c, inside)
        })
        .collect();
    for (child, inside) in order_groups(cx, groups) {
        let is_collapsed = collapsed.contains(&child);
        let writing = is_collapsed && any_writing(cx, &inside);
        rows.push(Row::Folder { path: child.clone(), name: file_name(&child).into_owned(), count: inside.len(), collapsed: is_collapsed, writing, depth });
        if !is_collapsed {
            emit_folder(&child, depth + 1, &inside, cx, collapsed, rows);
        }
    }
}

/// Builds the visible rows. With several output folders each gets a collapsible header row.
pub fn build_rows(roots: &[Root], recs: &Recs, sort: Sort, group: bool, collapsed: &HashSet<PathBuf>, now: Instant) -> Vec<Row> {
    let cx = Cx { recs, sort, now };
    let multi = roots.len() > 1;
    let base = usize::from(multi);
    let groups: Vec<(PathBuf, Vec<PathBuf>)> = roots
        .iter()
        .map(|r| (r.dir.clone(), recs.iter().filter(|(_, rec)| rec.root == r.dir).map(|(p, _)| p.clone()).collect()))
        .collect();
    let mut rows = Vec::new();
    for (dir, files) in order_groups(cx, groups) {
        let label = roots.iter().find(|r| r.dir == dir).map(|r| r.label.as_str()).unwrap_or("");
        let root_collapsed = multi && collapsed.contains(&dir);
        if multi {
            let name = format!("{label}  —  {}", dir.display());
            let writing = root_collapsed && any_writing(cx, &files);
            rows.push(Row::Folder { path: dir.clone(), name, count: files.len(), collapsed: root_collapsed, writing, depth: 0 });
        }
        if root_collapsed {
            continue;
        }
        if files.is_empty() {
            rows.push(Row::Empty);
        } else if group {
            emit_folder(&dir, base, &files, cx, collapsed, &mut rows);
        } else {
            let mut flat: Vec<&PathBuf> = files.iter().collect();
            flat.sort_by(|a, b| cmp_paths(cx, a, b));
            for p in flat {
                let (prefix, name) = flat_names(p, &dir);
                rows.push(Row::File { path: p.clone(), prefix, name, depth: base });
            }
        }
    }
    rows
}

/// Splits a file path into its subfolder prefix (`sub/dir/`) under `root` and its file name.
fn flat_names(path: &Path, root: &Path) -> (String, String) {
    let name = file_name(path).into_owned();
    let prefix = path
        .strip_prefix(root)
        .ok()
        .and_then(Path::parent)
        .map(|p| p.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/"))
        .filter(|p| !p.is_empty())
        .map(|p| format!("{p}/"))
        .unwrap_or_default();
    (prefix, name)
}

/// Every subfolder (below an output folder) that holds at least one file.
pub fn subfolders(recs: &Recs, roots: &[Root]) -> HashSet<PathBuf> {
    let mut out = HashSet::new();
    for (path, rec) in recs {
        if !roots.iter().any(|r| r.dir == rec.root) {
            continue;
        }
        let mut cur = path.parent();
        while let Some(d) = cur {
            if d == rec.root || !d.starts_with(&rec.root) {
                break;
            }
            out.insert(d.to_path_buf());
            cur = d.parent();
        }
    }
    out
}

/// Keeps the selection across a rebuild; a vanished file moves to its next (else previous) sibling in the same folder when grouped, else to the neighbouring row.
pub fn keep_selection(old_rows: &[Row], cur: Option<&Sel>, new_rows: &[Row], group: bool) -> Option<Sel> {
    let fresh: Vec<Sel> = new_rows.iter().filter_map(Row::sel).collect();
    if fresh.is_empty() {
        return None;
    }
    if let Some(c) = cur {
        if fresh.contains(c) {
            return Some(c.clone());
        }
        if let (true, Sel::File(path)) = (group, c) {
            if let Some(sib) = sibling_in_folder(old_rows, path, &fresh) {
                return Some(sib);
            }
        }
    }
    let old_pos = cur.and_then(|c| old_rows.iter().filter_map(Row::sel).position(|s| &s == c));
    Some(fresh[old_pos.map_or(0, |p| p.min(fresh.len() - 1))].clone())
}

/// The nearest surviving file after `path` (else before it) in the same folder of `old_rows`.
fn sibling_in_folder(old_rows: &[Row], path: &Path, fresh: &[Sel]) -> Option<Sel> {
    let i = old_rows.iter().position(|r| matches!(r, Row::File { path: p, .. } if p == path))?;
    let same = |r: &Row| match r {
        Row::File { path: p, .. } if p.parent() == path.parent() => Some(Sel::File(p.clone())),
        _ => None,
    };
    let after = old_rows[i + 1..].iter().map(same).take_while(Option::is_some).flatten();
    let before = old_rows[..i].iter().rev().map(same).take_while(Option::is_some).flatten();
    after.chain(before).find(|s| fresh.contains(s))
}

/// The selection `delta` selectable rows away from `cur`, clamped to the list.
pub fn step(rows: &[Row], cur: Option<&Sel>, delta: isize) -> Option<Sel> {
    let sels: Vec<Sel> = rows.iter().filter_map(Row::sel).collect();
    if sels.is_empty() {
        return None;
    }
    let last = sels.len() as isize - 1;
    let i = match cur.and_then(|c| sels.iter().position(|s| s == c)) {
        None => if delta >= 0 { 0 } else { last },
        Some(p) => (p as isize + delta).clamp(0, last),
    };
    Some(sels[i as usize].clone())
}

/// The first or last selectable row.
pub fn edge(rows: &[Row], last: bool) -> Option<Sel> {
    let mut sels = rows.iter().filter_map(Row::sel);
    if last { sels.last() } else { sels.next() }
}

/// Index of the selection within `rows`.
pub fn row_index(rows: &[Row], sel: &Sel) -> Option<usize> {
    rows.iter().position(|r| r.sel().as_ref() == Some(sel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn rec(root: &str, size: u64, mtime: u64) -> FileRec {
        let now = Instant::now();
        FileRec { size, mtime, rate: 0.0, last_change: now, last_poll: now, root: PathBuf::from(root) }
    }

    fn build(roots: &[Root], recs: &Recs, sort: Sort, group: bool, collapsed: &HashSet<PathBuf>) -> Vec<Row> {
        build_rows(roots, recs, sort, group, collapsed, Instant::now())
    }

    fn roots(dirs: &[&str]) -> Vec<Root> {
        dirs.iter().map(|d| Root { label: d.to_string(), dir: PathBuf::from(d) }).collect()
    }

    fn names(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                Row::Folder { name, .. } => format!("[{name}]"),
                Row::File { prefix, name, .. } => format!("{prefix}{name}"),
                Row::Empty => "-".to_string(),
            })
            .collect()
    }

    fn sample() -> Recs {
        let mut m = HashMap::new();
        m.insert(PathBuf::from("/r/a10.ts"), rec("/r", 10, 100));
        m.insert(PathBuf::from("/r/a2.ts"), rec("/r", 30, 200));
        m.insert(PathBuf::from("/r/s/x.ts"), rec("/r", 20, 300));
        m
    }

    #[test]
    fn flat_sorts_by_name_and_size() {
        let rs = roots(&["/r"]);
        let rows = build(&rs, &sample(), Sort::default(), false, &HashSet::new());
        assert_eq!(names(&rows), ["a2.ts", "a10.ts", "s/x.ts"].map(String::from));
        let by_size = Sort { key: SortKey::Size, asc: false };
        let rows = build(&rs, &sample(), by_size, false, &HashSet::new());
        assert_eq!(names(&rows), ["a2.ts", "s/x.ts", "a10.ts"].map(String::from));
    }

    #[test]
    fn grouping_and_collapse() {
        let rs = roots(&["/r"]);
        let rows = build(&rs, &sample(), Sort::default(), true, &HashSet::new());
        assert_eq!(names(&rows), ["a2.ts", "a10.ts", "[s]", "x.ts"].map(String::from));
        let mut collapsed = HashSet::new();
        collapsed.insert(PathBuf::from("/r/s"));
        let rows = build(&rs, &sample(), Sort::default(), true, &collapsed);
        assert_eq!(names(&rows), ["a2.ts", "a10.ts", "[s]"].map(String::from));
        assert_eq!(subfolders(&sample(), &rs), HashSet::from([PathBuf::from("/r/s")]));
    }

    #[test]
    fn several_roots_get_headers_and_empty_markers() {
        let rs = roots(&["/r", "/e"]);
        let rows = build(&rs, &sample(), Sort::default(), false, &HashSet::new());
        let n = names(&rows);
        assert_eq!(n.len(), 6);
        assert!(n[0].starts_with("[/r"));
        assert!(n[4].starts_with("[/e"));
        assert_eq!(n[5], "-");
        let mut collapsed = HashSet::new();
        collapsed.insert(PathBuf::from("/r"));
        let rows = build(&rs, &sample(), Sort::default(), false, &collapsed);
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn selection_survives_and_moves_to_neighbour() {
        let rs = roots(&["/r"]);
        let old = build(&rs, &sample(), Sort::default(), false, &HashSet::new());
        let cur = Sel::File(PathBuf::from("/r/a10.ts"));
        let same = keep_selection(&old, Some(&cur), &old, false);
        assert_eq!(same, Some(cur.clone()));
        let mut recs = sample();
        recs.remove(Path::new("/r/a10.ts"));
        let new = build(&rs, &recs, Sort::default(), false, &HashSet::new());
        assert_eq!(keep_selection(&old, Some(&cur), &new, false), Some(Sel::File(PathBuf::from("/r/s/x.ts"))));
        assert_eq!(keep_selection(&old, None, &new, false), Some(Sel::File(PathBuf::from("/r/a2.ts"))));
        assert_eq!(keep_selection(&old, Some(&cur), &[], false), None);
    }

    #[test]
    fn grouped_delete_stays_in_folder() {
        let rs = roots(&["/r"]);
        let mut recs = HashMap::new();
        for n in ["1", "2", "3", "4"] {
            recs.insert(PathBuf::from(format!("/r/d/{n}.mp4")), rec("/r", 1, 0));
        }
        recs.insert(PathBuf::from("/r/z/9.mp4"), rec("/r", 1, 0));
        let old = build(&rs, &recs, Sort::default(), true, &HashSet::new());
        let pick = |gone: &str| {
            let cur = Sel::File(PathBuf::from(format!("/r/d/{gone}.mp4")));
            let mut left = recs.clone();
            left.remove(Path::new(&format!("/r/d/{gone}.mp4")));
            let new = build(&rs, &left, Sort::default(), true, &HashSet::new());
            keep_selection(&old, Some(&cur), &new, true)
        };
        assert_eq!(pick("1"), Some(Sel::File(PathBuf::from("/r/d/2.mp4"))));
        assert_eq!(pick("3"), Some(Sel::File(PathBuf::from("/r/d/4.mp4"))));
        assert_eq!(pick("4"), Some(Sel::File(PathBuf::from("/r/d/3.mp4"))));
    }

    #[test]
    fn collapsed_folder_shows_writing() {
        let rs = roots(&["/r"]);
        let mut collapsed = HashSet::new();
        collapsed.insert(PathBuf::from("/r/s"));
        let rows = build(&rs, &sample(), Sort::default(), true, &collapsed);
        assert!(matches!(rows.last(), Some(Row::Folder { writing: true, .. })));
        let rows = build(&rs, &sample(), Sort::default(), true, &HashSet::new());
        assert!(rows.iter().all(|r| !matches!(r, Row::Folder { writing: true, .. })));
    }

    #[test]
    fn stepping_clamps() {
        let rs = roots(&["/r"]);
        let rows = build(&rs, &sample(), Sort::default(), false, &HashSet::new());
        let first = edge(&rows, false).unwrap();
        assert_eq!(first, Sel::File(PathBuf::from("/r/a2.ts")));
        assert_eq!(step(&rows, Some(&first), -1), Some(first.clone()));
        assert_eq!(step(&rows, Some(&first), 50), edge(&rows, true));
        assert_eq!(row_index(&rows, &first), Some(0));
    }

    #[test]
    fn status_sort_puts_writing_files_apart() {
        let now = Instant::now();
        let mut m = HashMap::new();
        let mut idle = rec("/r", 1, 0);
        idle.last_change = now.checked_sub(Duration::from_secs(30)).unwrap_or(now);
        m.insert(PathBuf::from("/r/a.ts"), idle);
        m.insert(PathBuf::from("/r/b.ts"), rec("/r", 1, 0));
        let rs = roots(&["/r"]);
        let asc = build_rows(&rs, &m, Sort { key: SortKey::Status, asc: true }, false, &HashSet::new(), now);
        assert_eq!(names(&asc), ["a.ts", "b.ts"].map(String::from));
        let desc = build_rows(&rs, &m, Sort { key: SortKey::Status, asc: false }, false, &HashSet::new(), now);
        assert_eq!(names(&desc), ["b.ts", "a.ts"].map(String::from));
    }

    #[test]
    fn header_click_flips_or_switches() {
        let s = Sort::default().click(SortKey::Name);
        assert_eq!(s, Sort { key: SortKey::Name, asc: false });
        assert_eq!(s.click(SortKey::Size), Sort { key: SortKey::Size, asc: true });
    }
}
