//! Desktop and ntfy notifications, plus the gate that decides when one may be sent.

use crate::api::StreamerId;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

/// The ntfy server notifications are published to.
const NTFY_URL: &str = "https://ntfy.sh";
/// Longest ntfy topic name.
const TOPIC_MAX: usize = 64;
/// How long an ntfy request may take.
const NTFY_TIMEOUT: Duration = Duration::from_secs(10);

/// What a notification is about; cooldowns are tracked per streamer and kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Live,
    Stalled,
    Restarted,
}

/// The text of one notification.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub title: String,
    pub body: String,
    /// Page opened when the ntfy notification is tapped.
    pub click: Option<String>,
}

/// Where a notification goes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Targets {
    /// Seconds the desktop notification stays up; `None` skips the desktop.
    pub desktop_timeout_secs: Option<u64>,
    /// ntfy topic; `None` skips ntfy.
    pub ntfy_topic: Option<String>,
}

impl Targets {
    pub fn is_empty(&self) -> bool {
        self.desktop_timeout_secs.is_none() && self.ntfy_topic.is_none()
    }
}

/// True when `topic` is a usable ntfy topic name.
pub fn topic_valid(topic: &str) -> bool {
    let t = topic.trim();
    !t.is_empty() && t.len() <= TOPIC_MAX && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Why a streamer is not recording; more reasons can be added here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotRecordingReason {
    Disabled,
    IntroDelay,
    LowerPriority,
}

impl NotRecordingReason {
    /// True when the reason only delays starting: a running recording is left alone and a recording notice follows once it clears.
    pub fn only_delays_start(self) -> bool {
        matches!(self, NotRecordingReason::IntroDelay)
    }

    /// True when a recording notice follows once the reason clears.
    pub fn defers_notice(self) -> bool {
        matches!(self, NotRecordingReason::IntroDelay | NotRecordingReason::LowerPriority)
    }
}

impl std::fmt::Display for NotRecordingReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotRecordingReason::Disabled => f.write_str("Disabled"),
            NotRecordingReason::IntroDelay => f.write_str("Intro Delay"),
            NotRecordingReason::LowerPriority => f.write_str("Lower priority"),
        }
    }
}

/// What a live notification says about the recording.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveStatus {
    NotRecording(NotRecordingReason),
    Unconfirmed,
    Confirmed,
}

/// Body of the live notification: recording status, the reason when not recording, a warning per evicted streamer, then the site.
pub fn live_body(status: LiveStatus, site: &str, evicted: &[String]) -> String {
    let status = match status {
        LiveStatus::NotRecording(reason) => format!("\u{1F7E1} Status: Not recording\nReason: {reason}"),
        LiveStatus::Confirmed => "\u{1F534} Status: Recording (confirmed)".to_string(),
        LiveStatus::Unconfirmed => "\u{1F534} Status: Recording (unconfirmed)".to_string(),
    };
    let warnings: String = evicted.iter().map(|name| format!("\n\u{1F7E1} Warning: evicted {name}")).collect();
    format!("{status}{warnings}\nSite: {site}")
}

/// The message with the free space of the drive holding `folder` added to its body; blocks.
pub fn with_disk_space(mut msg: Message, folder: &std::path::Path) -> Message {
    if let Some(line) = super::disks::free_line(folder) {
        msg.body = format!("{}\n{line}", msg.body);
    }
    msg
}

/// Sends a message to every target and returns the log lines for failures; blocks, so call it off the async threads.
pub fn deliver(msg: &Message, targets: &Targets, always_log_missing: bool) -> Vec<String> {
    let mut logs = Vec::new();
    if let Some(secs) = targets.desktop_timeout_secs {
        if let Err(e) = desktop(msg, secs, always_log_missing) {
            logs.extend(e);
        }
    }
    if let Some(topic) = &targets.ntfy_topic {
        if let Err(e) = ntfy(msg, topic) {
            logs.push(format!("ntfy notification failed: {e}"));
        }
    }
    logs
}

/// Posts the message as JSON so titles and bodies can hold any characters.
fn ntfy(msg: &Message, topic: &str) -> Result<(), String> {
    let mut body = serde_json::json!({ "topic": topic, "title": msg.title, "message": msg.body });
    if let Some(click) = &msg.click {
        body["click"] = click.clone().into();
    }
    ureq::post(NTFY_URL).timeout(NTFY_TIMEOUT).send_json(body).map(|_| ()).map_err(|e| e.to_string())
}

