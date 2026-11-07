//! The Posts monitor: schedules checks and downloads, keeps the library and tells front ends what changed.

use super::runner::{self, CycleReport, DownloadOutcome};
use super::store;
use super::{AccountError, Post, PostSite, PostState, PostsCommand, PostsConfig, PostsEvent, PostsMsg, PostsStatus, SiteId};
use crate::api::{now_secs, DEFAULT_NOTIFY_TIMEOUT_SECS};
use crate::filemgr::ops::{self, DeleteMode};
use crate::policy::{notify, uilog, PolicyEvent};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

/// Time to wait after launch before the first check.
const FIRST_CHECK_DELAY: Duration = Duration::from_secs(3);
/// Time between auto-delete sweeps.
const SWEEP_EVERY: Duration = Duration::from_secs(3600);
const SECS_PER_DAY: u64 = 86_400;
/// Time a post stays listed after it is opened.
const VIEW_DELAY: Duration = Duration::from_secs(2);
/// Downloads that run at the same time.
const MAX_DOWNLOADS: usize = 2;

type AccountKey = (SiteId, String);

pub struct PostsMonitor {
    cfg: PostsConfig,
    posts: Vec<Post>,
    events: UnboundedSender<PolicyEvent>,
    tx: UnboundedSender<PostsMsg>,
    global_dir: String,
    checking: bool,
    last_check: Option<u64>,
    next_check: HashMap<SiteId, Instant>,
    errors: HashMap<AccountKey, String>,
    /// Ids of X posts found to have no video; not saved.
    no_video: HashSet<String>,
    downloads: usize,
    next_sweep: Instant,
    pending_view: Vec<(String, Instant)>,
}

impl PostsMonitor {
    /// Loads the saved config and library; `global_dir` is the global output folder.
    pub(crate) fn new(events: UnboundedSender<PolicyEvent>, tx: UnboundedSender<PostsMsg>, global_dir: String) -> Self {
        let cfg = store::load_config();
        let soon = Instant::now() + FIRST_CHECK_DELAY;
        PostsMonitor {
            next_check: cfg.sites.iter().map(|s| (s.id, soon)).collect(),
            cfg,
            posts: store::load_library(),
            events,
            tx,
            global_dir,
            checking: false,
            last_check: None,
            errors: HashMap::new(),
            no_video: HashSet::new(),
            downloads: 0,
            next_sweep: soon,
            pending_view: Vec::new(),
        }
    }

    pub fn set_global_dir(&mut self, dir: &str) {
        self.global_dir = dir.to_string();
    }

    /// Sends the config, the whole library and the status.
    pub fn emit_snapshot(&self) {
        self.emit(PostsEvent::Config(self.cfg.clone()));
        self.emit(PostsEvent::Library(self.posts.clone()));
        self.emit_status();
    }

    /// Saves again after the person chose to start fresh over unreadable files.
    pub fn resume_saving(&self) {
        store::resume_saving();
        self.save_cfg();
        self.save_lib();
    }

    fn emit(&self, ev: PostsEvent) {
        let _ = self.events.send(PolicyEvent::Posts(ev));
    }

    fn log(&self, msg: String) {
        uilog::append(&msg);
        let _ = self.events.send(PolicyEvent::Log(msg));
    }

    fn notice(&self, msg: impl Into<String>) {
        self.emit(PostsEvent::Notice(msg.into()));
    }

    fn save_cfg(&self) {
        store::save_config(&self.cfg);
    }

    fn save_lib(&self) {
        store::save_library(&self.posts);
    }

    fn emit_config(&self) {
        self.emit(PostsEvent::Config(self.cfg.clone()));
    }

    /// Sends the posts with these ids as changed.
    fn emit_changed(&self, ids: &[String]) {
        let set: HashSet<&String> = ids.iter().collect();
        let changed: Vec<Post> = self.posts.iter().filter(|p| set.contains(&p.id)).cloned().collect();
        if !changed.is_empty() {
            self.emit(PostsEvent::Changed(changed));
        }
    }

    /// True when a site has accounts to check and is not paused.
    fn active(site: &PostSite) -> bool {
        site.has_enabled_accounts() && !site.paused
    }

