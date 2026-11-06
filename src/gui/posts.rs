//! Posts monitor front end: the library view, filters and site drafts; the monitor itself runs in the policy layer.

use super::{prefs, App, Msg, SettingsTab, Tab};
use crate::api::now_secs;
use crate::policy::posts::{Post, PostSite, PostState, PostsCommand, PostsConfig, PostsEvent, Preset, SiteId, MIN_INTERVAL_SECS};
use crate::policy::PolicyCommand;
use iced::widget::text_editor::{Action, Content, Edit};
use iced::Task;
use std::collections::HashMap;
use std::path::PathBuf;

/// Cards shown before "Show more" is pressed, and added by each press.
pub const PAGE: usize = 40;
const STATUS_SECS: u64 = 5;
/// Draft key for the unsaved blank site; real site ids start at 1.
pub const NEW_SITE: SiteId = 0;

/// Identifies an account: its site and name.
pub type AccountKey = (SiteId, String);

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
    /// The sites and accounts as last reported by the policy layer.
    pub cfg: PostsConfig,
    /// The library as last reported by the policy layer.
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
    next_check: Option<u64>,
    pub newest_first: bool,
    pub card_w: f32,
    pending_jump: Option<String>,
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
            added: None,
            confirm_remove: None,
            errors: HashMap::new(),
            last_check: None,
            checking: false,
            next_check: None,
            newest_first: prefs.newest_first,
            card_w: prefs.card_w,
            pending_jump: None,
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

    /// True while a jump to a newly added site is waiting for the next tick.
    pub fn jumping(&self) -> bool {
        self.pending_jump.is_some()
    }

    /// The site the username box adds to: the chosen one, else TikTok, else the first.
    pub fn add_target(&self) -> Option<SiteId> {
        let chosen = self.add_site.filter(|id| self.cfg.site(*id).is_some());
        chosen
            .or_else(|| self.cfg.sites.iter().find(|s| s.name.eq_ignore_ascii_case("tiktok")).map(|s| s.id))
            .or_else(|| self.cfg.sites.first().map(|s| s.id))
    }

    /// Seconds until the next check, or `None` while checking, paused or with nothing to check.
    pub fn secs_to_next(&self) -> Option<u64> {
        if self.checking {
            return None;
        }
        self.next_check.map(|at| at.saturating_sub(now_secs()))
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
                self.checking = st.checking;
                self.last_check = st.last_check;
                self.next_check = st.next_check;
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
                self.pending_jump = Some(name.clone());
                self.added = Some((name, now_secs()));
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

    fn posts_send(&self, cmd: PostsCommand) {
        self.client.send(PolicyCommand::Posts(cmd));
    }

    /// Scrolls to a newly added site once its card has been drawn; called on every tick.
    pub(super) fn posts_tick(&mut self) -> Task<Msg> {
        match self.posts.pending_jump.take() {
            Some(name) => self.posts_jump(&name).unwrap_or_else(Task::none),
            None => Task::none(),
        }
    }

    /// Applies a state change reported by the policy layer.
    pub(super) fn posts_on_event(&mut self, ev: PostsEvent) {
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
            PostsMsg::OpenSettings => {
                self.tab = Tab::Settings;
                self.settings_tab = SettingsTab::Posts;
            }
            PostsMsg::AddSite(Preset::Blank) => {
                self.posts.drafts.entry(NEW_SITE).or_insert_with(|| SiteDraft::from_site(&PostSite::from_preset(Preset::Blank)));
            }
            PostsMsg::AddSite(preset) => {
                let site = PostSite::from_preset(preset);
                if self.posts.cfg.name_taken(&site.name) {
                    return self.posts_jump(&site.name).unwrap_or_else(Task::none);
                }
                self.posts_send(PostsCommand::AddSite(site));
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
            PostsMsg::ConfirmRemoveSite(id) => {
                self.posts.confirm_remove = None;
                if self.posts.add_site == Some(id) {
                    self.posts.add_site = None;
                }
                if self.posts.account.as_ref().is_some_and(|(sid, _)| *sid == id) {
                    self.posts.account = None;
                }
                self.posts_send(PostsCommand::RemoveSite(id));
            }
            PostsMsg::Site(id, field) => self.posts_site_field(id, field),
            PostsMsg::RemoveAccount(id, name) => {
                if self.posts.account.as_ref() == Some(&(id, name.clone())) {
                    self.posts.account = None;
                }
                self.posts_send(PostsCommand::RemoveAccount { site_id: id, name });
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

    /// Scrolls the Posts site list to the named site, if it exists.
    fn posts_jump(&self, name: &str) -> Option<Task<Msg>> {
        let sites = &self.posts.cfg.sites;
        let idx = sites.iter().position(|s| s.name.eq_ignore_ascii_case(name))?;
        let y = if sites.len() > 1 { idx as f32 / (sites.len() - 1) as f32 } else { 0.0 };
        Some(iced::widget::operation::snap_to(super::posts_scroll_id(), iced::widget::scrollable::RelativeOffset { x: 0.0, y }))
    }

    /// Sends a site's draft to the policy layer: adds the unsaved blank site, or updates an existing one.
    fn posts_save_site(&mut self, id: SiteId) {
        let saved = self.posts.cfg.site(id).cloned().unwrap_or_default();
        let Some(site) = self.posts.drafts.get(&id).and_then(|d| d.build(&saved)) else { return };
        if self.posts.cfg.sites.iter().any(|s| s.id != id && s.name.trim().eq_ignore_ascii_case(&site.name)) {
            self.posts.say(format!("A site named {} already exists", site.name));
            return;
        }
        let cmd = if id == NEW_SITE { PostsCommand::AddSite(site) } else { PostsCommand::UpdateSite(site) };
        self.posts_send(cmd);
    }

    /// Sends the typed username or profile URL for the site chosen under the posts grid.
    fn posts_add_account(&mut self) {
        let Some(id) = self.posts.add_target() else { return };
        let valid = self.posts.cfg.site(id).and_then(|s| s.account_from_input(&self.posts.add_name)).is_some();
        if !valid {
            self.posts.say("Enter a new username or profile URL");
            return;
        }
        let input = std::mem::take(&mut self.posts.add_name);
        self.posts_send(PostsCommand::AddAccount { site_id: id, input });
    }

    /// Label of the Posts switch on the Monitor tab, with the unviewed count.
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
