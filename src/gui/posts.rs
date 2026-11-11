//! Posts monitor front end: the library view, filters and site drafts; the monitor itself runs in the policy layer.

use super::{prefs, App, Msg};
use crate::api::now_secs;
use crate::policy::posts::{Post, PostSite, PostState, PostsCommand, PostsConfig, PostsEvent, Preset, SiteId, MIN_INTERVAL_SECS};
use crate::policy::PolicyCommand;
use iced::widget::text_editor::{Action, Content, Edit};
use iced::Task;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Cards shown before "Show more" is pressed, and added by each press.
pub const PAGE: usize = 40;
const STATUS_SECS: u64 = 5;
/// Draft key for the unsaved blank site; real site ids start at 1.
pub const NEW_SITE: SiteId = 0;

/// Identifies an account: its site and name.
pub type AccountKey = (SiteId, String);

/// What the add-account dropdown can point at: an existing site, or a preset that has no site yet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AddTarget {
    Site(SiteId),
    Template(Preset),
}

/// One entry of the add-account dropdown.
#[derive(Debug, Clone, PartialEq)]
pub struct AddOption {
    pub target: AddTarget,
    pub name: String,
}

impl std::fmt::Display for AddOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

/// Which slice of the library the main area shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Unviewed,
    Viewed,
    Starred,
    Failed,
}

impl Section {
    pub fn label(self) -> &'static str {
        match self {
            Section::Unviewed => "Unviewed",
            Section::Viewed => "Viewed",
            Section::Starred => "Starred",
            Section::Failed => "Failed",
        }
    }
}

fn in_section(p: &Post, section: Section) -> bool {
    if p.state == PostState::Removed {
        return false;
    }
    match section {
        Section::Unviewed => !p.viewed,
        Section::Viewed => p.viewed,
        Section::Starred => p.starred,
        Section::Failed => p.state == PostState::Failed,
    }
}

/// True when the lowercase `query` is blank or appears in the title or account.
fn matches_query(p: &Post, query: &str) -> bool {
    query.is_empty() || p.title.to_lowercase().contains(query) || p.account.to_lowercase().contains(query)
}

