//! State, messages and logic of the Log tab's Output sub-tab.

use super::{App, Msg, prefs};
use crate::api::{SiteId, StreamerId};
use crate::engine::logfile;
use crate::filemgr::ops;
use crate::policy::posts::{LogTarget, PostsCommand};
use crate::policy::{PolicyCommand, logtail};
use iced::Task;
use iced::widget::text_editor::{Action, Content, Edit, Motion};
use std::collections::HashSet;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;

/// Lines loaded when a file is opened.
const TAIL_LINES: usize = 500;
/// The view is cut back to `VIEW_KEEP` lines once it holds more than `VIEW_MAX`.
const VIEW_MAX: usize = 2000;
const VIEW_KEEP: usize = 1000;
/// Most lines held back while following is paused.
const PENDING_MAX: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Stdout,
    Stderr,
}

impl LogKind {
    /// The name used in log file names.
    pub fn file_label(self) -> &'static str {
        match self {
            LogKind::Stdout => "stdout",
            LogKind::Stderr => "stderr",
        }
    }
}

#[derive(Debug, Clone)]
pub enum LogsMsg {
    Open(PathBuf, String),
    Listed(HashSet<String>),
    Polled(u64, Result<logtail::Chunk, ErrorKind>),
    Action(Action),
    SetFollow(bool),
    SetWrap(bool),
    Copy,
    RevealFolder,
    OpenUiLog,
    ToggleStreamer(StreamerId, LogKind, bool),
    ToggleChecker(SiteId, LogKind, bool),
    TogglePostChecker(SiteId, String, LogKind, bool),
    TogglePostDownloader(SiteId, LogKind, bool),
}

pub struct LogsState {
    /// File names present in the logs folder.
    pub existing: HashSet<String>,
    /// The viewed file and its display label.
    pub open: Option<(PathBuf, String)>,
    pub content: Content,
    pub follow: bool,
    pub wrap: bool,
    /// The viewed file does not exist (yet).
    pub missing: bool,
    /// Lines that arrived while following was paused.
    pub pending: Vec<String>,
    offset: Option<u64>,
    lines: usize,
    generation: u64,
    polling: bool,
    listing: bool,
}

impl LogsState {
    pub fn new() -> Self {
        LogsState {
            existing: HashSet::new(),
            open: None,
            content: Content::new(),
            follow: true,
            wrap: prefs::load_wrap_logs(),
            missing: false,
            pending: Vec::new(),
            offset: None,
            lines: 0,
            generation: 0,
            polling: false,
            listing: false,
        }
    }

    /// Whether the file's name is in the logs folder.
    pub fn has_file(&self, path: &std::path::Path) -> bool {
        path.file_name().and_then(|n| n.to_str()).is_some_and(|n| self.existing.contains(n))
    }

    fn start(&mut self, path: PathBuf, label: String) {
        self.generation += 1;
        self.open = Some((path, label));
        self.content = Content::new();
        self.lines = 0;
        self.offset = None;
        self.pending.clear();
        self.follow = true;
        self.missing = false;
        self.polling = false;
    }

    fn to_end(&mut self) {
        self.content.perform(Action::Move(Motion::DocumentEnd));
    }

    /// Replaces the view with the given lines.
    fn show(&mut self, lines: &[String]) {
        self.content = Content::with_text(&lines.join("\n"));
        self.lines = lines.len();
        self.to_end();
    }

    /// Adds lines at the end, cutting the view back when it grows too long.
    fn append(&mut self, lines: &[String]) {
        if lines.is_empty() {
            return;
        }
        if self.lines == 0 {
            return self.show(lines);
        }
        self.to_end();
        self.content.perform(Action::Edit(Edit::Paste(Arc::new(format!("\n{}", lines.join("\n"))))));
        self.lines += lines.len();
        if self.lines > VIEW_MAX {
            let text = self.content.text();
            let all: Vec<&str> = text.lines().collect();
            let keep: Vec<String> = all[all.len().saturating_sub(VIEW_KEEP)..].iter().map(|l| l.to_string()).collect();
            self.show(&keep);
        }
    }

