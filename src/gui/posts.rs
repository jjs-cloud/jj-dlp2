//! State, messages and logic of the Posts monitor.

use super::{App, Msg, SettingsTab, Tab};
use crate::api::{now_secs, DEFAULT_NOTIFY_TIMEOUT_SECS};
use crate::filemgr::ops::{self, DeleteMode};
use crate::policy::notify;
use crate::posts::runner::{self, CycleReport, DownloadOutcome};
use crate::posts::{self, Post, PostSite, PostState, Preset, Section, SiteId, MIN_INTERVAL_SECS};
use iced::widget::text_editor::{Action, Content, Edit};
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

/// Identifies an account: its site and name.
pub type AccountKey = (SiteId, String);

/// Editor state of one site's settings card.
pub struct SiteUi {
    pub interval: String,
    pub checker: Content,
    pub download: Content,
    pub new_account: String,
}

impl SiteUi {
    fn from_site(site: &PostSite) -> Self {
        SiteUi {
            interval: site.interval_secs.map(|n| n.to_string()).unwrap_or_default(),
            checker: Content::with_text(&site.checker_args),
            download: Content::with_text(&site.download_args),
            new_account: String::new(),
        }
    }
}

pub struct PostsState {
    pub cfg: posts::PostsConfig,
    pub posts: Vec<Post>,
    pub section: Section,
    pub account: Option<AccountKey>,
    pub search: String,
    pub shown: usize,
    pub ui: HashMap<SiteId, SiteUi>,
    pub confirm_remove: Option<SiteId>,
    pub errors: HashMap<AccountKey, String>,
    pub last_check: Option<u64>,
    pub checking: bool,
    next_check: HashMap<SiteId, Instant>,
    downloads: usize,
    status: Option<(String, u64)>,
}

