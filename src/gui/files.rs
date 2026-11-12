//! State, messages and logic of the Files tab.

use super::{App, Msg, Tab, prefs};
use crate::api::now_secs;
use crate::filemgr::jobs::{self, FixupSpec, MoveSpec, Outcome, Token, TrimSpec};
use crate::filemgr::ops::{self, DeleteMode};
use crate::filemgr::rows::{self, Row, Sel, Sort, SortKey};
use crate::filemgr::scan::{self, FileRec, Root, ScanResult};
use crate::filemgr::file_name;
use crate::policy::layout;
use crate::policy::posts::{Post, PostState};
use iced::widget::scrollable::{AbsoluteOffset, Viewport};
use iced::{Task, keyboard};
use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Height of one list row in pixels.
pub const ROW_H: f32 = 28.0;
const SCAN_EVERY: Duration = Duration::from_millis(900);
/// Scan interval while nothing is recording, being written or being moved.
const SCAN_IDLE_EVERY: Duration = Duration::from_secs(4);
const STATUS_SECS: u64 = 4;
const TRIM_IDS: [&str; 6] = ["files-trim-0", "files-trim-1", "files-trim-2", "files-trim-3", "files-trim-4", "files-trim-5"];

pub fn scroll_id() -> iced::widget::Id {
    iced::widget::Id::new("files-scroll")
}

/// Id of one Trim time field; `n` counts the six fields from the start hours.
pub fn trim_id(n: usize) -> iced::widget::Id {
    iced::widget::Id::new(TRIM_IDS[n.min(5)])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobKind {
    Fixup,
    Trim,
    Move,
}

impl JobKind {
    pub fn label(self) -> &'static str {
        match self {
            JobKind::Fixup => "Fixup",
            JobKind::Trim => "Trim",
            JobKind::Move => "Move",
        }
    }

    /// Tag shown on the file's row while the job runs.
    pub fn busy(self) -> &'static str {
        match self {
            JobKind::Fixup => "Fixing…",
            JobKind::Trim => "Trimming…",
            JobKind::Move => "Moving…",
        }
    }
}

pub struct RunningJob {
    pub id: u64,
    pub kind: JobKind,
    pub target: PathBuf,
    pub token: Arc<Token>,
}

/// The inline action panel under the header.
#[derive(Debug, Clone, PartialEq)]
pub enum Panel {
    Fixup { target: PathBuf, delete_original: bool, convert_mp4: bool },
    Trim { target: PathBuf, start: [String; 3], end: [String; 3], delete_original: bool, convert_mp4: bool },
    Move { target: PathBuf, fixup: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    Fixup,
    Trim,
    Move,
}

impl Panel {
    pub fn kind(&self) -> PanelKind {
        match self {
            Panel::Fixup { .. } => PanelKind::Fixup,
            Panel::Trim { .. } => PanelKind::Trim,
            Panel::Move { .. } => PanelKind::Move,
        }
    }
}

/// The file a move is currently writing at its destination.
pub struct Moving {
    pub name: String,
    pub tmp: PathBuf,
    pub size: u64,
    pub rate: f64,
    pub last_poll: Instant,
}

pub struct FilesState {
    /// How many files were being written when the rows were last built.
    built_writing: usize,
    /// The rows need rebuilding even if the next scan finds no changes.
    stale: bool,
    pub records: HashMap<PathBuf, FileRec>,
    pub roots: Vec<Root>,
    pub rows: Vec<Row>,
    pub sel: Option<Sel>,
    pub collapsed: HashSet<PathBuf>,
    pub sort: Sort,
    pub group: bool,
    pub delete_empty: bool,
    pub hide_posts: bool,
    pub move_dir: String,
    pub panel: Option<Panel>,
    pub jobs: Vec<RunningJob>,
    pub moving: Option<Moving>,
    status: Option<(String, u64)>,
    scanning: bool,
    last_scan: Option<Instant>,
    pub(super) scroll_y: f32,
    view_h: f32,
    next_job: u64,
}

impl FilesState {
    pub fn new() -> Self {
        let p = prefs::load_files();
        FilesState {
            records: HashMap::new(),
            roots: Vec::new(),
            rows: Vec::new(),
            sel: None,
            collapsed: HashSet::new(),
            sort: Sort { key: SortKey::from_name(&p.sort), asc: p.asc },
            group: p.group,
            delete_empty: p.delete_empty,
            hide_posts: p.hide_posts,
            move_dir: p.move_dir,
            panel: None,
            jobs: Vec::new(),
            moving: None,
            status: None,
            scanning: false,
            last_scan: None,
            built_writing: 0,
            stale: true,
            scroll_y: 0.0,
            view_h: 0.0,
            next_job: 1,
        }
    }

