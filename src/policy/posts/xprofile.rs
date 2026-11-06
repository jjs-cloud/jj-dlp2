//! Lists an X/Twitter account's recent posts from its logged-out profile page, which yt-dlp cannot list.

use std::io::Read;
use std::time::Duration;

const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
const MAX_PAGE_BYTES: u64 = 8 * 1024 * 1024;
/// Newest posts examined per check.
pub const MAX_CANDIDATES: usize = 10;

fn host_is(host: &str, domain: &str) -> bool {
    host == domain || host.strip_suffix(domain).is_some_and(|p| p.ends_with('.'))
}

/// The username of an x.com or twitter.com profile URL; `None` for other hosts and for post or sub-page URLs.
pub fn profile_user(url: &str) -> Option<String> {
    let rest = url.trim().split_once("://")?.1;
    let (host, path) = rest.split_once('/')?;
    let host = host.to_ascii_lowercase();
    if !host_is(&host, "x.com") && !host_is(&host, "twitter.com") {
        return None;
    }
    let mut segs = path.split(['?', '#']).next()?.split('/').filter(|s| !s.is_empty());
    let user = segs.next()?.trim_start_matches('@');
    let valid = !user.is_empty() && user.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    (valid && segs.next().is_none()).then(|| user.to_string())
}

/// Ids of the user's posts linked in the page, in page order without repeats.
pub fn extract_status_ids(html: &str, user: &str) -> Vec<String> {
    let text = html.replace("\\/", "/").to_ascii_lowercase();
    let needle = format!("/{}/status/", user.to_ascii_lowercase());
    let mut ids: Vec<String> = Vec::new();
    let mut from = 0;
    while let Some(pos) = text[from..].find(&needle) {
        let start = from + pos + needle.len();
        let id: String = text[start..].chars().take_while(char::is_ascii_digit).collect();
        from = start;
        if !id.is_empty() && !ids.contains(&id) {
            ids.push(id);
        }
    }
    ids
}

/// The newest `MAX_CANDIDATES` post ids, newest first.
pub fn newest(mut ids: Vec<String>) -> Vec<String> {
    ids.sort_by_key(|id| std::cmp::Reverse(id.parse::<u64>().unwrap_or(0)));
    ids.truncate(MAX_CANDIDATES);
    ids
}

/// Downloads the profile page.
pub async fn fetch_profile(url: String, user_agent: Option<String>) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let agent = ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout_read(Duration::from_secs(20)).build();
        let ua = user_agent.as_deref().map(str::trim).filter(|u| !u.is_empty()).unwrap_or(DEFAULT_USER_AGENT);
        let resp = agent
            .get(&url)
            .set("User-Agent", ua)
            .set("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .set("Accept-Language", "en-US,en;q=0.9")
            .call()
            .map_err(|e| format!("could not load the profile page: {e}"))?;
        let mut buf = Vec::new();
        resp.into_reader().take(MAX_PAGE_BYTES).read_to_end(&mut buf).map_err(|e| format!("could not read the profile page: {e}"))?;
        Ok(String::from_utf8_lossy(&buf).into_owned())
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_urls_give_the_username() {
        assert_eq!(profile_user("https://x.com/smeepsx").as_deref(), Some("smeepsx"));
        assert_eq!(profile_user("https://www.twitter.com/@Ab_1/?lang=en").as_deref(), Some("Ab_1"));
        assert_eq!(profile_user("https://mobile.twitter.com/ab/").as_deref(), Some("ab"));
        assert_eq!(profile_user("https://x.com/ab/status/123"), None);
        assert_eq!(profile_user("https://notx.com/ab"), None);
        assert_eq!(profile_user("https://www.tiktok.com/@ab"), None);
        assert_eq!(profile_user("https://x.com/"), None);
    }

    #[test]
    fn status_ids_are_deduped_and_filtered_by_user() {
        let html = r#"<a href="/SmeepsX/status/20">a</a><a href="/smeepsx/status/10?s=1">b</a>
            <a href="/smeepsx/status/10/photo/1">c</a><a href="/other/status/99">d</a>
            <a href="/xsmeepsx/status/98">e</a>{"u":"https:\/\/x.com\/smeepsx\/status\/30"}"#;
        assert_eq!(extract_status_ids(html, "smeepsx"), ["20", "10", "30"]);
        assert!(extract_status_ids("<html></html>", "smeepsx").is_empty());
    }

    #[test]
    fn newest_sorts_by_id_and_caps() {
        let ids: Vec<String> = (1..=12).map(|n| n.to_string()).collect();
        let top = newest(ids);
        assert_eq!((top.len(), top[0].as_str(), top[9].as_str()), (MAX_CANDIDATES, "12", "3"));
    }
}