    fn apply(&mut self, chunk: logtail::Chunk) {
        self.missing = false;
        self.offset = Some(chunk.offset);
        if chunk.restart {
            self.pending.clear();
            self.show(&chunk.lines);
        } else if self.follow {
            self.append(&chunk.lines);
        } else {
            self.pending.extend(chunk.lines);
            let over = self.pending.len().saturating_sub(PENDING_MAX);
            self.pending.drain(..over);
        }
    }

    /// Applies a viewer action; selecting or moving away from the end pauses following.
    fn act(&mut self, action: Action) {
        if action.is_edit() {
            return;
        }
        let pauses = match &action {
            Action::Move(_)
            | Action::Select(_)
            | Action::SelectWord
            | Action::SelectLine
            | Action::SelectAll
            | Action::Drag(_) => true,
            Action::Scroll { lines } => *lines < 0,
            _ => false,
        };
        if pauses {
            self.follow = false;
        }
        self.content.perform(action);
    }

    fn set_follow(&mut self, on: bool) {
        self.follow = on;
        if on {
            let lines = std::mem::take(&mut self.pending);
            self.append(&lines);
            self.to_end();
        }
    }
}

impl App {
    /// Refreshes the file list and the viewed file while the Output sub-tab is shown.
    pub(super) fn logs_tick(&mut self) -> Task<Msg> {
        let mut tasks = vec![self.logs_poll()];
        if !self.logs.listing {
            self.logs.listing = true;
            tasks.push(Task::perform(
                async { tokio::task::spawn_blocking(logtail::existing).await.unwrap_or_default() },
                |files| Msg::Logs(LogsMsg::Listed(files)),
            ));
        }
        Task::batch(tasks)
    }

    fn logs_poll(&mut self) -> Task<Msg> {
        let Some((path, _)) = self.logs.open.clone() else { return Task::none() };
        if self.logs.polling {
            return Task::none();
        }
        self.logs.polling = true;
        let (generation, offset) = (self.logs.generation, self.logs.offset);
        Task::perform(
            async move {
                tokio::task::spawn_blocking(move || logtail::read(&path, offset, TAIL_LINES)).await.unwrap_or(Err(ErrorKind::Other))
            },
            move |result| Msg::Logs(LogsMsg::Polled(generation, result)),
        )
    }

    pub(super) fn logs_update(&mut self, m: LogsMsg) -> Task<Msg> {
        match m {
            LogsMsg::Open(path, label) => {
                self.logs.start(path, label);
                return self.logs_poll();
            }
            LogsMsg::Listed(files) => {
                self.logs.listing = false;
                self.logs.existing = files;
            }
            LogsMsg::Polled(generation, result) => {
                if generation != self.logs.generation {
                    return Task::none();
                }
                self.logs.polling = false;
                match result {
                    Ok(chunk) => self.logs.apply(chunk),
                    Err(ErrorKind::NotFound) => {
                        self.logs.missing = true;
                        self.logs.offset = None;
                    }
                    Err(_) => {}
                }
            }
            LogsMsg::Action(action) => self.logs.act(action),
            LogsMsg::SetFollow(on) => self.logs.set_follow(on),
            LogsMsg::SetWrap(on) => {
                self.logs.wrap = on;
                prefs::save_wrap_logs(on);
            }
            LogsMsg::Copy => {
                let text = self.logs.content.selection().unwrap_or_else(|| self.logs.content.text());
                return iced::clipboard::write(text);
            }
            LogsMsg::RevealFolder => {
                let dir = logfile::logs_dir();
                let _ = std::fs::create_dir_all(&dir);
                let result = match &self.logs.open {
                    Some((file, _)) => ops::reveal(file),
                    None => open::that_detached(&dir).map_err(|e| e.to_string()),
                };
                if let Err(e) = result {
                    self.note(format!("could not open {}: {e}", dir.display()));
                }
            }
            LogsMsg::OpenUiLog => ops::open_detached(crate::policy::uilog::path()),
            LogsMsg::ToggleStreamer(id, kind, on) => self.toggle_streamer_log(id, kind, on),
            LogsMsg::ToggleChecker(id, kind, on) => self.toggle_checker_log(id, kind, on),
            LogsMsg::TogglePostChecker(site_id, name, kind, on) => self.set_post_log(site_id, LogTarget::Checker(name), kind, on),
            LogsMsg::TogglePostDownloader(site_id, kind, on) => self.set_post_log(site_id, LogTarget::Downloader, kind, on),
        }
        Task::none()
    }