/// Shows a Windows toast; the timeout picks the nearer of the short and long toast durations.
#[cfg(windows)]
fn desktop(msg: &Message, timeout_secs: u64, _always_log_missing: bool) -> Result<(), Vec<String>> {
    use tauri_winrt_notification::{Duration as ToastDuration, Toast};
    let duration = if timeout_secs < 16 { ToastDuration::Short } else { ToastDuration::Long };
    Toast::new(Toast::POWERSHELL_APP_ID)
        .title(&msg.title)
        .text1(&msg.body)
        .duration(duration)
        .show()
        .map_err(|e| vec![format!("desktop notification failed: {e}")])
}

/// Runs `notify-send`; a missing program is logged once per session unless `always_log_missing`.
#[cfg(target_os = "linux")]
fn desktop(msg: &Message, timeout_secs: u64, always_log_missing: bool) -> Result<(), Vec<String>> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static MISSING_LOGGED: AtomicBool = AtomicBool::new(false);
    let result = std::process::Command::new("notify-send")
        .args(["-a", "jj-dlp", "-t", &timeout_secs.saturating_mul(1000).to_string(), "--"])
        .arg(&msg.title)
        .arg(&msg.body)
        .output();
    match result {
        Ok(out) if out.status.success() => Ok(()),
        Ok(out) => Err(vec![format!(
            "notify-send failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )]),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if always_log_missing || !MISSING_LOGGED.swap(true, Ordering::Relaxed) {
                Err(vec!["notify-send not found; desktop notifications are unavailable".to_string()])
            } else {
                Ok(())
            }
        }
        Err(e) => Err(vec![format!("could not run notify-send: {e}")]),
    }
}

/// Desktop notifications are only available on Windows and Linux.
#[cfg(not(any(windows, target_os = "linux")))]
fn desktop(_msg: &Message, _timeout_secs: u64, _always_log_missing: bool) -> Result<(), Vec<String>> {
    Ok(())
}

/// Per-streamer state deciding when a notification may go out.
#[derive(Default)]
pub struct Gate {
    last_sent: HashMap<(StreamerId, Kind), Instant>,
    held: HashSet<StreamerId>,
    seen: HashSet<StreamerId>,
    deferred: HashSet<StreamerId>,
}

impl Gate {
    /// True the first time a streamer's check result is seen this session.
    pub fn first_check(&mut self, id: StreamerId) -> bool {
        self.seen.insert(id)
    }

    /// True when no notification of this kind was sent to the streamer within `cooldown`.
    pub fn ready(&self, id: StreamerId, kind: Kind, cooldown: Duration, now: Instant) -> bool {
        self.last_sent.get(&(id, kind)).is_none_or(|at| now.duration_since(*at) >= cooldown)
    }

    pub fn mark_sent(&mut self, id: StreamerId, kind: Kind, now: Instant) {
        self.last_sent.insert((id, kind), now);
    }

    /// Holds a live notification until the recording is confirmed growing.
    pub fn hold(&mut self, id: StreamerId) {
        self.held.insert(id);
    }

    /// Releases a held live notification; true when one was waiting.
    pub fn take_held(&mut self, id: StreamerId) -> bool {
        self.held.remove(&id)
    }

    /// Notes that a not-recording notice went out and a recording notice should follow.
    pub fn defer(&mut self, id: StreamerId) {
        self.deferred.insert(id);
    }

    /// Clears the follow-up note; true when one was waiting.
    pub fn take_deferred(&mut self, id: StreamerId) -> bool {
        self.deferred.remove(&id)
    }

    /// Forgets when a notification kind was last sent, so the next one skips the cooldown.
    pub fn clear_sent(&mut self, id: StreamerId, kind: Kind) {
        self.last_sent.remove(&(id, kind));
    }