    /// Called when the tab is opened.
    pub fn on_enter(&mut self) {
        self.stale = true;
        self.scroll_y = 0.0;
        self.last_scan = None;
    }

    fn save_prefs(&self) {
        prefs::save_files(&prefs::FilesPrefs {
            sort: self.sort.key.name().to_string(),
            asc: self.sort.asc,
            group: self.group,
            delete_empty: self.delete_empty,
            hide_posts: self.hide_posts,
            move_dir: self.move_dir.clone(),
        });
    }

    fn rebuild(&mut self) {
        self.stale = false;
        let rows = rows::build_rows(&self.roots, &self.records, self.sort, self.group, &self.collapsed, Instant::now());
        self.sel = rows::keep_selection(&self.rows, self.sel.as_ref(), &rows, self.group);
        self.rows = rows;
    }

    fn say(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), now_secs()));
    }

    /// The status line text while it is still fresh.
    pub fn status_text(&self, now: u64) -> Option<&str> {
        self.status.as_ref().filter(|(_, at)| now.saturating_sub(*at) < STATUS_SECS).map(|(m, _)| m.as_str())
    }

    /// The running job working on `path`.
    pub fn job_for(&self, path: &Path) -> Option<&RunningJob> {
        self.jobs.iter().find(|j| j.target == path)
    }

    /// True when the selection is a file.
    pub fn file_selected(&self) -> bool {
        matches!(self.sel, Some(Sel::File(_)))
    }

    /// True when some subfolder is currently collapsed.
    pub fn any_collapsed(&self) -> bool {
        self.group && rows::subfolders(&self.records, &self.roots).iter().any(|d| self.collapsed.contains(d))
    }

    /// True when some subfolder is currently expanded.
    pub fn any_expanded(&self) -> bool {
        self.group && rows::subfolders(&self.records, &self.roots).iter().any(|d| !self.collapsed.contains(d))
    }
}

/// Files that belong to posts, so the Files tab can hide them.
struct PostFiles {
    done: HashSet<String>,
    downloading: Vec<String>,
}

/// A path as a comparison key: forward slashes, lower case on Windows.
fn path_key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    if cfg!(windows) { s.to_lowercase() } else { s }
}

impl PostFiles {
    fn new(posts: &[Post]) -> Self {
        PostFiles {
            done: posts.iter().filter_map(|p| p.file.as_deref()).map(|f| path_key(Path::new(f))).collect(),
            downloading: posts.iter().filter(|p| p.state == PostState::Downloading && !p.id.is_empty()).map(|p| p.id.clone()).collect(),
        }
    }

    /// True for a finished post file, or a file of a post that is still downloading.
    fn hides(&self, path: &Path) -> bool {
        if self.done.contains(&path_key(path)) {
            return true;
        }
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.downloading.iter().any(|id| name.contains(id.as_str()))
    }
}

#[derive(Debug, Clone)]
pub enum FilesMsg {
    Scanned(ScanResult),
    Key(keyboard::Event),
    Scrolled(Viewport),
    Select(Sel),
    Activate(Sel),
    HeaderSort(SortKey),
    ToggleGroup(bool),
    ToggleDeleteEmpty(bool),
    ToggleHidePosts(bool),
    ExpandAll,
    CollapseAll,
    OpenSelected,
    RevealSelected,
    DeleteSelected,
    OpenPanel(PanelKind),
    ClosePanel,
    FixupDelete(bool),
    FixupMp4(bool),
    TrimField(bool, usize, String),
    TrimDelete(bool),
    TrimMp4(bool),
    MoveFixup(bool),
    RunFixup,
    RunTrim,
    ChooseMoveDest,
    MoveDestChosen(Option<PathBuf>),
    CancelJob(u64),
    JobDone(u64, Outcome),
}

