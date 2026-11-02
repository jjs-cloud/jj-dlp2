//! Sorting and the folder/file row layout of the Files tab.

use super::scan::{FileRec, Root};
use super::{file_name, natural_cmp};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Modified,
    Size,
    Rate,
}

impl SortKey {
    pub fn name(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Modified => "modified",
            SortKey::Size => "size",
            SortKey::Rate => "rate",
        }
    }

    pub fn from_name(s: &str) -> SortKey {
        match s {
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
    Folder { path: PathBuf, name: String, count: usize, collapsed: bool, depth: usize },
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

/// Ascending order of two files for `key`; ties fall back to the natural name order.
fn base_cmp(key: SortKey, ap: &Path, a: &FileRec, bp: &Path, b: &FileRec) -> Ordering {
    let primary = match key {
        SortKey::Name => Ordering::Equal,
        SortKey::Modified => (a.mtime / 60).cmp(&(b.mtime / 60)),
        SortKey::Size => a.size.cmp(&b.size),
        SortKey::Rate => a.rate.partial_cmp(&b.rate).unwrap_or(Ordering::Equal),
    };
    primary.then_with(|| natural_cmp(&file_name(ap), &file_name(bp)))
}

fn cmp_paths(sort: Sort, recs: &Recs, a: &PathBuf, b: &PathBuf) -> Ordering {
    match (recs.get(a), recs.get(b)) {
        (Some(x), Some(y)) => {
            let o = base_cmp(sort.key, a, x, b, y);
            if sort.asc { o } else { o.reverse() }
        }
        _ => Ordering::Equal,
    }
}

/// Orders groups by their first file under the sort; groups without files go last.
fn order_groups(sort: Sort, recs: &Recs, groups: Vec<(PathBuf, Vec<PathBuf>)>) -> Vec<(PathBuf, Vec<PathBuf>)> {
    let mut keyed: Vec<(Option<PathBuf>, (PathBuf, Vec<PathBuf>))> = groups
        .into_iter()
        .map(|g| (g.1.iter().min_by(|a, b| cmp_paths(sort, recs, a, b)).cloned(), g))
        .collect();
    keyed.sort_by(|a, b| match (&a.0, &b.0) {
        (Some(x), Some(y)) => cmp_paths(sort, recs, x, y),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    });
    keyed.into_iter().map(|(_, g)| g).collect()
}

fn emit_folder(dir: &Path, depth: usize, files: &[PathBuf], recs: &Recs, sort: Sort, collapsed: &HashSet<PathBuf>, rows: &mut Vec<Row>) {
    let mut direct: Vec<&PathBuf> = files.iter().filter(|p| p.parent() == Some(dir)).collect();
    direct.sort_by(|a, b| cmp_paths(sort, recs, a, b));
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
    for (child, inside) in order_groups(sort, recs, groups) {
        let is_collapsed = collapsed.contains(&child);
        rows.push(Row::Folder { path: child.clone(), name: file_name(&child).into_owned(), count: inside.len(), collapsed: is_collapsed, depth });
        if !is_collapsed {
            emit_folder(&child, depth + 1, &inside, recs, sort, collapsed, rows);
        }
    }
}

/// Builds the visible rows. With several output folders each gets a collapsible header row.
pub fn build_rows(roots: &[Root], recs: &Recs, sort: Sort, group: bool, collapsed: &HashSet<PathBuf>) -> Vec<Row> {
    let multi = roots.len() > 1;
    let base = usize::from(multi);
    let groups: Vec<(PathBuf, Vec<PathBuf>)> = roots
        .iter()
        .map(|r| (r.dir.clone(), recs.iter().filter(|(_, rec)| rec.root == r.dir).map(|(p, _)| p.clone()).collect()))
        .collect();
    let mut rows = Vec::new();
    for (dir, files) in order_groups(sort, recs, groups) {
        let label = roots.iter().find(|r| r.dir == dir).map(|r| r.label.as_str()).unwrap_or("");
        let root_collapsed = multi && collapsed.contains(&dir);
        if multi {
            let name = format!("{label}  —  {}", dir.display());
            rows.push(Row::Folder { path: dir.clone(), name, count: files.len(), collapsed: root_collapsed, depth: 0 });
        }
        if root_collapsed {
            continue;
        }
        if files.is_empty() {
            rows.push(Row::Empty);
        } else if group {
            emit_folder(&dir, base, &files, recs, sort, collapsed, &mut rows);
        } else {
            let mut flat: Vec<&PathBuf> = files.iter().collect();
            flat.sort_by(|a, b| cmp_paths(sort, recs, a, b));
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

/// Keeps the selection across a rebuild, moving to the neighbouring row when it disappeared.
pub fn keep_selection(old_rows: &[Row], cur: Option<&Sel>, new_rows: &[Row]) -> Option<Sel> {
    let fresh: Vec<Sel> = new_rows.iter().filter_map(Row::sel).collect();
    if fresh.is_empty() {
        return None;
    }
    if let Some(c) = cur {
        if fresh.contains(c) {
            return Some(c.clone());
        }
    }
    let old_pos = cur.and_then(|c| old_rows.iter().filter_map(Row::sel).position(|s| &s == c));
    Some(fresh[old_pos.map_or(0, |p| p.min(fresh.len() - 1))].clone())
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
    use std::time::Instant;

    fn rec(root: &str, size: u64, mtime: u64) -> FileRec {
        let now = Instant::now();
        FileRec { size, mtime, rate: 0.0, last_change: now, last_poll: now, root: PathBuf::from(root) }
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
        let rows = build_rows(&rs, &sample(), Sort::default(), false, &HashSet::new());
        assert_eq!(names(&rows), ["a2.ts", "a10.ts", "s/x.ts"].map(String::from));
        let by_size = Sort { key: SortKey::Size, asc: false };
        let rows = build_rows(&rs, &sample(), by_size, false, &HashSet::new());
        assert_eq!(names(&rows), ["a2.ts", "s/x.ts", "a10.ts"].map(String::from));
    }

    #[test]
    fn grouping_and_collapse() {
        let rs = roots(&["/r"]);
        let rows = build_rows(&rs, &sample(), Sort::default(), true, &HashSet::new());
        assert_eq!(names(&rows), ["a2.ts", "a10.ts", "[s]", "x.ts"].map(String::from));
        let mut collapsed = HashSet::new();
        collapsed.insert(PathBuf::from("/r/s"));
        let rows = build_rows(&rs, &sample(), Sort::default(), true, &collapsed);
        assert_eq!(names(&rows), ["a2.ts", "a10.ts", "[s]"].map(String::from));
        assert_eq!(subfolders(&sample(), &rs), HashSet::from([PathBuf::from("/r/s")]));
    }

    #[test]
    fn several_roots_get_headers_and_empty_markers() {
        let rs = roots(&["/r", "/e"]);
        let rows = build_rows(&rs, &sample(), Sort::default(), false, &HashSet::new());
        let n = names(&rows);
        assert_eq!(n.len(), 6);
        assert!(n[0].starts_with("[/r"));
        assert!(n[4].starts_with("[/e"));
        assert_eq!(n[5], "-");
        let mut collapsed = HashSet::new();
        collapsed.insert(PathBuf::from("/r"));
        let rows = build_rows(&rs, &sample(), Sort::default(), false, &collapsed);
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn selection_survives_and_moves_to_neighbour() {
        let rs = roots(&["/r"]);
        let old = build_rows(&rs, &sample(), Sort::default(), false, &HashSet::new());
        let cur = Sel::File(PathBuf::from("/r/a10.ts"));
        let same = keep_selection(&old, Some(&cur), &old);
        assert_eq!(same, Some(cur.clone()));
        let mut recs = sample();
        recs.remove(Path::new("/r/a10.ts"));
        let new = build_rows(&rs, &recs, Sort::default(), false, &HashSet::new());
        assert_eq!(keep_selection(&old, Some(&cur), &new), Some(Sel::File(PathBuf::from("/r/s/x.ts"))));
        assert_eq!(keep_selection(&old, None, &new), Some(Sel::File(PathBuf::from("/r/a2.ts"))));
        assert_eq!(keep_selection(&old, Some(&cur), &[]), None);
    }

    #[test]
    fn stepping_clamps() {
        let rs = roots(&["/r"]);
        let rows = build_rows(&rs, &sample(), Sort::default(), false, &HashSet::new());
        let first = edge(&rows, false).unwrap();
        assert_eq!(first, Sel::File(PathBuf::from("/r/a2.ts")));
        assert_eq!(step(&rows, Some(&first), -1), Some(first.clone()));
        assert_eq!(step(&rows, Some(&first), 50), edge(&rows, true));
        assert_eq!(row_index(&rows, &first), Some(0));
    }

    #[test]
    fn header_click_flips_or_switches() {
        let s = Sort::default().click(SortKey::Name);
        assert_eq!(s, Sort { key: SortKey::Name, asc: false });
        assert_eq!(s.click(SortKey::Size), Sort { key: SortKey::Size, asc: true });
    }
}