    /// Saves a streamer's log switch and mirrors it into its open settings draft.
    fn toggle_streamer_log(&mut self, id: StreamerId, kind: LogKind, on: bool) {
        let Some(streamer) = self.settings.streamers.iter().find(|s| s.id == id) else { return };
        let mut options = streamer.options.clone();
        let draft = self.streamer_drafts.get_mut(&id);
        match kind {
            LogKind::Stdout => {
                options.log_stdout = on;
                if let Some(d) = draft {
                    d.log_stdout = on;
                }
            }
            LogKind::Stderr => {
                options.log_stderr = on;
                if let Some(d) = draft {
                    d.log_stderr = on;
                }
            }
        }
        self.client.send(PolicyCommand::SetStreamerOptions { id, options });
    }

    /// Saves a site's checker log switch and mirrors it into its open settings draft.
    fn toggle_checker_log(&mut self, id: SiteId, kind: LogKind, on: bool) {
        let Some(found) = self.settings.sites.iter().find(|s| s.id == id) else { return };
        let mut site = found.clone();
        let draft = self.drafts.get_mut(&id);
        match kind {
            LogKind::Stdout => {
                site.log_checker_stdout = on;
                if let Some(d) = draft {
                    d.log_stdout = on;
                }
            }
            LogKind::Stderr => {
                site.log_checker_stderr = on;
                if let Some(d) = draft {
                    d.log_stderr = on;
                }
            }
        }
        self.client.send(PolicyCommand::UpdateSite(site));
    }

    /// Asks the Posts monitor to turn one of a Posts site's command logs on or off.
    fn set_post_log(&mut self, site_id: SiteId, target: LogTarget, kind: LogKind, on: bool) {
        let stderr = kind == LogKind::Stderr;
        self.client.send(PolicyCommand::Posts(PostsCommand::SetLog { site_id, target, stderr, on }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(lines: &[&str], restart: bool) -> logtail::Chunk {
        logtail::Chunk { lines: lines.iter().map(|l| l.to_string()).collect(), offset: 0, restart }
    }

    #[test]
    fn following_appends_new_lines() {
        let mut s = LogsState::new();
        s.apply(chunk(&["a", "b"], true));
        s.apply(chunk(&["c"], false));
        assert_eq!(s.content.text(), "a\nb\nc");
    }

    #[test]
    fn paused_lines_are_held_until_following_resumes() {
        let mut s = LogsState::new();
        s.apply(chunk(&["a"], true));
        s.act(Action::SelectAll);
        assert!(!s.follow);
        s.apply(chunk(&["b", "c"], false));
        assert_eq!(s.content.text(), "a");
        assert_eq!(s.pending.len(), 2);
        s.set_follow(true);
        assert_eq!(s.content.text(), "a\nb\nc");
        assert!(s.pending.is_empty());
    }

    #[test]
    fn restart_replaces_the_view() {
        let mut s = LogsState::new();
        s.apply(chunk(&["a", "b"], true));
        s.apply(chunk(&["x"], true));
        assert_eq!(s.content.text(), "x");
    }

    #[test]
    fn edits_are_ignored() {
        let mut s = LogsState::new();
        s.apply(chunk(&["a"], true));
        s.act(Action::Edit(Edit::Insert('z')));
        assert_eq!(s.content.text(), "a");
    }

    #[test]
    fn the_view_is_cut_back_when_too_long() {
        let mut s = LogsState::new();
        s.apply(chunk(&["first"], true));
        let many: Vec<String> = (0..VIEW_MAX).map(|i| format!("l{i}")).collect();
        s.apply(logtail::Chunk { lines: many, offset: 0, restart: false });
        let text = s.content.text();
        assert_eq!(text.lines().count(), VIEW_KEEP);
        assert!(text.ends_with(&format!("l{}", VIEW_MAX - 1)));
    }
}