    fn emit_status(&self) {
        let now = Instant::now();
        let wait = self
            .cfg
            .sites
            .iter()
            .filter(|s| Self::active(s))
            .map(|s| self.next_check.get(&s.id).map_or(0, |t| t.saturating_duration_since(now).as_secs()))
            .min();
        let mut errors: Vec<AccountError> =
            self.errors.iter().map(|((site_id, name), message)| AccountError { site_id: *site_id, name: name.clone(), message: message.clone() }).collect();
        errors.sort_by(|a, b| (a.site_id, &a.name).cmp(&(b.site_id, &b.name)));
        let next_check = if self.checking { None } else { wait.map(|w| now_secs() + w) };
        self.emit(PostsEvent::Status(PostsStatus { checking: self.checking, last_check: self.last_check, next_check, errors }));
    }

    fn post(&self, id: &str) -> Option<&Post> {
        self.posts.iter().find(|p| p.id == id)
    }

    fn post_mut(&mut self, id: &str) -> Option<&mut Post> {
        self.posts.iter_mut().find(|p| p.id == id)
    }

    /// Starts due checks, queued downloads and delayed view marks; called regularly.
    pub fn tick(&mut self) {
        let now = Instant::now();
        self.mark_due_views(now);
        if now >= self.next_sweep {
            self.next_sweep = now + SWEEP_EVERY;
            self.sweep();
        }
        if !self.checking {
            let due: Vec<PostSite> = self
                .cfg
                .sites
                .iter()
                .filter(|s| Self::active(s) && self.next_check.get(&s.id).is_none_or(|t| now >= *t))
                .cloned()
                .collect();
            if !due.is_empty() {
                self.start_check(due);
            }
        }
        while self.downloads < MAX_DOWNLOADS {
            let Some(next) = self.next_queued() else { break };
            self.start_download(next);
        }
    }

    fn start_check(&mut self, sites: Vec<PostSite>) {
        self.checking = true;
        let known: HashSet<String> = self.posts.iter().map(|p| p.id.clone()).collect();
        let (no_video, tx) = (self.no_video.clone(), self.tx.clone());
        tokio::spawn(async move {
            let _ = tx.send(PostsMsg::Checked(runner::check_cycle(sites, known, no_video).await));
        });
        self.emit_status();
    }

    /// The id of the newest queued post.
    fn next_queued(&self) -> Option<String> {
        self.posts.iter().filter(|p| p.state == PostState::Queued).max_by_key(|p| p.sort_time()).map(|p| p.id.clone())
    }

    fn start_download(&mut self, id: String) {
        let site = self.post(&id).and_then(|p| self.cfg.site(p.site_id)).cloned();
        let Some(post) = self.post_mut(&id) else { return };
        let Some(site) = site else {
            post.state = PostState::Failed;
            post.attempts = super::MAX_ATTEMPTS;
            post.error = "Its site was removed".into();
            self.save_lib();
            self.emit_changed(&[id]);
            return;
        };
        post.state = PostState::Downloading;
        let max_mb = if post.manual { None } else { site.auto_download_limit_mb };
        let post = post.clone();
        self.downloads += 1;
        self.emit(PostsEvent::Changed(vec![post.clone()]));
        let (tx, dir) = (self.tx.clone(), self.global_dir.clone());
        tokio::spawn(async move {
            let outcome = runner::download(site, post, dir, max_mb).await;
            let _ = tx.send(PostsMsg::Downloaded(id, outcome));
        });
    }

    /// Handles the result of background work.
    pub(crate) fn on_msg(&mut self, msg: PostsMsg) {
        match msg {
            PostsMsg::Checked(report) => self.on_checked(report),
            PostsMsg::Downloaded(id, outcome) => self.on_downloaded(&id, outcome),
            PostsMsg::Stream { id, result: Ok(url) } => self.open_stream(&id, &url),
            PostsMsg::Stream { result: Err(e), .. } => {
                self.notice(format!("Could not get the stream link: {e}"));
                self.log(format!("Posts: stream link failed: {e}"));
            }
            PostsMsg::Notified(lines) => lines.into_iter().for_each(|l| self.log(l)),
        }
        self.tick();
    }

