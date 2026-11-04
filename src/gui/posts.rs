//! State, messages and logic of the Posts monitor.

use super::{App, Msg, SettingsTab, Tab};
use crate::api::{now_secs, DEFAULT_NOTIFY_TIMEOUT_SECS};
use crate::filemgr::ops::{self, DeleteMode};
use crate::policy::notify;
use crate::posts::runner::{self, CycleReport, DownloadOutcome};
use crate::posts::{self, Post, PostState, PostsConfig, Section, MIN_INTERVAL_SECS};
use iced::Task;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Cards shown before "Show more" is pressed, and added by each press.
pub const PAGE: usize = 40;
/// Downloads that run at the same time.
const MAX_DOWNLOADS: usize = 2;
const STATUS_SECS: u64 = 5;
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(3);

pub struct PostsState {
    pub cfg: PostsConfig,
    pub posts: Vec<Post>,
    pub section: Section,
    pub account: Option<String>,
    pub search: String,
    pub shown: usize,
    pub new_account: String,
    pub interval_text: String,
    pub errors: HashMap<String, String>,
    pub last_check: Option<u64>,
    pub checking: bool,
    pub next_check: Instant,
    downloads: usize,
    status: Option<(String, u64)>,
}

impl PostsState {
    pub fn new() -> Self {
        let cfg = posts::load_config();
        PostsState {
            interval_text: cfg.interval_secs.to_string(),
            cfg,
            posts: posts::load_library(),
            section: Section::Unviewed,
            account: None,
            search: String::new(),
            shown: PAGE,
            new_account: String::new(),
            errors: HashMap::new(),
            last_check: None,
            checking: false,
            next_check: Instant::now() + FIRST_CHECK_DELAY,
            downloads: 0,
            status: None,
        }
    }