/// Unsaved edits to one site's settings card.
pub struct SiteDraft {
    pub name: String,
    pub interval: String,
    pub exe: String,
    pub url_template: String,
    pub output_dir: String,
    pub auto_delete: String,
    pub auto_limit: String,
    pub auto_secs: String,
    pub checker: Content,
    pub download: Content,
    pub x_scraper: bool,
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
            auto_limit: site.auto_download_limit_mb.map(|n| n.to_string()).unwrap_or_default(),
            auto_secs: site.auto_download_secs.map(|n| n.to_string()).unwrap_or_default(),
            checker: Content::with_text(&site.checker_args),
            download: Content::with_text(&site.download_args),
            x_scraper: site.use_x_scraper,
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
        SiteDraft::from_site(&PostSite { name: name.to_string(), ..PostSite::from_preset(Preset::from_name(name)) })
    }

    /// `base` with these edits applied; `None` while a field is invalid.
    pub fn build(&self, base: &PostSite) -> Option<PostSite> {
        let number = |text: &str, min: u64| match text.trim() {
            "" => Some(None),
            t => t.parse::<u64>().ok().filter(|n| *n >= min).map(Some),
        };
        if self.name.trim().is_empty() || self.url_template.trim().is_empty() {
            return None;
        }
        Some(PostSite {
            name: self.name.trim().to_string(),
            interval_secs: number(&self.interval, MIN_INTERVAL_SECS)?,
            exe_path: self.exe.clone(),
            url_template: self.url_template.clone(),
            output_dir: self.output_dir.clone(),
            auto_delete_days: number(&self.auto_delete, 1)?,
            auto_download_limit_mb: number(&self.auto_limit, 1)?,
            auto_download_secs: number(&self.auto_secs, 1)?,
            checker_args: self.checker.text(),
            download_args: self.download.text(),
            use_x_scraper: self.x_scraper,
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

    /// False while notifications and ntfy are on without a usable topic, own or `global`.
    pub fn ntfy_ok(&self, global: &str) -> bool {
        !(self.notify && self.notify_ntfy) || crate::policy::notify::topic_ok(&self.ntfy_topic, global)
    }

    /// True when the edits are valid, or the site is unsaved.
    pub fn valid(&self, global: &str) -> bool {
        self.ntfy_ok(global) && self.build(&PostSite::default()).is_some()
    }

    /// True when the edits differ from the saved site.
    pub fn differs_from(&self, saved: &PostSite) -> bool {
        self.build(saved).is_none_or(|b| b != *saved)
    }
}

pub struct PostsState {
    /// The sites and accounts as last reported by the policy layer.
    pub cfg: PostsConfig,
    /// The library as last reported by the policy layer.
    pub posts: Vec<Post>,
    pub section: Section,
    pub account: Option<AccountKey>,
    pub search: String,
    pub shown: usize,
    pub drafts: HashMap<SiteId, SiteDraft>,
    pub add_site: Option<AddTarget>,
    pub add_name: String,
    pub settings_site: Option<AddTarget>,
    pub errors: HashMap<AccountKey, String>,
    pub checking: HashSet<SiteId>,
    next_checks: HashMap<SiteId, u64>,
    pub newest_first: bool,
    pub card_w: f32,
    status: Option<(String, u64)>,
}

impl PostsState {
    pub fn new() -> Self {
        let prefs = prefs::load_posts();
        PostsState {
            cfg: PostsConfig::default(),
            posts: Vec::new(),
            section: Section::Unviewed,
            account: None,
            search: String::new(),
            shown: PAGE,
            drafts: HashMap::new(),
            add_site: None,
            add_name: String::new(),
            settings_site: None,
            errors: HashMap::new(),
            checking: HashSet::new(),
            next_checks: HashMap::new(),
            newest_first: prefs.newest_first,
            card_w: prefs.card_w,
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

    fn account_ok(&self, p: &Post) -> bool {
        self.account.as_ref().is_none_or(|(sid, name)| p.site_id == *sid && p.account.eq_ignore_ascii_case(name))
    }

    /// Posts in the current section, account and search, in display order.
    pub fn visible(&self) -> Vec<&Post> {
        let q = self.search.trim().to_lowercase();
        let mut v: Vec<&Post> = self.posts.iter().filter(|p| in_section(p, self.section) && self.account_ok(p) && matches_query(p, &q)).collect();
        v.sort_by_key(|p| p.sort_time());
        if self.newest_first {
            v.reverse();
        }
        v
    }

    /// Posts in `section`, for the account when one is given.
    pub fn count(&self, section: Section, account: Option<(SiteId, &str)>) -> usize {
        self.posts
            .iter()
            .filter(|p| in_section(p, section) && account.is_none_or(|(sid, n)| p.site_id == sid && p.account.eq_ignore_ascii_case(n)))
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

    /// The dropdown entries: each named preset (its site once created), then custom sites.
    pub fn add_options(&self) -> Vec<AddOption> {
        let mut out: Vec<AddOption> = Vec::new();
        for preset in Preset::NAMED {
            let name = preset.label();
            let found = self.cfg.sites.iter().find(|s| s.name.trim().eq_ignore_ascii_case(name));
            out.push(match found {
                Some(site) => AddOption { target: AddTarget::Site(site.id), name: site.name.clone() },
                None => AddOption { target: AddTarget::Template(preset), name: name.to_string() },
            });
        }
        for site in &self.cfg.sites {
            if !out.iter().any(|o| o.target == AddTarget::Site(site.id)) {
                out.push(AddOption { target: AddTarget::Site(site.id), name: site.name.clone() });
            }
        }
        out
    }

    /// The dropdown entry the username box adds to: the picked one, else the first.
    pub fn add_choice(&self) -> Option<AddOption> {
        let opts = self.add_options();
        let picked = self.add_site.and_then(|t| match t {
            AddTarget::Site(_) => opts.iter().find(|o| o.target == t),
            AddTarget::Template(p) => opts.iter().find(|o| o.target == t || o.name.trim().eq_ignore_ascii_case(p.label())),
        });
        let choice = picked.or_else(|| opts.first()).cloned();
        choice
    }

    /// The Settings > Posts dropdown entries: the add-account entries, then New site.
    pub fn settings_options(&self) -> Vec<AddOption> {
        let mut out = self.add_options();
        out.push(AddOption { target: AddTarget::Template(Preset::Blank), name: "New site".to_string() });
        out
    }

    /// The dropdown entry shown in Settings > Posts: the picked one, else the first.
    pub fn settings_choice(&self, opts: &[AddOption]) -> Option<AddOption> {
        let picked = self.settings_site.and_then(|t| match t {
            AddTarget::Site(_) => opts.iter().find(|o| o.target == t),
            AddTarget::Template(Preset::Blank) => opts.iter().find(|o| o.target == t),
            AddTarget::Template(p) => opts.iter().find(|o| o.target == t || o.name.trim().eq_ignore_ascii_case(p.label())),
        });
        picked.or_else(|| opts.iter().find(|o| matches!(o.target, AddTarget::Site(_)))).cloned()
    }

    /// The site's check countdown for the sidebar: paused, checking or seconds left.
    pub fn site_countdown(&self, site: &PostSite) -> Option<String> {
        if site.paused {
            return Some("paused".to_string());
        }
        if self.checking.contains(&site.id) {
            return Some("checking…".to_string());
        }
        self.next_checks.get(&site.id).map(|at| format!("{}s", at.saturating_sub(now_secs())))
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

    /// Back to the Unviewed section with all accounts.
    pub fn reset_filters(&mut self) {
        self.section = Section::Unviewed;
        self.account = None;
        self.reset_view();
    }

    fn reset_view(&mut self) {
        self.shown = PAGE;
    }

    /// Applies a state change reported by the policy layer.
    fn apply(&mut self, ev: PostsEvent) {
        match ev {
            PostsEvent::Config(cfg) => {
                for site in &cfg.sites {
                    self.drafts.entry(site.id).or_insert_with(|| SiteDraft::from_site(site));
                }
                self.drafts.retain(|id, _| *id == NEW_SITE || cfg.site(*id).is_some());
                self.cfg = cfg;
            }
            PostsEvent::Library(posts) => self.posts = posts,
            PostsEvent::Changed(changed) => {
                for post in changed {
                    match self.posts.iter_mut().find(|p| p.id == post.id) {
                        Some(slot) => *slot = post,
                        None => self.posts.push(post),
                    }
                }
            }
            PostsEvent::Removed(ids) => self.posts.retain(|p| !ids.contains(&p.id)),
            PostsEvent::Status(st) => {
                self.checking = st.checking.into_iter().collect();
                self.next_checks = st.next_checks.into_iter().collect();
                self.errors = st.errors.into_iter().map(|e| ((e.site_id, e.name), e.message)).collect();
            }
            PostsEvent::Notice(msg) => self.say(msg),
            PostsEvent::SiteAdded { id, name } => {
                let unsaved = self.drafts.get(&NEW_SITE).is_some_and(|d| d.name.trim().eq_ignore_ascii_case(name.trim()));
                if unsaved {
                    self.drafts.remove(&NEW_SITE);
                }
                if let Some(site) = self.cfg.site(id) {
                    self.drafts.entry(id).or_insert_with(|| SiteDraft::from_site(site));
                }
                let wanted = matches!(self.settings_site, Some(AddTarget::Template(p)) if p.label().eq_ignore_ascii_case(name.trim()));
                if unsaved || wanted {
                    self.settings_site = Some(AddTarget::Site(id));
                }
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
    AutoLimit(String),
    AutoSecs(String),
    Checker(Action),
    Download(Action),
    XScraper(bool),
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
    CancelDownload(String),
    Reveal(String),
    Delete(String),
    MarkAllViewed,
    NewestFirst(bool),
    CardWidth(f32),
    CardWidthSave,
    PickSettingsSite(AddTarget),
    SaveSite(SiteId),
    RevertSite(SiteId),
    DefaultsSite(SiteId),
    DiscardSite,
    Site(SiteId, SiteField),
    RemoveAccount(SiteId, String),
    PickAddSite(AddTarget),
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

    fn posts_send(&self, cmd: PostsCommand) {
        self.client.send(PolicyCommand::Posts(cmd));
    }

    /// Applies a state change reported by the policy layer.
    pub(super) fn posts_on_event(&mut self, ev: PostsEvent) {
        if matches!(ev, PostsEvent::Config(_)) {
            self.disks_dirty = true;
        }
        self.posts.apply(ev);
    }

    pub(super) fn posts_update(&mut self, m: PostsMsg) -> Task<Msg> {
        match m {
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
            PostsMsg::CheckNow => self.posts_send(PostsCommand::CheckNow),
            PostsMsg::Paused(v) => {
                for d in self.posts.drafts.values_mut() {
                    d.paused = v;
                }
                self.posts_send(PostsCommand::SetPaused(v));
            }
            PostsMsg::Play(id) => self.posts_send(PostsCommand::Play(id)),
            PostsMsg::OpenPage(id) => self.posts_send(PostsCommand::OpenPage(id)),
            PostsMsg::ToggleViewed(id) => {
                let viewed = self.posts.posts.iter().find(|p| p.id == id).is_some_and(|p| p.viewed);
                self.posts_send(PostsCommand::SetViewed { ids: vec![id], viewed: !viewed });
            }
            PostsMsg::ToggleStar(id) => {
                let starred = self.posts.posts.iter().find(|p| p.id == id).is_some_and(|p| p.starred);
                self.posts_send(PostsCommand::SetStarred { id, starred: !starred });
            }
            PostsMsg::Download(id) => self.posts_send(PostsCommand::Download(id)),
            PostsMsg::CancelDownload(id) => self.posts_send(PostsCommand::CancelDownload(id)),
            PostsMsg::Reveal(id) => self.posts_send(PostsCommand::Reveal(id)),
            PostsMsg::Delete(id) => self.posts_send(PostsCommand::Delete(id)),
            PostsMsg::MarkAllViewed => {
                let ids: Vec<String> = self.posts.visible().iter().map(|p| p.id.clone()).collect();
                self.posts.say(format!("Marked {} as viewed", ids.len()));
                self.posts_send(PostsCommand::SetViewed { ids, viewed: true });
            }
            PostsMsg::NewestFirst(v) => {
                self.posts.newest_first = v;
                prefs::save_posts(&prefs::PostsPrefs { newest_first: v, card_w: self.posts.card_w });
            }
            PostsMsg::CardWidth(v) => {
                self.posts.card_w = ((v / 10.0).round() * 10.0).clamp(prefs::CARD_W_MIN, prefs::CARD_W_MAX);
            }
            PostsMsg::CardWidthSave => {
                prefs::save_posts(&prefs::PostsPrefs { newest_first: self.posts.newest_first, card_w: self.posts.card_w });
            }
            PostsMsg::PickSettingsSite(AddTarget::Template(Preset::Blank)) => {
                self.posts.drafts.entry(NEW_SITE).or_insert_with(|| SiteDraft::from_site(&PostSite::from_preset(Preset::Blank)));
                self.posts.settings_site = Some(AddTarget::Template(Preset::Blank));
            }
            PostsMsg::PickSettingsSite(AddTarget::Template(preset)) => {
                let site = PostSite::from_preset(preset);
                if let Some(found) = self.posts.cfg.sites.iter().find(|s| s.name.trim().eq_ignore_ascii_case(site.name.trim())) {
                    self.posts.settings_site = Some(AddTarget::Site(found.id));
                } else {
                    self.posts.settings_site = Some(AddTarget::Template(preset));
                    self.posts_send(PostsCommand::AddSite(site));
                }
            }
            PostsMsg::PickSettingsSite(target) => self.posts.settings_site = Some(target),
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
            PostsMsg::DiscardSite => {
                self.posts.drafts.remove(&NEW_SITE);
                self.posts.settings_site = None;
            }
            PostsMsg::Site(id, field) => self.posts_site_field(id, field),
            PostsMsg::RemoveAccount(id, name) => {
                if self.posts.account.as_ref() == Some(&(id, name.clone())) {
                    self.posts.account = None;
                }
                self.posts_send(PostsCommand::RemoveAccount { site_id: id, name });
            }
            PostsMsg::PickAddSite(target) => self.posts.add_site = Some(target),
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
            PostsMsg::OpenOutputDir(id) => self.posts_send(PostsCommand::OpenOutputDir(id)),
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
            SiteField::AutoLimit(v) => d.auto_limit = v,
            SiteField::AutoSecs(v) => d.auto_secs = v,
            SiteField::XScraper(v) => d.x_scraper = v,
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

    /// Sends a site's draft to the policy layer: adds the unsaved blank site, or updates an existing one.
    fn posts_save_site(&mut self, id: SiteId) {
        let saved = self.posts.cfg.site(id).cloned().unwrap_or_default();
        let topic = self.settings.notifications.ntfy_topic.as_str();
        let Some(site) = self.posts.drafts.get(&id).filter(|d| d.ntfy_ok(topic)).and_then(|d| d.build(&saved)) else { return };
        if self.posts.cfg.sites.iter().any(|s| s.id != id && s.name.trim().eq_ignore_ascii_case(&site.name)) {
            self.posts.say(format!("A site named {} already exists", site.name));
            return;
        }
        let cmd = if id == NEW_SITE { PostsCommand::AddSite(site) } else { PostsCommand::UpdateSite(site) };
        self.posts_send(cmd);
    }

    /// Sends the typed username or profile URL for the site chosen under the posts grid.
    fn posts_add_account(&mut self) {
        let Some(choice) = self.posts.add_choice() else { return };
        let valid = match choice.target {
            AddTarget::Site(id) => self.posts.cfg.site(id).and_then(|s| s.account_from_input(&self.posts.add_name)).is_some(),
            AddTarget::Template(preset) => PostSite::from_preset(preset).account_from_input(&self.posts.add_name).is_some(),
        };
        if !valid {
            self.posts.say("Enter a new username or profile URL");
            return;
        }
        let input = std::mem::take(&mut self.posts.add_name);
        self.posts.add_site = Some(choice.target);
        self.posts_send(match choice.target {
            AddTarget::Site(site_id) => PostsCommand::AddAccount { site_id, input },
            AddTarget::Template(preset) => PostsCommand::AddAccountToPreset { preset, input },
        });
    }

    /// Label of the Posts tab, with the unviewed count.
    pub(super) fn posts_tab_label(&self) -> String {
        match self.posts.unviewed_total() {
            0 => "Posts".to_string(),
            n => format!("Posts ({n})"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_filter_posts() {
        let mut p = Post { viewed: true, starred: true, ..Default::default() };
        assert!(in_section(&p, Section::Viewed) && in_section(&p, Section::Starred) && !in_section(&p, Section::Unviewed));
        p.state = PostState::Removed;
        assert!(!in_section(&p, Section::Viewed));
    }

    #[test]
    fn search_matches_title_or_account() {
        let p = Post { title: "Funny Cat".into(), account: "Emiru".into(), ..Default::default() };
        assert!(matches_query(&p, "") && matches_query(&p, "cat") && matches_query(&p, "emi") && !matches_query(&p, "dog"));
    }

    #[test]
    fn events_update_the_read_model() {
        let mut st = PostsState::new();
        let a = Post { id: "a".into(), ..Default::default() };
        st.apply(PostsEvent::Library(vec![a.clone()]));
        st.apply(PostsEvent::Changed(vec![Post { starred: true, ..a }, Post { id: "b".into(), ..Default::default() }]));
        assert_eq!(st.posts.len(), 2);
        assert!(st.posts[0].starred);
        st.apply(PostsEvent::Removed(vec!["a".into()]));
        assert_eq!(st.posts.len(), 1);
    }
}
