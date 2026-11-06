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
/// Time between auto-delete sweeps.
const SWEEP_EVERY: Duration = Duration::from_secs(3600);
const SECS_PER_DAY: u64 = 86_400;
/// Time a post stays listed after it is opened.
const VIEW_DELAY: Duration = Duration::from_secs(2);
/// Draft key for the unsaved blank site; real site ids start at 1.
pub const NEW_SITE: SiteId = 0;

/// Identifies an account: its site and name.
pub type AccountKey = (SiteId, String);

/// Unsaved edits to one site's settings card.
pub struct SiteDraft {
    pub name: String,
    pub interval: String,
    pub exe: String,
    pub url_template: String,
    pub output_dir: String,
    pub auto_delete: String,
    pub checker: Content,
    pub download: Content,
    pub paused: bool,
    pub auto_download: bool,
    pub backfill: bool,
    pub mark_on_open: bool,
    pub notify: bool,
    pub notify_desktop: bool,
    pub notify_ntfy: bool,
    pub ntfy_topic: String,
    pub open_m3u8: bool,
    pub m3u8: String,
    pub player: String,
}

impl SiteDraft {
    pub fn from_site(site: &PostSite) -> Self {
        SiteDraft {
            name: site.name.clone(),
            interval: site.interval_secs.map(|n| n.to_string()).unwrap_or_default(),
            exe: site.exe_path.clone(),
            url_template: site.url_template.clone(),
            output_dir: site.output_dir.clone(),
            auto_delete: site.auto_delete_days.map(|n| n.to_string()).unwrap_or_default(),
            checker: Content::with_text(&site.checker_args),
            download: Content::with_text(&site.download_args),
            paused: site.paused,
            auto_download: site.auto_download,
            backfill: site.backfill,
            mark_on_open: site.mark_on_open,
            notify: site.notify,
            notify_desktop: site.notify_desktop,
            notify_ntfy: site.notify_ntfy,
            ntfy_topic: site.ntfy_topic.clone(),
            open_m3u8: site.open_m3u8,
            m3u8: site.m3u8_args.clone(),
            player: site.player_path.clone(),
        }
    }

    /// The template for a site name: its preset's, else blank; the name is kept.
    pub fn from_defaults(name: &str) -> Self {
        let preset = match name.trim().to_ascii_lowercase().as_str() {
            "tiktok" => Preset::Tiktok,
            "youtube shorts" => Preset::YoutubeShorts,
            "twitch vods" => Preset::TwitchVods,
            _ => Preset::Blank,
        };
        SiteDraft::from_site(&PostSite { name: name.to_string(), ..PostSite::from_preset(preset) })
    }

    /// `base` with these edits applied; `None` while a field is invalid.
    pub fn build(&self, base: &PostSite) -> Option<PostSite> {
        let number = |text: &str, min: u64| match text.trim() {
            "" => Some(None),
            t => t.parse::<u64>().ok().filter(|n| *n >= min).map(Some),
        };
        if self.name.trim().is_empty() || self.url_template.trim().is_empty() || (self.notify && self.notify_ntfy && !crate::policy::notify::topic_valid(&self.ntfy_topic)) {
            return None;
        }
        Some(PostSite {
            name: self.name.trim().to_string(),
            interval_secs: number(&self.interval, MIN_INTERVAL_SECS)?,
            exe_path: self.exe.clone(),
            url_template: self.url_template.clone(),
            output_dir: self.output_dir.clone(),
            auto_delete_days: number(&self.auto_delete, 1)?,
            checker_args: self.checker.text(),
            download_args: self.download.text(),
            paused: self.paused,
            auto_download: self.auto_download,
            backfill: self.backfill,
            mark_on_open: self.mark_on_open,
            notify: self.notify,
            notify_desktop: self.notify_desktop,
            notify_ntfy: self.notify_ntfy,
            ntfy_topic: self.ntfy_topic.clone(),
            open_m3u8: self.open_m3u8,
            m3u8_args: self.m3u8.clone(),
            player_path: self.player.clone(),
            ..base.clone()
        })
    }