/// Subscription mapper for key presses on the Files tab.
pub fn key_msg(e: keyboard::Event) -> Msg {
    Msg::Files(FilesMsg::Key(e))
}

impl App {
    /// The distinct output folders of all sites, labelled with the first site using each.
    fn files_roots(&self) -> Vec<Root> {
        let mut roots: Vec<Root> = Vec::new();
        for site in &self.settings.sites {
            let dir = layout::base_dir(&site.output_dir, &self.settings.default_output_dir);
            if dir.is_empty() {
                continue;
            }
            let p = PathBuf::from(dir);
            let p = std::path::absolute(&p).unwrap_or(p);
            if !roots.iter().any(|r| r.dir == p) {
                roots.push(Root { label: site.name.clone(), dir: p });
            }
        }
        roots
    }

    /// Starts a rescan once a second while the Files tab is shown.
    pub(super) fn files_tick(&mut self) -> Task<Msg> {
        let now = Instant::now();
        let f = &self.files;
        let busy = !self.processes.is_empty() || f.built_writing > 0 || !f.jobs.is_empty() || f.moving.is_some();
        let every = if busy { SCAN_EVERY } else { SCAN_IDLE_EVERY };
        if f.scanning || f.last_scan.is_some_and(|t| now.duration_since(t) < every) {
            return Task::none();
        }
        let roots = self.files_roots();
        let moving = self.files.moving.as_ref().map(|m| m.tmp.clone());
        self.files.scanning = true;
        self.files.last_scan = Some(now);
        self.files.stale |= self.files.roots != roots;
        self.files.roots = roots.clone();
        Task::perform(
            async move { tokio::task::spawn_blocking(move || scan::scan(&roots, moving.as_deref())).await.unwrap_or_default() },
            |r| Msg::Files(FilesMsg::Scanned(r)),
        )
    }

    /// Makes sure the selected row is inside the visible part of the list.
    fn files_reveal(&mut self) -> Task<Msg> {
        let Some(sel) = &self.files.sel else { return Task::none() };
        let Some(i) = rows::row_index(&self.files.rows, sel) else { return Task::none() };
        let h = self.files_view_h();
        let (top, off) = (i as f32 * ROW_H, self.files.scroll_y);
        let y = if top < off {
            top
        } else if top + ROW_H > off + h {
            top + ROW_H - h
        } else {
            return Task::none();
        };
        self.files.scroll_y = y;
        iced::widget::operation::scroll_to(scroll_id(), AbsoluteOffset { x: 0.0, y })
    }

    pub(super) fn files_view_h(&self) -> f32 {
        if self.files.view_h > 0.0 { self.files.view_h } else { (self.win_h - 330.0).max(ROW_H * 4.0) }
    }

    fn files_select(&mut self, sel: Option<Sel>) -> Task<Msg> {
        if sel.is_none() {
            return Task::none();
        }
        self.files.sel = sel;
        self.files_reveal()
    }

    fn files_key(&mut self, ev: keyboard::Event) -> Task<Msg> {
        use keyboard::Key;
        use keyboard::key::Named;
        let keyboard::Event::KeyPressed { key, modifiers, .. } = ev else { return Task::none() };
        if self.tab != Tab::Files {
            return Task::none();
        }
        if self.files.panel.is_some() {
            if key.as_ref() == Key::Named(Named::Escape) {
                self.files.panel = None;
            }
            return Task::none();
        }
        let page = ((self.files_view_h() / ROW_H) as isize - 1).max(1);
        let cur = self.files.sel.clone();
        match key.as_ref() {
            Key::Named(Named::ArrowUp) => {
                let s = rows::step(&self.files.rows, cur.as_ref(), -1);
                self.files_select(s)
            }
            Key::Named(Named::ArrowDown) => {
                let s = rows::step(&self.files.rows, cur.as_ref(), 1);
                self.files_select(s)
            }
            Key::Named(Named::PageUp) => {
                let s = rows::step(&self.files.rows, cur.as_ref(), -page);
                self.files_select(s)
            }
            Key::Named(Named::PageDown) => {
                let s = rows::step(&self.files.rows, cur.as_ref(), page);
                self.files_select(s)
            }
            Key::Named(Named::Home) => {
                let s = rows::edge(&self.files.rows, false);
                self.files_select(s)
            }
            Key::Named(Named::End) => {
                let s = rows::edge(&self.files.rows, true);
                self.files_select(s)
            }
            Key::Named(Named::Enter) => {
                if let Some(s) = cur {
                    self.files_activate(s);
                }
                Task::none()
            }
            Key::Named(Named::Space) | Key::Character(" ") => {
                self.files_reveal_selected();
                Task::none()
            }
            Key::Named(Named::Delete) => {
                self.files_delete(modifiers.shift());
                Task::none()
            }
            _ => Task::none(),
        }
    }