    /// Keeps only the streamers the predicate accepts.
    pub fn retain(&mut self, mut keep: impl FnMut(StreamerId) -> bool) {
        self.last_sent.retain(|(id, _), _| keep(*id));
        self.held.retain(|id| keep(*id));
        self.seen.retain(|id| keep(*id));
        self.deferred.retain(|id| keep(*id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topics_allow_letters_digits_dash_and_underscore() {
        assert!(topic_valid("j73hd92xl"));
        assert!(topic_valid(" my-topic_1 "));
        assert!(!topic_valid(""));
        assert!(!topic_valid("   "));
        assert!(!topic_valid("a/b"));
        assert!(!topic_valid("a b"));
        assert!(!topic_valid(&"a".repeat(65)));
    }

    #[test]
    fn live_body_reports_status_reason_and_site() {
        let not_recording = |r| LiveStatus::NotRecording(r);
        let disabled = live_body(not_recording(NotRecordingReason::Disabled), "Twitch", &[]);
        assert_eq!(disabled, "\u{1F7E1} Status: Not recording\nReason: Disabled\nSite: Twitch");
        let delay = live_body(not_recording(NotRecordingReason::IntroDelay), "Twitch", &[]);
        assert_eq!(delay, "\u{1F7E1} Status: Not recording\nReason: Intro Delay\nSite: Twitch");
        assert_eq!(live_body(LiveStatus::Confirmed, "Twitch", &[]), "\u{1F534} Status: Recording (confirmed)\nSite: Twitch");
        assert_eq!(live_body(LiveStatus::Unconfirmed, "Twitch", &[]), "\u{1F534} Status: Recording (unconfirmed)\nSite: Twitch");
        let low = live_body(not_recording(NotRecordingReason::LowerPriority), "Twitch", &[]);
        assert_eq!(low, "\u{1F7E1} Status: Not recording\nReason: Lower priority\nSite: Twitch");
    }

    #[test]
    fn live_body_warns_about_each_evicted_streamer() {
        let evicted = ["amy".to_string(), "bo".to_string()];
        let body = live_body(LiveStatus::Unconfirmed, "Twitch", &evicted);
        let want = "\u{1F534} Status: Recording (unconfirmed)\n\u{1F7E1} Warning: evicted amy\n\u{1F7E1} Warning: evicted bo\nSite: Twitch";
        assert_eq!(body, want);
    }

    #[test]
    fn only_the_intro_delay_just_delays_the_start() {
        assert!(NotRecordingReason::IntroDelay.only_delays_start());
        assert!(!NotRecordingReason::Disabled.only_delays_start());
        assert!(!NotRecordingReason::LowerPriority.only_delays_start());
    }

    #[test]
    fn delay_and_priority_defer_the_recording_notice() {
        assert!(NotRecordingReason::IntroDelay.defers_notice());
        assert!(NotRecordingReason::LowerPriority.defers_notice());
        assert!(!NotRecordingReason::Disabled.defers_notice());
    }

    #[test]
    fn cooldown_is_per_streamer_and_kind() {
        let (mut g, t0) = (Gate::default(), Instant::now());
        let cd = Duration::from_secs(60);
        assert!(g.ready(1, Kind::Live, cd, t0));
        g.mark_sent(1, Kind::Live, t0);
        assert!(!g.ready(1, Kind::Live, cd, t0 + Duration::from_secs(59)));
        assert!(g.ready(1, Kind::Live, cd, t0 + Duration::from_secs(60)));
        assert!(g.ready(1, Kind::Stalled, cd, t0));
        assert!(g.ready(2, Kind::Live, cd, t0));
        assert!(g.ready(1, Kind::Live, Duration::ZERO, t0));
    }

    #[test]
    fn deferred_notice_is_taken_once_and_clear_sent_skips_the_cooldown() {
        let (mut g, t0) = (Gate::default(), Instant::now());
        g.defer(1);
        assert!(g.take_deferred(1));
        assert!(!g.take_deferred(1));
        g.mark_sent(1, Kind::Live, t0);
        assert!(!g.ready(1, Kind::Live, Duration::from_secs(60), t0));
        g.clear_sent(1, Kind::Live);
        assert!(g.ready(1, Kind::Live, Duration::from_secs(60), t0));
    }

    #[test]
    fn first_check_and_held_state() {
        let mut g = Gate::default();
        assert!(g.first_check(1));
        assert!(!g.first_check(1));
        g.hold(1);
        assert!(g.take_held(1));
        assert!(!g.take_held(1));
    }
}