    /// Merges a finished check into the library and queues what is new.
    fn on_checked(&mut self, report: CycleReport) {
        let now = Instant::now();
        let mut changed: Vec<String> = Vec::new();
        self.checking = false;
        self.last_check = Some(report.at);
        for sid in &report.site_ids {
            if let Some(site) = self.cfg.site(*sid) {
                self.next_check.insert(*sid, now + Duration::from_secs(site.interval_or_default()));
            }
        }
        for ar in report.accounts {
            self.no_video.extend(ar.no_video.iter().cloned());
            let key = (ar.site_id, ar.name.clone());
            let Some(site) = self.cfg.site_mut(ar.site_id) else { continue };
            let Some(acct) = site.accounts.iter_mut().find(|a| a.name == ar.name) else { continue };
            let (site_name, backfill, auto) = (site.name.clone(), site.backfill, site.auto_download);
            if let Some(e) = &ar.error {
                if self.errors.insert(key, e.clone()).as_ref() != Some(e) {
                    self.log(format!("Posts: {site_name} @{}: {e}", ar.name));
                }
                continue;
            }
            self.errors.remove(&key);
            let first = !acct.baselined;
            acct.baselined = true;
            let mut added = Vec::new();
            for mut post in ar.new_posts {
                if self.posts.iter().any(|p| p.id == post.id) {
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
            self.log(format!("Posts: {} new from @{} on {site_name}", added.len(), ar.name));
            if !first {
                self.notify_new(ar.site_id, &ar.name, &added);
            }
            changed.extend(added.iter().map(|p| p.id.clone()));
            self.posts.extend(added);
        }
        let auto_sites: HashSet<SiteId> = self.cfg.sites.iter().filter(|s| s.auto_download).map(|s| s.id).collect();
        for p in &mut self.posts {
            if p.state == PostState::Failed && p.attempts < super::MAX_ATTEMPTS && auto_sites.contains(&p.site_id) {
                p.state = PostState::Queued;
                changed.push(p.id.clone());
            }
        }
        self.save_cfg();
        self.save_lib();
        self.emit_config();
        self.emit_changed(&changed);
        self.emit_status();
    }

    /// Sends the desktop and ntfy notifications the site asks for.
    fn notify_new(&self, site_id: SiteId, account: &str, added: &[Post]) {
        let Some(site) = self.cfg.site(site_id).filter(|s| s.notify) else { return };
        let targets = notify::Targets {
            desktop_timeout_secs: site.notify_desktop.then_some(DEFAULT_NOTIFY_TIMEOUT_SECS),
            ntfy_topic: (site.notify_ntfy && notify::topic_valid(&site.ntfy_topic)).then(|| site.ntfy_topic.trim().to_string()),
        };
        if targets.is_empty() {
            return;
        }
        let msg = super::new_posts_message(account, added);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(PostsMsg::Notified(runner::send_notification(msg, targets).await));
        });
    }