    /// Opens a file, or toggles a folder.
    fn files_activate(&mut self, sel: Sel) {
        match sel {
            Sel::File(p) => {
                if let Err(e) = ops::open_file(&p) {
                    self.files.say(format!("Could not open file: {e}"));
                }
            }
            Sel::Folder(p) => self.files_toggle(p),
        }
    }

    fn files_toggle(&mut self, p: PathBuf) {
        if !self.files.collapsed.remove(&p) {
            self.files.collapsed.insert(p);
        }
        self.files.rebuild();
    }

    fn files_reveal_selected(&mut self) {
        let path = match &self.files.sel {
            Some(Sel::File(p)) | Some(Sel::Folder(p)) => p.clone(),
            None => return,
        };
        if let Err(e) = ops::reveal(&path) {
            self.files.say(format!("Could not open folder: {e}"));
        }
    }

    fn files_delete(&mut self, permanent: bool) {
        let Some(Sel::File(path)) = self.files.sel.clone() else { return };
        if self.files.job_for(&path).is_some() {
            self.files.say("That file has a job running - cancel it first");
            return;
        }
        let mode = if permanent { DeleteMode::Permanent } else { DeleteMode::Trash };
        let root = self.files.records.get(&path).map(|r| r.root.clone());
        let name = file_name(&path).into_owned();
        match ops::delete(&path, mode) {
            Err(e) => self.files.say(format!("Delete failed: {e}")),
            Ok(()) => {
                self.files.records.remove(&path);
                let note = match (self.files.delete_empty, root, path.parent()) {
                    (true, Some(r), Some(dir)) => ops::remove_empty_folder(dir, &r, mode),
                    _ => String::new(),
                };
                self.files.rebuild();
                let how = if permanent { "Deleted permanently" } else { "Moved to recycle bin" };
                self.files.say(format!("{how}: {name}{note}"));
            }
        }
    }

    /// Checks a job can start for `target`; says why in the status line when it cannot.
    fn files_can_start(&mut self, kind: JobKind, target: &Path) -> bool {
        let label = kind.label();
        if !target.is_file() {
            self.files.say(format!("{label} failed: the file no longer exists"));
            return false;
        }
        if self.files.jobs.iter().any(|j| j.kind == kind) {
            self.files.say(format!("A {label} is already running - please wait"));
            return false;
        }
        if self.files.job_for(target).is_some() {
            self.files.say("That file already has a job running");
            return false;
        }
        true
    }

    fn files_launch<F, Fut>(&mut self, kind: JobKind, target: PathBuf, make: F) -> Task<Msg>
    where
        F: FnOnce(Arc<Token>) -> Fut,
        Fut: Future<Output = Outcome> + Send + 'static,
    {
        let id = self.files.next_job;
        self.files.next_job += 1;
        let token = Arc::new(Token::default());
        let fut = make(token.clone());
        self.files.jobs.push(RunningJob { id, kind, target, token });
        Task::perform(fut, move |o| Msg::Files(FilesMsg::JobDone(id, o)))
    }