    /// True when the edits are valid, or the site is unsaved.
    pub fn valid(&self) -> bool {
        self.build(&PostSite::default()).is_some()
    }

    /// True when the edits differ from the saved site.
    pub fn differs_from(&self, saved: &PostSite) -> bool {
        self.build(saved).is_none_or(|b| b != *saved)
    }
}

pub struct PostsState {
    pub cfg: posts::PostsConfig,
    pub posts: Vec<Post>,
    pub section: Section,
    pub account: Option<AccountKey>,
    pub search: String,
    pub shown: usize,
    pub drafts: HashMap<SiteId, SiteDraft>,
    pub add_site: Option<SiteId>,
    pub add_name: String,
    pub added: Option<(String, u64)>,
    pub confirm_remove: Option<SiteId>,
    pub errors: HashMap<AccountKey, String>,
    pub last_check: Option<u64>,
    pub checking: bool,
    next_check: HashMap<SiteId, Instant>,
    next_sweep: Instant,
    pending_jump: Option<String>,
    pending_view: Vec<(String, Instant)>,
    downloads: usize,
    status: Option<(String, u64)>,
}

impl PostsState {
    pub fn new() -> Self {
        let cfg = posts::load_config();
        let soon = Instant::now() + FIRST_CHECK_DELAY;
        PostsState {
            drafts: cfg.sites.iter().map(|s| (s.id, SiteDraft::from_site(s))).collect(),
            add_site: None,
            add_name: String::new(),
            added: None,
            next_sweep: soon,
            pending_jump: None,
            pending_view: Vec::new(),
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

    pub(super) fn save_cfg(&self) {
        posts::save_config(&self.cfg);
    }

    pub(super) fn save_lib(&self) {
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

    /// True when every site that has accounts to check is paused.
    pub fn all_paused(&self) -> bool {
        let mut live = self.cfg.sites.iter().filter(|s| s.has_enabled_accounts()).peekable();
        live.peek().is_some() && live.all(|s| s.paused)
    }

    /// The site the username box adds to: the chosen one, else TikTok, else the first.
    pub fn add_target(&self) -> Option<SiteId> {
        let chosen = self.add_site.filter(|id| self.cfg.site(*id).is_some());
        chosen
            .or_else(|| self.cfg.sites.iter().find(|s| s.name.eq_ignore_ascii_case("tiktok")).map(|s| s.id))
            .or_else(|| self.cfg.sites.first().map(|s| s.id))
    }

    /// Seconds until the next site is checked, or `None` while paused, checking or with nothing to check.
    pub fn secs_to_next(&self) -> Option<u64> {
        if self.checking {
            return None;
        }
        let now = Instant::now();
        self.cfg
            .sites
            .iter()
            .filter(|s| s.has_enabled_accounts() && !s.paused)
            .map(|s| self.next_check.get(&s.id).map_or(0, |t| t.saturating_duration_since(now).as_secs()))
            .min()
    }

    /// Applies an editor action to a site's checker or download argument draft.
    fn edit_args(&mut self, id: SiteId, action: Action, checker: bool) {
        if matches!(action, Action::Edit(Edit::Enter)) {
            return;
        }
        let Some(d) = self.drafts.get_mut(&id) else { return };
        let content = if checker { &mut d.checker } else { &mut d.download };
        content.perform(action);
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
    AutoDelete(String),
    Checker(Action),
    Download(Action),
    Paused(bool),
    AutoDownload(bool),
    Backfill(bool),
    MarkOnOpen(bool),
    Notify(bool),
    NotifyDesktop(bool),
    NotifyNtfy(bool),
    NtfyTopic(String),
    OpenM3u8(bool),
    M3u8Args(String),
    Player(String),
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
    M3u8Ready(String, Result<String, String>),
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
    SaveSite(SiteId),
    RevertSite(SiteId),
    DefaultsSite(SiteId),
    RemoveSite(SiteId),
    ConfirmRemoveSite(SiteId),
    CancelRemoveSite,
    Site(SiteId, SiteField),
    RemoveAccount(SiteId, String),
    PickAddSite(SiteId),
    AddName(String),
    AddAccount,
    BrowseOutput(SiteId),
    OutputChosen(SiteId, Option<PathBuf>),
    OpenOutputDir(SiteId),
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
        if let Some(name) = self.posts.pending_jump.take() {
            tasks.extend(self.posts_jump(&name));
        }
        if self.posts.pending_view.iter().any(|(_, at)| now >= *at) {
            let (due, wait): (Vec<_>, Vec<_>) = std::mem::take(&mut self.posts.pending_view).into_iter().partition(|(_, at)| now >= *at);
            self.posts.pending_view = wait;
            for (id, _) in due {
                self.posts.set_viewed(&id, true);
            }
            self.posts.save_lib();
        }
        if now >= self.posts.next_sweep {
            self.posts.next_sweep = now + SWEEP_EVERY;
            self.posts_sweep();
        }
        if !self.posts.checking {
            let due: Vec<PostSite> = self
                .posts
                .cfg
                .sites
                .iter()
                .filter(|s| s.has_enabled_accounts() && !s.paused && self.posts.next_check.get(&s.id).is_none_or(|t| now >= *t))
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
            let (site_name, backfill, auto) = (site.name.clone(), site.backfill, site.auto_download);
            if let Some(e) = &ar.error {
                if self.posts.errors.insert(key, e.clone()).as_ref() != Some(e) {
                    logs.push(format!("Posts: {site_name} @{}: {e}", ar.name));
                }
                continue;
            }
            self.posts.errors.remove(&key);
            let first = !acct.baselined;
            acct.baselined = true;
            let mut added = Vec::new();
            for mut post in ar.new_posts {
                if self.posts.posts.iter().any(|p| p.id == post.id) {
                    continue;
                }
                if auto && (!first || backfill) {
                    post.state = PostState::Queued;
                }
                added.push(post);
            }
            if added.is_empty() {
                continue;
            }
            logs.push(format!("Posts: {} new from @{} on {site_name}", added.len(), ar.name));
            if !first {
                if let Some(t) = self.posts_notify(ar.site_id, &ar.name, &added) {
                    tasks.push(t);
                }
            }
            self.posts.posts.extend(added);
        }
        let auto_sites: HashSet<SiteId> = self.posts.cfg.sites.iter().filter(|s| s.auto_download).map(|s| s.id).collect();
        for p in &mut self.posts.posts {
            if p.state == PostState::Failed && p.attempts < posts::MAX_ATTEMPTS && auto_sites.contains(&p.site_id) {
                p.state = PostState::Queued;
            }
        }
        self.posts.save_cfg();
        self.posts.save_lib();
        for l in logs {
            self.push_log(l);
        }
        Task::batch(tasks)
    }

    fn posts_notify(&self, site_id: SiteId, account: &str, added: &[Post]) -> Option<Task<Msg>> {
        let site = self.posts.cfg.site(site_id).filter(|s| s.notify)?;
        let targets = notify::Targets {
            desktop_timeout_secs: site.notify_desktop.then_some(DEFAULT_NOTIFY_TIMEOUT_SECS),
            ntfy_topic: (site.notify_ntfy && notify::topic_valid(&site.ntfy_topic)).then(|| site.ntfy_topic.trim().to_string()),
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
                for site in &mut self.posts.cfg.sites {
                    site.paused = v;
                }
                for d in self.posts.drafts.values_mut() {
                    d.paused = v;
                }
                self.posts.save_cfg();
            }
            PostsMsg::Play(id) => return self.posts_play(&id),
            PostsMsg::M3u8Ready(id, Ok(url)) => self.posts_open_stream(&id, &url),
            PostsMsg::M3u8Ready(_, Err(e)) => {
                self.posts.say(format!("Could not get the stream link: {e}"));
                self.push_log(format!("Posts: stream link failed: {e}"));
            }
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
            PostsMsg::AddSite(Preset::Blank) => {
                self.posts.drafts.entry(NEW_SITE).or_insert_with(|| SiteDraft::from_site(&PostSite::from_preset(Preset::Blank)));
            }
            PostsMsg::AddSite(preset) => {
                let site = PostSite::from_preset(preset);
                let name = site.name.clone();
                match self.posts.cfg.add_site(site) {
                    Some(id) => self.posts_site_added(id, name),
                    None => return self.posts_jump(&name).unwrap_or_else(Task::none),
                }
            }
            PostsMsg::SaveSite(id) => self.posts_save_site(id),
            PostsMsg::RevertSite(id) => {
                if let Some(site) = self.posts.cfg.site(id) {
                    let draft = SiteDraft::from_site(site);
                    self.posts.drafts.insert(id, draft);
                }
            }
            PostsMsg::DefaultsSite(id) => {
                let name = self.posts.drafts.get(&id).map(|d| d.name.clone()).unwrap_or_default();
                self.posts.drafts.insert(id, SiteDraft::from_defaults(&name));
            }
            PostsMsg::RemoveSite(NEW_SITE) => {
                self.posts.drafts.remove(&NEW_SITE);
            }
            PostsMsg::RemoveSite(id) => self.posts.confirm_remove = Some(id),
            PostsMsg::CancelRemoveSite => self.posts.confirm_remove = None,
            PostsMsg::ConfirmRemoveSite(id) => self.posts_remove_site(id),
            PostsMsg::Site(id, field) => self.posts_site_field(id, field),
            PostsMsg::RemoveAccount(id, name) => {
                if let Some(site) = self.posts.cfg.site_mut(id) {
                    site.accounts.retain(|a| a.name != name);
                }
                self.posts.errors.remove(&(id, name.clone()));
                let mine = |p: &Post| p.site_id == id && p.account.eq_ignore_ascii_case(&name);
                for p in self.posts.posts.iter().filter(|p| mine(p)) {
                    let _ = std::fs::remove_file(p.thumb_path());
                }
                self.posts.posts.retain(|p| !mine(p));
                self.posts.save_lib();
                if self.posts.account.as_ref() == Some(&(id, name)) {
                    self.posts.account = None;
                }
                self.posts.save_cfg();
            }
            PostsMsg::PickAddSite(id) => self.posts.add_site = Some(id),
            PostsMsg::AddName(v) => self.posts.add_name = v,
            PostsMsg::AddAccount => self.posts_add_account(),
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
                if let Some(d) = self.posts.drafts.get_mut(&id) {
                    d.output_dir = dir.to_string_lossy().into_owned();
                }
            }
            PostsMsg::OutputChosen(_, None) => {}
            PostsMsg::OpenOutputDir(id) => {
                let dir = self.posts_out_dir(id);
                let _ = std::fs::create_dir_all(&dir);
                let _ = open::that_detached(dir);
            }
        }
        Task::none()
    }

    /// Applies one edit to a site's unsaved draft.
    fn posts_site_field(&mut self, id: SiteId, field: SiteField) {
        match field {
            SiteField::Checker(a) => return self.posts.edit_args(id, a, true),
            SiteField::Download(a) => return self.posts.edit_args(id, a, false),
            _ => {}
        }
        let Some(d) = self.posts.drafts.get_mut(&id) else { return };
        match field {
            SiteField::Name(v) => d.name = v,
            SiteField::Interval(v) => d.interval = v,
            SiteField::Exe(v) => d.exe = v,
            SiteField::UrlTemplate(v) => d.url_template = v,
            SiteField::OutDir(v) => d.output_dir = v,
            SiteField::AutoDelete(v) => d.auto_delete = v,
            SiteField::Paused(v) => d.paused = v,
            SiteField::AutoDownload(v) => d.auto_download = v,
            SiteField::Backfill(v) => d.backfill = v,
            SiteField::MarkOnOpen(v) => d.mark_on_open = v,
            SiteField::Notify(v) => d.notify = v,
            SiteField::NotifyDesktop(v) => d.notify_desktop = v,
            SiteField::NotifyNtfy(v) => d.notify_ntfy = v,
            SiteField::NtfyTopic(v) => d.ntfy_topic = v,
            SiteField::OpenM3u8(v) => d.open_m3u8 = v,
            SiteField::M3u8Args(v) => d.m3u8 = v,
            SiteField::Player(v) => d.player = v,
            SiteField::Checker(_) | SiteField::Download(_) => {}
        }
    }

    /// Registers a newly added site's editor and first check, and shows the confirmation.
    fn posts_site_added(&mut self, id: SiteId, name: String) {
        if let Some(site) = self.posts.cfg.site(id) {
            self.posts.drafts.insert(id, SiteDraft::from_site(site));
        }
        self.posts.next_check.insert(id, Instant::now());
        self.posts.save_cfg();
        self.posts.pending_jump = Some(name.clone());
        self.posts.added = Some((name, now_secs()));
    }

    /// Scrolls the Posts site list to the named site, if it exists.
    fn posts_jump(&self, name: &str) -> Option<Task<Msg>> {
        let sites = &self.posts.cfg.sites;
        let idx = sites.iter().position(|s| s.name.eq_ignore_ascii_case(name))?;
        let y = if sites.len() > 1 { idx as f32 / (sites.len() - 1) as f32 } else { 0.0 };
        Some(iced::widget::operation::snap_to(super::posts_scroll_id(), iced::widget::scrollable::RelativeOffset { x: 0.0, y }))
    }

    /// Saves a site's draft: adds the unsaved blank site, or updates an existing one.
    fn posts_save_site(&mut self, id: SiteId) {
        let saved = self.posts.cfg.site(id).cloned().unwrap_or_default();
        let Some(site) = self.posts.drafts.get(&id).and_then(|d| d.build(&saved)) else { return };
        if self.posts.cfg.sites.iter().any(|s| s.id != id && s.name.trim().eq_ignore_ascii_case(&site.name)) {
            self.posts.say(format!("A site named {} already exists", site.name));
            return;
        }
        if id == NEW_SITE {
            let name = site.name.clone();
            if let Some(new_id) = self.posts.cfg.add_site(site) {
                self.posts.drafts.remove(&NEW_SITE);
                self.posts_site_added(new_id, name);
            }
            return;
        }
        if let Some(slot) = self.posts.cfg.site_mut(id) {
            *slot = site;
        }
        self.posts.next_sweep = Instant::now();
        self.posts.save_cfg();
        self.posts.say("Changes saved");
    }

    /// Adds the typed username or profile URL to the site chosen under the posts grid.
    fn posts_add_account(&mut self) {
        let Some(id) = self.posts.add_target() else { return };
        let account = self.posts.cfg.site(id).and_then(|s| s.account_from_input(&self.posts.add_name));
        let Some(a) = account else {
            self.posts.say("Enter a new username or profile URL");
            return;
        };
        let name = a.name.clone();
        if let Some(site) = self.posts.cfg.site_mut(id) {
            site.accounts.push(a);
        }
        self.posts.add_name.clear();
        self.posts.next_check.insert(id, Instant::now());
        self.posts.save_cfg();
        self.posts.say(format!("Added @{name}"));
    }

    /// Removes a site with its accounts, posts and thumbnails.
    fn posts_remove_site(&mut self, id: SiteId) {
        self.posts.confirm_remove = None;
        self.posts.cfg.sites.retain(|s| s.id != id);
        self.posts.drafts.remove(&id);
        if self.posts.add_site == Some(id) {
            self.posts.add_site = None;
        }
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

    /// Opens the downloaded file, else the stream or the post page, and marks it viewed.
    fn posts_play(&mut self, id: &str) -> Task<Msg> {
        let Some(post) = self.posts.posts.iter().find(|p| p.id == id).cloned() else { return Task::none() };
        let file = post.file.as_deref().filter(|f| post.state == PostState::Ready && Path::new(f).is_file());
        if file.is_none() {
            if let Some(site) = self.posts.cfg.site(post.site_id).filter(|s| s.open_m3u8).cloned() {
                self.posts.say("Getting the stream link…");
                let id = post.id.clone();
                return Task::perform(runner::resolve_m3u8(site, post.url.clone()), move |r| Msg::Posts(PostsMsg::M3u8Ready(id.clone(), r)));
            }
        }
        let result = match file {
            Some(f) => ops::open_file(Path::new(f)),
            None => open::that_detached(&post.url).map_err(|e| e.to_string()),
        };
        let mark = self.posts.cfg.site(post.site_id).is_none_or(|s| s.mark_on_open);
        match result {
            Ok(()) if mark => self.posts.pending_view.push((id.to_string(), Instant::now() + VIEW_DELAY)),
            Ok(()) => {}
            Err(e) => self.posts.say(format!("Could not open the post: {e}")),
        }
        Task::none()
    }

    /// Plays a resolved stream URL in the site's player and marks the post viewed.
    fn posts_open_stream(&mut self, id: &str, url: &str) {
        let Some(site_id) = self.posts.posts.iter().find(|p| p.id == id).map(|p| p.site_id) else { return };
        let site = self.posts.cfg.site(site_id);
        let player = site.map(|s| s.player_path.clone()).unwrap_or_default();
        let mark = site.is_none_or(|s| s.mark_on_open);
        match runner::play_stream(&player, url, id) {
            Ok(()) if mark => self.posts.pending_view.push((id.to_string(), Instant::now() + VIEW_DELAY)),
            Ok(()) => {}
            Err(e) => self.posts.say(format!("Could not play the stream: {e}")),
        }
    }

    /// Sends the downloaded file to the recycle bin and hides the post.
    fn posts_trash(&mut self, id: &str) -> Result<(), String> {
        let Some(post) = self.posts.posts.iter().find(|p| p.id == id).cloned() else { return Ok(()) };
        if let Some(f) = post.file.as_deref().filter(|f| Path::new(f).is_file()) {
            ops::delete(Path::new(f), DeleteMode::Trash).map_err(|e| e.to_string())?;
        }
        let _ = std::fs::remove_file(post.thumb_path());
        if let Some(p) = self.posts.post_mut(id) {
            p.state = PostState::Removed;
            p.file = None;
            p.has_thumb = false;
            p.viewed = true;
        }
        Ok(())
    }

    fn posts_delete(&mut self, id: &str) {
        match self.posts_trash(id) {
            Ok(()) => {
                self.posts.save_lib();
                self.posts.say("Moved to the recycle bin");
            }
            Err(e) => self.posts.say(format!("Could not delete: {e}")),
        }
    }

    /// Deletes unstarred posts older than their site's auto-delete age.
    fn posts_sweep(&mut self) {
        let now = now_secs();
        let limits: HashMap<SiteId, u64> =
            self.posts.cfg.sites.iter().filter_map(|s| s.auto_delete_days.map(|d| (s.id, d.saturating_mul(SECS_PER_DAY)))).collect();
        let old: Vec<String> = self
            .posts
            .posts
            .iter()
            .filter(|p| !p.starred && !matches!(p.state, PostState::Removed | PostState::Downloading))
            .filter(|p| limits.get(&p.site_id).is_some_and(|max| now.saturating_sub(p.found_at) >= *max))
            .map(|p| p.id.clone())
            .collect();
        let deleted = old.iter().filter(|id| self.posts_trash(id).is_ok()).count();
        if deleted > 0 {
            self.posts.save_lib();
            self.push_log(format!("Posts: auto-deleted {deleted} unstarred post(s)"));
        }
    }

    /// Label of the Posts switch on the Monitor tab, with the unviewed count.
    pub(super) fn posts_tab_label(&self) -> String {
        match self.posts.unviewed_total() {
            0 => "Posts".to_string(),
            n => format!("Posts ({n})"),
        }
    }
}