    fn on_downloaded(&mut self, id: &str, out: DownloadOutcome) {
        self.downloads = self.downloads.saturating_sub(1);
        let mut log = None;
        if let Some(p) = self.post_mut(id) {
            p.has_thumb = out.has_thumb;
            p.manual = false;
            if p.state != PostState::Removed {
                match out.file {
                    Err(e) if out.too_large => {
                        p.state = PostState::Remote;
                        log = Some(format!("Posts: {} skipped: {e}", p.id));
                        p.error = e;
                    }
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
        }
        self.save_lib();
        self.emit_changed(&[id.to_string()]);
        if let Some(l) = log {
            self.log(l);
        }
    }

    /// Applies a command from a front end.
    pub fn handle(&mut self, cmd: PostsCommand) {
        match cmd {
            PostsCommand::CheckNow => {
                let now = Instant::now();
                for site in &self.cfg.sites {
                    self.next_check.insert(site.id, now);
                }
                self.emit_status();
            }
            PostsCommand::SetPaused(paused) => {
                for site in &mut self.cfg.sites {
                    site.paused = paused;
                }
                self.save_cfg();
                self.emit_config();
                self.emit_status();
            }
            PostsCommand::Play(id) => self.play(&id),
            PostsCommand::OpenPage(id) => {
                if let Some(url) = self.post(&id).map(|p| p.url.clone()) {
                    let _ = open::that_detached(url);
                }
            }
            PostsCommand::Reveal(id) => {
                let file = self.post(&id).and_then(|p| p.file.clone());
                if let Some(Err(e)) = file.map(|f| ops::reveal(Path::new(&f))) {
                    self.notice(format!("Could not open the folder: {e}"));
                }
            }
            PostsCommand::Delete(id) => match self.trash(&id) {
                Ok(()) => {
                    self.save_lib();
                    self.emit_changed(&[id]);
                    self.notice("Moved to the recycle bin");
                }
                Err(e) => self.notice(format!("Could not delete: {e}")),
            },
            PostsCommand::Download(id) => {
                if let Some(p) = self.post_mut(&id).filter(|p| matches!(p.state, PostState::Remote | PostState::Failed)) {
                    p.state = PostState::Queued;
                    p.attempts = 0;
                    p.manual = true;
                    p.error.clear();
                }
                self.save_lib();
                self.emit_changed(&[id]);
            }
            PostsCommand::SetViewed { ids, viewed } => {
                for p in self.posts.iter_mut().filter(|p| ids.contains(&p.id)) {
                    p.viewed = viewed;
                    p.viewed_at = viewed.then(now_secs);
                }
                self.save_lib();
                self.emit_changed(&ids);
            }
            PostsCommand::SetStarred { id, starred } => {
                if let Some(p) = self.post_mut(&id) {
                    p.starred = starred;
                }
                self.save_lib();
                self.emit_changed(&[id]);
            }
            PostsCommand::AddSite(site) => self.add_site(site),
            PostsCommand::UpdateSite(site) => self.update_site(site),
            PostsCommand::RemoveSite(id) => self.remove_site(id),
            PostsCommand::AddAccount { site_id, input } => self.add_account(site_id, &input),
            PostsCommand::RemoveAccount { site_id, name } => self.remove_account(site_id, &name),
            PostsCommand::OpenOutputDir(id) => {
                if let Some(site) = self.cfg.site(id) {
                    let dir = site.output_folder(&self.global_dir);
                    let _ = std::fs::create_dir_all(&dir);
                    let _ = open::that_detached(dir);
                }
            }
        }
        self.tick();
    }

    fn add_site(&mut self, mut site: PostSite) {
        site.accounts.clear();
        let name = site.name.clone();
        match self.cfg.add_site(site) {
            Some(id) => {
                self.next_check.insert(id, Instant::now());
                self.save_cfg();
                self.emit_config();
                self.emit(PostsEvent::SiteAdded { id, name });
                self.emit_status();
            }
            None => self.notice(format!("A site named {name} already exists")),
        }
    }

    fn update_site(&mut self, site: PostSite) {
        let taken = self.cfg.sites.iter().any(|s| s.id != site.id && s.name.trim().eq_ignore_ascii_case(site.name.trim()));
        if taken {
            self.notice(format!("A site named {} already exists", site.name));
            return;
        }
        let Some(slot) = self.cfg.site_mut(site.id) else { return };
        let accounts = std::mem::take(&mut slot.accounts);
        *slot = PostSite { accounts, ..site };
        self.next_sweep = Instant::now();
        self.save_cfg();
        self.emit_config();
        self.emit_status();
        self.notice("Changes saved");
    }

    fn remove_site(&mut self, id: SiteId) {
        self.cfg.sites.retain(|s| s.id != id);
        self.next_check.remove(&id);
        self.errors.retain(|(sid, _), _| *sid != id);
        let removed = self.take_posts(|p| p.site_id == id);
        self.save_cfg();
        self.save_lib();
        self.emit_config();
        self.emit(PostsEvent::Removed(removed));
        self.emit_status();
    }

    fn add_account(&mut self, site_id: SiteId, input: &str) {
        let Some(account) = self.cfg.site(site_id).and_then(|s| s.account_from_input(input)) else {
            self.notice("Enter a new username or profile URL");
            return;
        };
        let name = account.name.clone();
        if let Some(site) = self.cfg.site_mut(site_id) {
            site.accounts.push(account);
        }
        self.next_check.insert(site_id, Instant::now());
        self.save_cfg();
        self.emit_config();
        self.emit_status();
        self.notice(format!("Added @{name}"));
    }

    fn remove_account(&mut self, site_id: SiteId, name: &str) {
        if let Some(site) = self.cfg.site_mut(site_id) {
            site.accounts.retain(|a| a.name != name);
        }
        self.errors.remove(&(site_id, name.to_string()));
        let removed = self.take_posts(|p| p.site_id == site_id && p.account.eq_ignore_ascii_case(name));
        self.save_cfg();
        self.save_lib();
        self.emit_config();
        self.emit(PostsEvent::Removed(removed));
        self.emit_status();
    }

    /// Removes the matching posts and their thumbnails; returns their ids.
    fn take_posts(&mut self, matches: impl Fn(&Post) -> bool) -> Vec<String> {
        for p in self.posts.iter().filter(|p| matches(p)) {
            let _ = std::fs::remove_file(p.thumb_path());
        }
        let ids = self.posts.iter().filter(|p| matches(p)).map(|p| p.id.clone()).collect();
        self.posts.retain(|p| !matches(p));
        ids
    }

    /// Opens the downloaded file, else the stream or the post page, and marks it viewed.
    fn play(&mut self, id: &str) {
        let Some(post) = self.post(id).cloned() else { return };
        let file = post.file.as_deref().filter(|f| post.state == PostState::Ready && Path::new(f).is_file());
        if file.is_none() {
            if let Some(site) = self.cfg.site(post.site_id).filter(|s| s.open_m3u8).cloned() {
                self.notice("Getting the stream link…");
                let (tx, url, id) = (self.tx.clone(), post.url.clone(), id.to_string());
                tokio::spawn(async move {
                    let _ = tx.send(PostsMsg::Stream { id, result: runner::resolve_m3u8(site, url).await });
                });
                return;
            }
        }
        let result = match file {
            Some(f) => ops::open_file(Path::new(f)),
            None => open::that_detached(&post.url).map_err(|e| e.to_string()),
        };
        self.after_open(id, post.site_id, result.map_err(|e| format!("Could not open the post: {e}")));
    }

    /// Plays a resolved stream URL in the site's player.
    fn open_stream(&mut self, id: &str, url: &str) {
        let Some(site_id) = self.post(id).map(|p| p.site_id) else { return };
        let player = self.cfg.site(site_id).map(|s| s.player_path.clone()).unwrap_or_default();
        let result = runner::play_stream(&player, url, id).map_err(|e| format!("Could not play the stream: {e}"));
        self.after_open(id, site_id, result);
    }

    /// Schedules the view mark after a successful open, or reports the failure.
    fn after_open(&mut self, id: &str, site_id: SiteId, result: Result<(), String>) {
        match result {
            Ok(()) if self.cfg.site(site_id).is_none_or(|s| s.mark_on_open) => {
                self.pending_view.push((id.to_string(), Instant::now() + VIEW_DELAY));
            }
            Ok(()) => {}
            Err(e) => self.notice(e),
        }
    }

    /// Marks the opened posts whose delay has passed as viewed.
    fn mark_due_views(&mut self, now: Instant) {
        if !self.pending_view.iter().any(|(_, at)| now >= *at) {
            return;
        }
        let (due, wait): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending_view).into_iter().partition(|(_, at)| now >= *at);
        self.pending_view = wait;
        let ids: Vec<String> = due.into_iter().map(|(id, _)| id).collect();
        for p in self.posts.iter_mut().filter(|p| ids.contains(&p.id)) {
            p.viewed = true;
            p.viewed_at = Some(now_secs());
        }
        self.save_lib();
        self.emit_changed(&ids);
    }

    /// Sends the downloaded file to the recycle bin and hides the post.
    fn trash(&mut self, id: &str) -> Result<(), String> {
        let Some(post) = self.post(id).cloned() else { return Ok(()) };
        if let Some(f) = post.file.as_deref().filter(|f| Path::new(f).is_file()) {
            ops::delete(Path::new(f), DeleteMode::Trash).map_err(|e| e.to_string())?;
        }
        let _ = std::fs::remove_file(post.thumb_path());
        if let Some(p) = self.post_mut(id) {
            p.state = PostState::Removed;
            p.file = None;
            p.has_thumb = false;
            p.viewed = true;
        }
        Ok(())
    }

    /// Deletes unstarred posts older than their site's auto-delete age.
    fn sweep(&mut self) {
        let now = now_secs();
        let limits: HashMap<SiteId, u64> =
            self.cfg.sites.iter().filter_map(|s| s.auto_delete_days.map(|d| (s.id, d.saturating_mul(SECS_PER_DAY)))).collect();
        let old: Vec<String> = self
            .posts
            .iter()
            .filter(|p| !p.starred && !matches!(p.state, PostState::Removed | PostState::Downloading))
            .filter(|p| limits.get(&p.site_id).is_some_and(|max| now.saturating_sub(p.found_at) >= *max))
            .map(|p| p.id.clone())
            .collect();
        let deleted: Vec<String> = old.into_iter().filter(|id| self.trash(id).is_ok()).collect();
        if !deleted.is_empty() {
            self.save_lib();
            self.emit_changed(&deleted);
            self.log(format!("Posts: auto-deleted {} unstarred post(s)", deleted.len()));
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::testutil::Sandbox;
    use tokio::sync::mpsc::{self, UnboundedReceiver};

    fn monitor() -> (PostsMonitor, UnboundedReceiver<PolicyEvent>) {
        let (ev_tx, ev_rx) = mpsc::unbounded_channel();
        let (tx, _rx) = mpsc::unbounded_channel();
        (PostsMonitor::new(ev_tx, tx, "out".into()), ev_rx)
    }

    fn drain(rx: &mut UnboundedReceiver<PolicyEvent>) -> Vec<PostsEvent> {
        let mut out = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            if let PolicyEvent::Posts(e) = ev {
                out.push(e);
            }
        }
        out
    }

    fn paused_site(name: &str) -> PostSite {
        PostSite { name: name.into(), url_template: "https://x.test/@{username}".into(), paused: true, ..Default::default() }
    }

    #[test]
    fn adding_a_site_reports_it_and_rejects_duplicates() {
        let _sb = Sandbox::new("posts-add");
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let (mut m, mut rx) = monitor();
        drain(&mut rx);
        m.handle(PostsCommand::AddSite(paused_site("Test")));
        let evs = drain(&mut rx);
        assert!(evs.iter().any(|e| matches!(e, PostsEvent::SiteAdded { name, .. } if name == "Test")));
        assert!(evs.iter().any(|e| matches!(e, PostsEvent::Config(c) if c.name_taken("test"))));
        m.handle(PostsCommand::AddSite(paused_site("test")));
        assert!(drain(&mut rx).iter().any(|e| matches!(e, PostsEvent::Notice(n) if n.contains("already exists"))));
    }

    #[test]
    fn viewed_marks_and_account_removal_are_reported() {
        let _sb = Sandbox::new("posts-viewed");
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let (mut m, mut rx) = monitor();
        m.handle(PostsCommand::AddSite(paused_site("Test")));
        let site_id = m.cfg.sites.last().unwrap().id;
        m.handle(PostsCommand::AddAccount { site_id, input: "someone".into() });
        m.posts.push(Post { id: "p1".into(), site_id, account: "someone".into(), ..Default::default() });
        drain(&mut rx);
        m.handle(PostsCommand::SetViewed { ids: vec!["p1".into()], viewed: true });
        let evs = drain(&mut rx);
        assert!(evs.iter().any(|e| matches!(e, PostsEvent::Changed(p) if p[0].viewed && p[0].viewed_at.is_some())));
        m.handle(PostsCommand::RemoveAccount { site_id, name: "someone".into() });
        let evs = drain(&mut rx);
        assert!(evs.iter().any(|e| matches!(e, PostsEvent::Removed(ids) if ids == &["p1".to_string()])));
        assert!(m.posts.is_empty());
    }

    #[test]
    fn updating_a_site_keeps_its_accounts() {
        let _sb = Sandbox::new("posts-update");
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let _guard = rt.enter();
        let (mut m, mut rx) = monitor();
        m.handle(PostsCommand::AddSite(paused_site("Test")));
        let site_id = m.cfg.sites.last().unwrap().id;
        m.handle(PostsCommand::AddAccount { site_id, input: "someone".into() });
        drain(&mut rx);
        let edited = PostSite { id: site_id, name: "Renamed".into(), paused: true, ..Default::default() };
        m.handle(PostsCommand::UpdateSite(edited));
        let site = m.cfg.site(site_id).unwrap();
        assert_eq!((site.name.as_str(), site.accounts.len()), ("Renamed", 1));
    }
}