    pub(super) fn files_update(&mut self, m: FilesMsg) -> Task<Msg> {
        let mut task = Task::none();
        match m {
            FilesMsg::Scanned(mut res) => {
                let now = Instant::now();
                let mut files = std::mem::take(&mut res.files);
                if self.files.hide_posts {
                    let hidden = PostFiles::new(&self.posts.posts);
                    files.retain(|s| !hidden.hides(&s.path));
                }
                let f = &mut self.files;
                f.scanning = false;
                let changed = scan::merge(&mut f.records, files, now);
                if let (Some(mv), Some(size)) = (f.moving.as_mut(), res.moving) {
                    let dt = now.saturating_duration_since(mv.last_poll).as_secs_f64().max(0.001);
                    if size != mv.size {
                        mv.rate = (size as f64 - mv.size as f64) / dt;
                    }
                    mv.size = size;
                    mv.last_poll = now;
                }
                let writing = f.records.values().filter(|r| r.writing(now)).count();
                if changed || f.stale || writing != f.built_writing {
                    f.built_writing = writing;
                    f.rebuild();
                }
            }
            FilesMsg::Key(ev) => task = self.files_key(ev),
            FilesMsg::Scrolled(v) => {
                self.files.scroll_y = v.absolute_offset().y;
                self.files.view_h = v.bounds().height;
            }
            FilesMsg::Select(s) => {
                if let Sel::Folder(p) = &s {
                    self.files_toggle(p.clone());
                }
                self.files.sel = Some(s);
            }
            FilesMsg::Activate(s) => self.files_activate(s),
            FilesMsg::HeaderSort(k) => {
                self.files.sort = self.files.sort.click(k);
                self.files.save_prefs();
                self.files.rebuild();
            }
            FilesMsg::ToggleGroup(on) => {
                self.files.group = on;
                self.files.save_prefs();
                self.files.rebuild();
            }
            FilesMsg::ToggleDeleteEmpty(on) => {
                self.files.delete_empty = on;
                self.files.save_prefs();
            }
            FilesMsg::ToggleHidePosts(on) => {
                self.files.hide_posts = on;
                self.files.save_prefs();
                self.files.stale = true;
                self.files.last_scan = None;
            }
            FilesMsg::ExpandAll => {
                self.files.collapsed.clear();
                self.files.rebuild();
            }
            FilesMsg::CollapseAll => {
                let subs = rows::subfolders(&self.files.records, &self.files.roots);
                self.files.collapsed.extend(subs);
                self.files.rebuild();
            }
            FilesMsg::OpenSelected => {
                if let Some(Sel::File(p)) = self.files.sel.clone() {
                    self.files_activate(Sel::File(p));
                }
            }
            FilesMsg::RevealSelected => self.files_reveal_selected(),
            FilesMsg::DeleteSelected => self.files_delete(false),
            FilesMsg::OpenPanel(kind) => {
                if self.files.panel.as_ref().map(Panel::kind) == Some(kind) {
                    self.files.panel = None;
                } else if let Some(Sel::File(target)) = self.files.sel.clone() {
                    self.files.panel = Some(match kind {
                        PanelKind::Fixup => Panel::Fixup { target, delete_original: true, convert_mp4: true },
                        PanelKind::Trim => Panel::Trim {
                            target,
                            start: Default::default(),
                            end: Default::default(),
                            delete_original: false,
                            convert_mp4: false,
                        },
                        PanelKind::Move => Panel::Move { target, fixup: false },
                    });
                }
            }
            FilesMsg::ClosePanel => self.files.panel = None,
            FilesMsg::FixupDelete(v) => {
                if let Some(Panel::Fixup { delete_original, .. }) = &mut self.files.panel {
                    *delete_original = v;
                }
            }
            FilesMsg::FixupMp4(v) => {
                if let Some(Panel::Fixup { convert_mp4, .. }) = &mut self.files.panel {
                    *convert_mp4 = v;
                }
            }
            FilesMsg::TrimDelete(v) => {
                if let Some(Panel::Trim { delete_original, .. }) = &mut self.files.panel {
                    *delete_original = v;
                }
            }
            FilesMsg::TrimMp4(v) => {
                if let Some(Panel::Trim { convert_mp4, .. }) = &mut self.files.panel {
                    *convert_mp4 = v;
                }
            }
            FilesMsg::MoveFixup(v) => {
                if let Some(Panel::Move { fixup, .. }) = &mut self.files.panel {
                    *fixup = v;
                }
            }
            FilesMsg::TrimField(is_end, i, v) => {
                let digits: String = v.chars().filter(|c| c.is_ascii_digit()).take(2).collect();
                if let Some(Panel::Trim { start, end, .. }) = &mut self.files.panel {
                    let slot = if is_end { &mut end[i] } else { &mut start[i] };
                    let filled = slot.len() < 2 && digits.len() == 2;
                    *slot = digits;
                    let n = usize::from(is_end) * 3 + i;
                    if filled && n < 5 {
                        task = iced::widget::operation::focus(trim_id(n + 1));
                    }
                }
            }
            FilesMsg::RunFixup => {
                let Some(Panel::Fixup { target, delete_original, convert_mp4 }) = self.files.panel.clone() else { return task };
                if !self.files_can_start(JobKind::Fixup, &target) {
                    return task;
                }
                self.files.panel = None;
                self.files.say(format!("Fixup started: {}", file_name(&target)));
                let spec = FixupSpec { path: target.clone(), delete_original, convert_mp4 };
                task = self.files_launch(JobKind::Fixup, target, move |t| jobs::fixup(spec, t));
            }
            FilesMsg::RunTrim => {
                let Some(Panel::Trim { target, start, end, delete_original, convert_mp4 }) = self.files.panel.clone() else { return task };
                let (s, e) = match (jobs::parse_hms(&start), jobs::parse_hms(&end)) {
                    (Ok(s), Ok(e)) => (s, e),
                    (Err(m), _) | (_, Err(m)) => {
                        self.files.say(m);
                        return task;
                    }
                };
                if e <= s {
                    self.files.say("End must be after Start");
                    return task;
                }
                if !self.files_can_start(JobKind::Trim, &target) {
                    return task;
                }
                self.files.panel = None;
                self.files.say(format!("Trim started: {}", file_name(&target)));
                let spec = TrimSpec { path: target.clone(), start: jobs::fmt_hms(s), end: jobs::fmt_hms(e), delete_original, convert_mp4 };
                task = self.files_launch(JobKind::Trim, target, move |t| jobs::trim(spec, t));
            }
            FilesMsg::ChooseMoveDest => {
                let Some(Panel::Move { target, .. }) = &self.files.panel else { return task };
                let name = file_name(target).into_owned();
                let dir = PathBuf::from(&self.files.move_dir);
                task = Task::perform(
                    async move {
                        let mut dlg = rfd::AsyncFileDialog::new().set_title("Move file to").set_file_name(name);
                        if dir.is_dir() {
                            dlg = dlg.set_directory(dir);
                        }
                        dlg.save_file().await.map(|h| h.path().to_path_buf())
                    },
                    |p| Msg::Files(FilesMsg::MoveDestChosen(p)),
                );
            }
            FilesMsg::MoveDestChosen(None) => {}
            FilesMsg::MoveDestChosen(Some(dest)) => {
                let Some(Panel::Move { target, fixup }) = self.files.panel.clone() else { return task };
                if !self.files_can_start(JobKind::Move, &target) {
                    return task;
                }
                self.files.panel = None;
                if let Some(dir) = dest.parent() {
                    self.files.move_dir = dir.to_string_lossy().into_owned();
                    self.files.save_prefs();
                }
                let cleanup = if self.files.delete_empty {
                    let root = self.files.records.get(&target).map(|r| r.root.clone());
                    root.zip(target.parent().map(Path::to_path_buf)).map(|(r, folder)| (folder, r))
                } else {
                    None
                };
                self.files.moving = Some(Moving { name: file_name(&dest).into_owned(), tmp: jobs::move_tmp(&dest), size: 0, rate: 0.0, last_poll: Instant::now() });
                self.files.say(format!("Move started: {}", file_name(&target)));
                let spec = MoveSpec { src: target.clone(), dest, fixup, cleanup };
                task = self.files_launch(JobKind::Move, target, move |t| jobs::move_file(spec, t));
            }
            FilesMsg::CancelJob(id) => {
                let cancelled = self.files.jobs.iter().find(|j| j.id == id).is_some_and(|j| j.token.cancel());
                if cancelled {
                    self.files.say("Cancelling…");
                }
            }
            FilesMsg::JobDone(id, outcome) => {
                if let Some(pos) = self.files.jobs.iter().position(|j| j.id == id) {
                    let job = self.files.jobs.remove(pos);
                    if job.kind == JobKind::Move {
                        self.files.moving = None;
                    }
                }
                let (Outcome::Done(msg) | Outcome::Cancelled(msg) | Outcome::Failed(msg)) = outcome;
                self.files.say(msg.clone());
                self.files.last_scan = None;
                self.note(format!("Files: {msg}"));
            }
        }
        task
    }
}