impl PostsState {
    pub fn new() -> Self {
        let cfg = posts::load_config();
        let soon = Instant::now() + FIRST_CHECK_DELAY;
        PostsState {
            ui: cfg.sites.iter().map(|s| (s.id, SiteUi::from_site(s))).collect(),
            next_check: cfg.sites.iter().map(|s| (s.id, soon)).collect(),
            cfg,
            posts: posts::load_library(),
            section: Section::Unviewed,
            account: None,
            search: String::new(),
            shown: PAGE,
            confirm_remove: None,
            errors: HashMap::new(),
            last_check: None,
            checking: false,
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
        self.account.as_ref().is_none_or(|(sid, name)| p.site_id == *sid && p.account.eq_ignore_ascii_case(name))
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
    pub fn count(&self, section: Section, account: Option<(SiteId, &str)>) -> usize {
        self.posts
            .iter()
            .filter(|p| p.in_section(section) && account.is_none_or(|(sid, n)| p.site_id == sid && p.account.eq_ignore_ascii_case(n)))
            .count()
    }

    pub fn unviewed_total(&self) -> usize {
        self.count(Section::Unviewed, None)
    }

    /// Seconds until the next site is checked, or `None` while paused, checking or with nothing to check.
    pub fn secs_to_next(&self) -> Option<u64> {
        if self.checking || self.cfg.paused {
            return None;
        }
        let now = Instant::now();
        self.cfg
            .sites
            .iter()
            .filter(|s| s.has_enabled_accounts())
            .map(|s| self.next_check.get(&s.id).map_or(0, |t| t.saturating_duration_since(now).as_secs()))
            .min()
    }

    /// Applies an editor action to a site's checker or download argument box and saves edits.
    fn edit_args(&mut self, id: SiteId, action: Action, checker: bool) {
        if matches!(action, Action::Edit(Edit::Enter)) {
            return;
        }
        let is_edit = action.is_edit();
        let Some(ui) = self.ui.get_mut(&id) else { return };
        let content = if checker { &mut ui.checker } else { &mut ui.download };
        content.perform(action);
        if !is_edit {
            return;
        }
        let text = content.text();
        if let Some(site) = self.cfg.site_mut(id) {
            if checker {
                site.checker_args = text;
            } else {
                site.download_args = text;
            }
        }
        self.save_cfg();
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

/// One editable field of a site's settings card.
#[derive(Debug, Clone)]
pub enum SiteField {
    Name(String),
    Interval(String),
    Exe(String),
    UrlTemplate(String),
    OutDir(String),
    Checker(Action),
    Download(Action),
    NewAccount(String),
}

#[derive(Debug, Clone)]
pub enum PostsMsg {
    Checked(CycleReport),
    Downloaded(String, DownloadOutcome),
    Notified(Vec<String>),
    SetSection(Section),
    SetAccount(Option<AccountKey>),
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
    AddSite(Preset),
    RemoveSite(SiteId),
    ConfirmRemoveSite(SiteId),
    CancelRemoveSite,
    Site(SiteId, SiteField),
    AddAccount(SiteId),
    RemoveAccount(SiteId, String),
    ToggleAccount(SiteId, String, bool),
    BrowseOutput(SiteId),
    OutputChosen(SiteId, Option<PathBuf>),
    OpenOutputDir(SiteId),
    AutoDownload(bool),
    Backfill(bool),
    MarkOnOpen(bool),
    NotifyDesktop(bool),
    NotifyNtfy(bool),
    NtfyTopic(String),
}

impl App {
    fn posts_out_dir(&self, id: SiteId) -> PathBuf {
        let global = &self.settings.default_output_dir;
        self.posts.cfg.site(id).map_or_else(|| PathBuf::from(global), |s| s.output_folder(global))
    }

    /// Starts due checks and queued downloads; called on every tick.
    pub(super) fn posts_tick(&mut self) -> Task<Msg> {
        let mut tasks = Vec::new();
        let now = Instant::now();
        if !self.posts.checking && !self.posts.cfg.paused {
            let due: Vec<PostSite> = self
                .posts
                .cfg
                .sites
                .iter()
                .filter(|s| s.has_enabled_accounts() && self.posts.next_check.get(&s.id).is_none_or(|t| now >= *t))
                .cloned()
                .collect();
            if !due.is_empty() {
                self.posts.checking = true;
                tasks.push(self.posts_start_check(due));
            }
        }
        while self.posts.downloads < MAX_DOWNLOADS {
            let Some(next) = self.next_queued() else { break };
            tasks.push(self.posts_start_download(next));
        }
        Task::batch(tasks)
    }

    fn posts_start_check(&self, sites: Vec<PostSite>) -> Task<Msg> {
        let known: HashSet<String> = self.posts.posts.iter().map(|p| p.id.clone()).collect();
        Task::perform(runner::check_cycle(sites, known), |r| Msg::Posts(PostsMsg::Checked(r)))
    }

    /// The id of the newest queued post.
    fn next_queued(&self) -> Option<String> {
        self.posts.posts.iter().filter(|p| p.state == PostState::Queued).max_by_key(|p| p.sort_time()).map(|p| p.id.clone())
    }

    fn posts_start_download(&mut self, id: String) -> Task<Msg> {
        let site_id = self.posts.posts.iter().find(|p| p.id == id).map(|p| p.site_id);
        let site = site_id.and_then(|sid| self.posts.cfg.site(sid).cloned());
        let Some(post) = self.posts.post_mut(&id) else { return Task::none() };
        let Some(site) = site else {
            post.state = PostState::Failed;
            post.attempts = posts::MAX_ATTEMPTS;
            post.error = "Its site was removed".into();
            return Task::none();
        };
        post.state = PostState::Downloading;
        let post = post.clone();
        self.posts.downloads += 1;
        let dir = self.settings.default_output_dir.clone();
        Task::perform(runner::download(site, post, dir), move |o| Msg::Posts(PostsMsg::Downloaded(id.clone(), o)))
    }

    /// Merges a finished check into the library and queues what is new.
    fn posts_on_checked(&mut self, report: CycleReport) -> Task<Msg> {
        let mut tasks = Vec::new();
        let mut logs = Vec::new();
        let now = Instant::now();
        self.posts.checking = false;
        self.posts.last_check = Some(report.at);
        for sid in &report.site_ids {
            if let Some(site) = self.posts.cfg.site(*sid) {
                let next = now + Duration::from_secs(site.interval_or_default());
                self.posts.next_check.insert(*sid, next);
            }
        }
        for ar in report.accounts {
            let key = (ar.site_id, ar.name.clone());
            let Some(site) = self.posts.cfg.site_mut(ar.site_id) else { continue };
            let Some(acct) = site.accounts.iter_mut().find(|a| a.name == ar.name) else { continue };
            let site_name = site.name.clone();
            if let Some(e) = &ar.error {
                if self.posts.errors.insert(key, e.clone()).as_ref() != Some(e) {
                    logs.push(format!("Posts: {site_name} @{}: {e}", ar.name));
                }
                continue;
            }
            self.posts.errors.remove(&key);
            let first = !acct.baselined;
            acct.baselined = true;
            let (backfill, auto) = (self.posts.cfg.backfill, self.posts.cfg.auto_download);
            let mut added = Vec::new();
            for mut post in ar.new_posts {
                if self.posts.posts.iter().any(|p| p.id == post.id) {
                    continue;
                }
                if first && !backfill {
                    post.viewed = true;
                } else if auto {
                    post.state = PostState::Queued;
                }
                added.push(post);
            }
            if added.is_empty() {
                continue;
            }
            logs.push(format!("Posts: {} new from @{} on {site_name}", added.len(), ar.name));
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
            PostsMsg::CheckNow => {
                let now = Instant::now();
                for site in &self.posts.cfg.sites {
                    self.posts.next_check.insert(site.id, now);
                }
            }
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
            PostsMsg::AddSite(preset) => match self.posts.cfg.add_site(preset) {
                Some(id) => {
                    let Some(site) = self.posts.cfg.site(id) else { return Task::none() };
                    let name = site.name.clone();
                    self.posts.ui.insert(id, SiteUi::from_site(site));
                    self.posts.next_check.insert(id, Instant::now());
                    self.posts.save_cfg();
                    self.posts.say(format!("{name} has been added!"));
                }
                None => self.posts.say("TikTok is already added"),
            },
            PostsMsg::RemoveSite(id) => self.posts.confirm_remove = Some(id),
            PostsMsg::CancelRemoveSite => self.posts.confirm_remove = None,
            PostsMsg::ConfirmRemoveSite(id) => self.posts_remove_site(id),
            PostsMsg::Site(id, field) => self.posts_site_field(id, field),
            PostsMsg::AddAccount(id) => {
                let input = self.posts.ui.get(&id).map(|u| u.new_account.clone()).unwrap_or_default();
                let account = self.posts.cfg.site(id).and_then(|s| s.account_from_input(&input));
                match account {
                    Some(a) => {
                        let name = a.name.clone();
                        if let Some(site) = self.posts.cfg.site_mut(id) {
                            site.accounts.push(a);
                        }
                        if let Some(ui) = self.posts.ui.get_mut(&id) {
                            ui.new_account.clear();
                        }
                        self.posts.next_check.insert(id, Instant::now());
                        self.posts.save_cfg();
                        self.posts.say(format!("Added @{name}"));
                    }
                    None => self.posts.say("Enter a new username or profile URL"),
                }
            }
            PostsMsg::RemoveAccount(id, name) => {
                if let Some(site) = self.posts.cfg.site_mut(id) {
                    site.accounts.retain(|a| a.name != name);
                }
                self.posts.errors.remove(&(id, name.clone()));
                if self.posts.account.as_ref() == Some(&(id, name)) {
                    self.posts.account = None;
                }
                self.posts.save_cfg();
            }
            PostsMsg::ToggleAccount(id, name, on) => {
                let acct = self.posts.cfg.site_mut(id).and_then(|s| s.accounts.iter_mut().find(|a| a.name == name));
                if let Some(a) = acct {
                    a.enabled = on;
                }
                self.posts.save_cfg();
            }
            PostsMsg::BrowseOutput(id) => {
                let start = self.posts_out_dir(id);
                return Task::perform(
                    async move {
                        let mut dlg = rfd::AsyncFileDialog::new().set_title("Posts download folder");
                        if start.is_dir() {
                            dlg = dlg.set_directory(start);
                        }
                        dlg.pick_folder().await.map(|h| h.path().to_path_buf())
                    },
                    move |p| Msg::Posts(PostsMsg::OutputChosen(id, p)),
                );
            }
            PostsMsg::OutputChosen(id, Some(dir)) => {
                if let Some(site) = self.posts.cfg.site_mut(id) {
                    site.output_dir = dir.to_string_lossy().into_owned();
                }
                self.posts.save_cfg();
            }
            PostsMsg::OutputChosen(_, None) => {}
            PostsMsg::OpenOutputDir(id) => {
                let dir = self.posts_out_dir(id);
                let _ = std::fs::create_dir_all(&dir);
                let _ = open::that_detached(dir);
            }
            PostsMsg::AutoDownload(v) => self.posts_edit(|c| c.auto_download = v),
            PostsMsg::Backfill(v) => self.posts_edit(|c| c.backfill = v),
            PostsMsg::MarkOnOpen(v) => self.posts_edit(|c| c.mark_on_open = v),
            PostsMsg::NotifyDesktop(v) => self.posts_edit(|c| c.notify_desktop = v),
            PostsMsg::NotifyNtfy(v) => self.posts_edit(|c| c.notify_ntfy = v),
            PostsMsg::NtfyTopic(v) => self.posts_edit(|c| c.ntfy_topic = v),
        }
        Task::none()
    }

    /// Applies one config edit and saves it.
    fn posts_edit(&mut self, f: impl FnOnce(&mut posts::PostsConfig)) {
        f(&mut self.posts.cfg);
        self.posts.save_cfg();
    }

    /// Applies one edit of a site's settings card and saves it.
    fn posts_site_field(&mut self, id: SiteId, field: SiteField) {
        match field {
            SiteField::Checker(a) => return self.posts.edit_args(id, a, true),
            SiteField::Download(a) => return self.posts.edit_args(id, a, false),
            SiteField::NewAccount(v) => {
                if let Some(ui) = self.posts.ui.get_mut(&id) {
                    ui.new_account = v;
                }
                return;
            }
            SiteField::Interval(v) => {
                let parsed = match v.trim() {
                    "" => Some(None),
                    t => t.parse::<u64>().ok().filter(|n| *n >= MIN_INTERVAL_SECS).map(Some),
                };
                if let (Some(value), Some(site)) = (parsed, self.posts.cfg.site_mut(id)) {
                    site.interval_secs = value;
                }
                if let Some(ui) = self.posts.ui.get_mut(&id) {
                    ui.interval = v;
                }
            }
            other => {
                let Some(site) = self.posts.cfg.site_mut(id) else { return };
                match other {
                    SiteField::Name(v) => site.name = v,
                    SiteField::Exe(v) => site.exe_path = v,
                    SiteField::UrlTemplate(v) => site.url_template = v,
                    SiteField::OutDir(v) => site.output_dir = v,
                    _ => {}
                }
            }
        }
        self.posts.save_cfg();
    }

    /// Removes a site with its accounts, posts and thumbnails.
    fn posts_remove_site(&mut self, id: SiteId) {
        self.posts.confirm_remove = None;
        self.posts.cfg.sites.retain(|s| s.id != id);
        self.posts.ui.remove(&id);
        self.posts.next_check.remove(&id);
        self.posts.errors.retain(|(sid, _), _| *sid != id);
        if self.posts.account.as_ref().is_some_and(|(sid, _)| *sid == id) {
            self.posts.account = None;
        }
        for p in self.posts.posts.iter().filter(|p| p.site_id == id) {
            let _ = std::fs::remove_file(p.thumb_path());
        }
        self.posts.posts.retain(|p| p.site_id != id);
        self.posts.save_cfg();
        self.posts.save_lib();
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
