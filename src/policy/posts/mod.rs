//! Posts monitor: checks accounts for new posts, downloads them and keeps the post library.

mod model;
mod monitor;
mod runner;
mod store;
mod xprofile;

pub use model::*;
pub use monitor::PostsMonitor;
pub use store::load_problem;

use runner::{CycleReport, DownloadOutcome};

/// What a front end can ask the Posts monitor to do.
#[derive(Debug, Clone)]
pub enum PostsCommand {
    /// Checks every site on the next tick.
    CheckNow,
    /// Pauses or resumes every site.
    SetPaused(bool),
    /// Opens the downloaded file, else the stream or the post page.
    Play(String),
    OpenPage(String),
    /// Shows the downloaded file in the file manager.
    Reveal(String),
    /// Sends the downloaded file to the recycle bin and hides the post.
    Delete(String),
    /// Queues a post that is not downloaded, or retries a failed one.
    Download(String),
    /// Stops a queued or running download.
    CancelDownload(String),
    SetViewed { ids: Vec<String>, viewed: bool },
    SetStarred { id: String, starred: bool },
    /// Adds a site; its id and accounts are ignored.
    AddSite(PostSite),
    /// Replaces a site's settings; its accounts are left as they are.
    UpdateSite(PostSite),
    /// Removes a site with its accounts and posts.
    RemoveSite(SiteId),
    AddAccount { site_id: SiteId, input: String },
    /// Removes an account with its posts.
    RemoveAccount { site_id: SiteId, name: String },
    /// Opens a site's download folder, creating it when missing.
    OpenOutputDir(SiteId),
}

/// An account whose last check failed.
#[derive(Debug, Clone, PartialEq)]
pub struct AccountError {
    pub site_id: SiteId,
    pub name: String,
    pub message: String,
}

/// Progress of the checks, for display.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PostsStatus {
    pub checking: bool,
    /// Unix time the last check finished.
    pub last_check: Option<u64>,
    /// Unix time the next check starts; `None` while paused or with nothing to check.
    pub next_check: Option<u64>,
    pub errors: Vec<AccountError>,
}

/// State changes of the Posts monitor reported to front ends.
#[derive(Debug, Clone)]
pub enum PostsEvent {
    /// The sites and their accounts; sent at startup and after every change.
    Config(PostsConfig),
    /// Every post in the library; sent at startup.
    Library(Vec<Post>),
    /// Posts that were added or changed.
    Changed(Vec<Post>),
    /// Ids of posts that left the library.
    Removed(Vec<String>),
    Status(PostsStatus),
    /// A short message for the person using the front end.
    Notice(String),
    /// A site was added.
    SiteAdded { id: SiteId, name: String },
}

/// Results of background work, fed back into the policy loop.
pub(crate) enum PostsMsg {
    Checked(CycleReport),
    Downloaded(String, DownloadOutcome),
    Stream { id: String, result: Result<String, String> },
    Notified(Vec<String>),
}