    fn say(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), now_secs()));
    }

    /// The status line text while it is still fresh.
    pub fn status_text(&self, now: u64) -> Option<&str> {
        self.status.as_ref().filter(|(_, at)| now.saturating_sub(*at) < STATUS_SECS).map(|(m, _)| m.as_str())
    }

    fn save_cfg(&self) {
        posts::save_config(&self.cfg);
    }

    fn save_lib(&self) {
        posts::save_library(&self.posts);
    }

    fn post_mut(&mut self, id: &str) -> Option<&mut Post> {
        self.posts.iter_mut().find(|p| p.id == id)
    }

    fn account_ok(&self, p: &Post) -> bool {
        self.account.as_deref().is_none_or(|a| p.account.eq_ignore_ascii_case(a))
    }

    /// Posts in the current section, account and search, in display order.
    pub fn visible(&self) -> Vec<&Post> {
        let q = self.search.trim().to_lowercase();
        let mut v: Vec<&Post> =
            self.posts.iter().filter(|p| p.in_section(self.section) && self.account_ok(p) && p.matches_query(&q)).collect();
        v.sort_by_key(|p| p.sort_time());
        if self.cfg.newest_first {
            v.reverse();
        }
        v
    }

    /// Posts in `section`, for the account when one is given.
    pub fn count(&self, section: Section, account: Option<&str>) -> usize {
        self.posts
            .iter()
            .filter(|p| p.in_section(section) && account.is_none_or(|a| p.account.eq_ignore_ascii_case(a)))
            .count()
    }

    pub fn unviewed_total(&self) -> usize {
        self.count(Section::Unviewed, None)
    }

    /// Seconds until the next check, or `None` while paused, checking or with nothing to check.
    pub fn secs_to_next(&self) -> Option<u64> {
        if self.checking || self.cfg.paused || !self.cfg.accounts.iter().any(|a| a.enabled) {
            return None;
        }
        Some(self.next_check.saturating_duration_since(Instant::now()).as_secs())
    }

    fn reset_view(&mut self) {
        self.shown = PAGE;
    }

    fn set_viewed(&mut self, id: &str, viewed: bool) {
        if let Some(p) = self.post_mut(id) {
            p.viewed = viewed;
            p.viewed_at = viewed.then(now_secs);
        }
    }

    fn queue(&mut self, id: &str) {
        if let Some(p) = self.post_mut(id) {
            if matches!(p.state, PostState::Remote | PostState::Failed) {
                p.state = PostState::Queued;
                p.attempts = 0;
                p.error.clear();
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum PostsMsg {
    Checked(CycleReport),
    Downloaded(String, DownloadOutcome),
    Notified(Vec<String>),
    SetSection(Section),
    SetAccount(Option<String>),
    Search(String),
    ShowMore,
    CheckNow,
    Paused(bool),
    Play(String),
    OpenPage(String),
    ToggleViewed(String),
    ToggleStar(String),
    Download(String),
    Reveal(String),
    Delete(String),
    MarkAllViewed,
    NewestFirst(bool),
    CardWidth(f32),
    CardWidthSave,
    OpenSettings,
    NewAccount(String),
    AddAccount,
    RemoveAccount(String),
    ToggleAccount(String, bool),
    Interval(String),
    Exe(String),
    CheckerArgs(String),
    DownloadArgs(String),
    OutputDir(String),
    SiteTmpl(String),
    AutoDownload(bool),
    Backfill(bool),
    MarkOnOpen(bool),
    NotifyDesktop(bool),
    NotifyNtfy(bool),
    NtfyTopic(String),
    BrowseOutput,
    OutputChosen(Option<PathBuf>),
    OpenOutputDir,
}

impl App {
    fn posts_out_dir(&self) -> PathBuf {
        self.posts.cfg.output_folder(&self.settings.default_output_dir)
    }

    /// Starts due checks and queued downloads; called on every tick.
    pub(super) fn posts_tick(&mut self) -> Task<Msg> {
        let mut tasks = Vec::new();
        let p = &mut self.posts;
        let due = Instant::now() >= p.next_check;
        if !p.checking && !p.cfg.paused && due && p.cfg.accounts.iter().any(|a| a.enabled) {
            p.checking = true;
            tasks.push(self.posts_start_check());
        }
        while self.posts.downloads < MAX_DOWNLOADS {
            let Some(next) = self.next_queued() else { break };
            tasks.push(self.posts_start_download(next));
        }
        Task::batch(tasks)
    }

    fn posts_start_check(&self) -> Task<Msg> {
        let cfg = self.posts.cfg.clone();
        let known: HashSet<String> = self.posts.posts.iter().map(|p| p.id.clone()).collect();
        Task::perform(runner::check_cycle(cfg, known), |r| Msg::Posts(PostsMsg::Checked(r)))
    }

    /// The id of the newest queued post.
    fn next_queued(&self) -> Option<String> {
        self.posts.posts.iter().filter(|p| p.state == PostState::Queued).max_by_key(|p| p.sort_time()).map(|p| p.id.clone())
    }

    fn posts_start_download(&mut self, id: String) -> Task<Msg> {
        let Some(post) = self.posts.post_mut(&id) else { return Task::none() };
        post.state = PostState::Downloading;
        let post = post.clone();
        self.posts.downloads += 1;
        let cfg = self.posts.cfg.clone();
        let dir = self.settings.default_output_dir.clone();
        Task::perform(runner::download(cfg, post, dir), move |o| Msg::Posts(PostsMsg::Downloaded(id.clone(), o)))
    }

    /// Merges a finished check into the library and queues what is new.
    fn posts_on_checked(&mut self, report: CycleReport) -> Task<Msg> {
        let mut tasks = Vec::new();
        let mut logs = Vec::new();
        let interval = Duration::from_secs(self.posts.cfg.interval_secs.max(MIN_INTERVAL_SECS));
        self.posts.checking = false;
        self.posts.last_check = Some(report.at);
        self.posts.next_check = Instant::now() + interval;
        for ar in report.accounts {
            let Some(idx) = self.posts.cfg.accounts.iter().position(|a| a.name == ar.name) else { continue };
            if let Some(e) = &ar.error {
                if self.posts.errors.insert(ar.name.clone(), e.clone()).as_ref() != Some(e) {
                    logs.push(format!("Posts: @{}: {e}", ar.name));
                }
                continue;
            }
            self.posts.errors.remove(&ar.name);
            let first = !self.posts.cfg.accounts[idx].baselined;
            self.posts.cfg.accounts[idx].baselined = true;
            let cfg = &self.posts.cfg;
            let mut added = Vec::new();
            for mut post in ar.new_posts {
                if self.posts.posts.iter().any(|p| p.id == post.id) {
                    continue;
                }
                if first && !cfg.backfill {
                    post.viewed = true;
                } else if cfg.auto_download {
                    post.state = PostState::Queued;
                }
                added.push(post);
            }
            if added.is_empty() {
                continue;
            }
            logs.push(format!("Posts: {} new from @{}", added.len(), ar.name));
            if !first {
                if let Some(t) = self.posts_notify(&ar.name, &added) {
                    tasks.push(t);
                }
            }
            self.posts.posts.extend(added);
        }
        if self.posts.cfg.auto_download {
            for p in &mut self.posts.posts {
                if p.state == PostState::Failed && p.attempts < posts::MAX_ATTEMPTS {
                    p.state = PostState::Queued;
                }
            }
        }
        self.posts.save_cfg();
        self.posts.save_lib();
        for l in logs {
            self.push_log(l);
        }
        Task::batch(tasks)
    }

    fn posts_notify(&self, account: &str, added: &[Post]) -> Option<Task<Msg>> {
        let cfg = &self.posts.cfg;
        let targets = notify::Targets {
            desktop_timeout_secs: cfg.notify_desktop.then_some(DEFAULT_NOTIFY_TIMEOUT_SECS),
            ntfy_topic: (cfg.notify_ntfy && notify::topic_valid(&cfg.ntfy_topic)).then(|| cfg.ntfy_topic.trim().to_string()),
        };
        if targets.is_empty() {
            return None;
        }
        let msg = posts::new_posts_message(account, added);
        Some(Task::perform(runner::send_notification(msg, targets), |l| Msg::Posts(PostsMsg::Notified(l))))
    }

    fn posts_on_downloaded(&mut self, id: &str, out: DownloadOutcome) {
        self.posts.downloads = self.posts.downloads.saturating_sub(1);
        let mut log = None;
        if let Some(p) = self.posts.post_mut(id) {
            p.has_thumb = out.has_thumb;
            if p.state == PostState::Removed {
                return;
            }
            match out.file {
                Ok(path) => {
                    p.state = PostState::Ready;
                    p.file = Some(path);
                    p.error.clear();
                }
                Err(e) => {
                    p.state = PostState::Failed;
                    p.attempts = p.attempts.saturating_add(1);
                    log = Some(format!("Posts: download of {} failed: {e}", p.id));
                    p.error = e;
                }
            }
        }
        self.posts.save_lib();
        if let Some(l) = log {
            self.push_log(l);
        }
    }

    pub(super) fn posts_update(&mut self, m: PostsMsg) -> Task<Msg> {
        match m {
            PostsMsg::Checked(r) => return self.posts_on_checked(r),
            PostsMsg::Downloaded(id, o) => self.posts_on_downloaded(&id, o),
            PostsMsg::Notified(lines) => {
                for l in lines {
                    self.push_log(l);
                }
            }
            PostsMsg::SetSection(s) => {
                self.posts.section = s;
                self.posts.reset_view();
            }
            PostsMsg::SetAccount(a) => {
                self.posts.account = a;
                self.posts.reset_view();
            }
            PostsMsg::Search(q) => {
                self.posts.search = q;
                self.posts.reset_view();
            }
            PostsMsg::ShowMore => self.posts.shown += PAGE,
            PostsMsg::CheckNow => self.posts.next_check = Instant::now(),
            PostsMsg::Paused(v) => {
                self.posts.cfg.paused = v;
                self.posts.save_cfg();
            }
            PostsMsg::Play(id) => self.posts_play(&id),
            PostsMsg::OpenPage(id) => {
                if let Some(url) = self.posts.posts.iter().find(|p| p.id == id).map(|p| p.url.clone()) {
                    let _ = open::that_detached(url);
                }
            }
            PostsMsg::ToggleViewed(id) => {
                let now = self.posts.posts.iter().find(|p| p.id == id).is_some_and(|p| p.viewed);
                self.posts.set_viewed(&id, !now);
                self.posts.save_lib();
            }
            PostsMsg::ToggleStar(id) => {
                if let Some(p) = self.posts.post_mut(&id) {
                    p.starred = !p.starred;
                }
                self.posts.save_lib();
            }
            PostsMsg::Download(id) => {
                self.posts.queue(&id);
                self.posts.save_lib();
            }
            PostsMsg::Reveal(id) => {
                let file = self.posts.posts.iter().find(|p| p.id == id).and_then(|p| p.file.clone());
                if let Some(Err(e)) = file.map(|f| ops::reveal(Path::new(&f))) {
                    self.posts.say(format!("Could not open the folder: {e}"));
                }
            }
            PostsMsg::Delete(id) => self.posts_delete(&id),
            PostsMsg::MarkAllViewed => {
                let ids: Vec<String> = self.posts.visible().iter().map(|p| p.id.clone()).collect();
                let n = ids.len();
                for id in ids {
                    self.posts.set_viewed(&id, true);
                }
                self.posts.save_lib();
                self.posts.say(format!("Marked {n} as viewed"));
            }
            PostsMsg::NewestFirst(v) => {
                self.posts.cfg.newest_first = v;
                self.posts.save_cfg();
            }
            PostsMsg::CardWidth(v) => {
                self.posts.cfg.card_w = ((v / 10.0).round() * 10.0).clamp(posts::CARD_W_MIN, posts::CARD_W_MAX);
            }
            PostsMsg::CardWidthSave => self.posts.save_cfg(),
            PostsMsg::OpenSettings => {
                self.tab = Tab::Settings;
                self.settings_tab = SettingsTab::Posts;
            }
            PostsMsg::NewAccount(v) => self.posts.new_account = v,
            PostsMsg::AddAccount => {
                match self.posts.cfg.account_from_input(&self.posts.new_account) {
                    Some(a) => {
                        let name = a.name.clone();
                        self.posts.cfg.accounts.push(a);
                        self.posts.new_account.clear();
                        self.posts.next_check = Instant::now();
                        self.posts.save_cfg();
                        self.posts.say(format!("Added @{name}"));
                    }
                    None => self.posts.say("Enter a new username or profile URL"),
                }
            }
            PostsMsg::RemoveAccount(name) => {
                self.posts.cfg.accounts.retain(|a| a.name != name);
                self.posts.errors.remove(&name);
                if self.posts.account.as_deref() == Some(name.as_str()) {
                    self.posts.account = None;
                }
                self.posts.save_cfg();
            }
            PostsMsg::ToggleAccount(name, on) => {
                if let Some(a) = self.posts.cfg.accounts.iter_mut().find(|a| a.name == name) {
                    a.enabled = on;
                }
                self.posts.save_cfg();
            }
            PostsMsg::Interval(v) => {
                if let Ok(n) = v.trim().parse::<u64>() {
                    if n >= MIN_INTERVAL_SECS {
                        self.posts.cfg.interval_secs = n;
                        self.posts.save_cfg();
                    }
                }
                self.posts.interval_text = v;
            }
            PostsMsg::Exe(v) => self.posts_edit(|c| c.exe_path = v),
            PostsMsg::CheckerArgs(v) => self.posts_edit(|c| c.checker_args = v),
            PostsMsg::DownloadArgs(v) => self.posts_edit(|c| c.download_args = v),
            PostsMsg::OutputDir(v) => self.posts_edit(|c| c.output_dir = v),
            PostsMsg::SiteTmpl(v) => self.posts_edit(|c| c.site_tmpl = v),
            PostsMsg::AutoDownload(v) => self.posts_edit(|c| c.auto_download = v),
            PostsMsg::Backfill(v) => self.posts_edit(|c| c.backfill = v),
            PostsMsg::MarkOnOpen(v) => self.posts_edit(|c| c.mark_on_open = v),
            PostsMsg::NotifyDesktop(v) => self.posts_edit(|c| c.notify_desktop = v),
            PostsMsg::NotifyNtfy(v) => self.posts_edit(|c| c.notify_ntfy = v),
            PostsMsg::NtfyTopic(v) => self.posts_edit(|c| c.ntfy_topic = v),
            PostsMsg::BrowseOutput => {
                let start = self.posts_out_dir();
                return Task::perform(
                    async move {
                        let mut dlg = rfd::AsyncFileDialog::new().set_title("Posts download folder");
                        if start.is_dir() {
                            dlg = dlg.set_directory(start);
                        }
                        dlg.pick_folder().await.map(|h| h.path().to_path_buf())
                    },
                    |p| Msg::Posts(PostsMsg::OutputChosen(p)),
                );
            }
            PostsMsg::OutputChosen(Some(dir)) => self.posts_edit(|c| c.output_dir = dir.to_string_lossy().into_owned()),
            PostsMsg::OutputChosen(None) => {}
            PostsMsg::OpenOutputDir => {
                let dir = self.posts_out_dir();
                let _ = std::fs::create_dir_all(&dir);
                let _ = open::that_detached(dir);
            }
        }
        Task::none()
    }

    /// Applies one config edit and saves it.
    fn posts_edit(&mut self, f: impl FnOnce(&mut PostsConfig)) {
        f(&mut self.posts.cfg);
        self.posts.save_cfg();
    }

    /// Opens the downloaded file, or the post page when there is none, and marks it viewed.
    fn posts_play(&mut self, id: &str) {
        let Some(post) = self.posts.posts.iter().find(|p| p.id == id).cloned() else { return };
        let file = post.file.as_deref().filter(|f| post.state == PostState::Ready && Path::new(f).is_file());
        let result = match file {
            Some(f) => ops::open_file(Path::new(f)),
            None => open::that_detached(&post.url).map_err(|e| e.to_string()),
        };
        match result {
            Ok(()) if self.posts.cfg.mark_on_open => {
                self.posts.set_viewed(id, true);
                self.posts.save_lib();
            }
            Ok(()) => {}
            Err(e) => self.posts.say(format!("Could not open the post: {e}")),
        }
    }

    /// Sends the downloaded file to the recycle bin and hides the post.
    fn posts_delete(&mut self, id: &str) {
        let Some(post) = self.posts.posts.iter().find(|p| p.id == id).cloned() else { return };
        if let Some(f) = post.file.as_deref().filter(|f| Path::new(f).is_file()) {
            if let Err(e) = ops::delete(Path::new(f), DeleteMode::Trash) {
                self.posts.say(format!("Could not delete: {e}"));
                return;
            }
        }
        let _ = std::fs::remove_file(post.thumb_path());
        if let Some(p) = self.posts.post_mut(id) {
            p.state = PostState::Removed;
            p.file = None;
            p.has_thumb = false;
            p.viewed = true;
        }
        self.posts.save_lib();
        self.posts.say("Moved to the recycle bin");
    }

    /// Label of the Posts switch on the Monitor tab, with the unviewed count.
    pub(super) fn posts_tab_label(&self) -> String {
        match self.posts.unviewed_total() {
            0 => "Posts".to_string(),
            n => format!("Posts ({n})"),
        }
    }
}
