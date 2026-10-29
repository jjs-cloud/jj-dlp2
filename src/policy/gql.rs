//! Twitch GQL lookup of when each channel's last broadcast started.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Duration;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

const GQL_URL: &str = "https://gql.twitch.tv/gql";
/// Public Client-Id the Twitch web client itself uses.
const CLIENT_ID: &str = "kimne78kx3ncx6brgo4mv6wki5h1ko";
const QUERY: &str = "query ChannelLastBroadcast($login: String!) { user(login: $login) { id lastBroadcast { startedAt } } }";
const BATCH_SIZE: usize = 25;

/// Minimum seconds between lookup attempts.
pub const BACKFILL_EVERY_SECS: u64 = 24 * 60 * 60;

/// Broadcast start times by login; logins Twitch doesn't know or that never broadcast are absent.
pub struct Lookup {
    pub found: HashMap<String, u64>,
    pub error: Option<String>,
}

/// The lowercase channel login in a Twitch URL, or `None` for other hosts.
pub fn login_from_url(url: &str) -> Option<String> {
    let url = url.trim();
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let (host, path) = rest.split_once('/')?;
    let host = host.to_ascii_lowercase();
    if host != "twitch.tv" && !host.ends_with(".twitch.tv") {
        return None;
    }
    let login = path.split(['/', '?', '#']).next()?.trim_start_matches('@');
    if login.is_empty() || !login.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some(login.to_ascii_lowercase())
}

/// Parses an RFC 3339 timestamp into unix seconds.
fn parse_started_at(s: &str) -> Option<u64> {
    let secs = OffsetDateTime::parse(s, &Rfc3339).ok()?.unix_timestamp();
    u64::try_from(secs).ok()
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(30))
        .build()
}

/// Looks up the last broadcast start of every login, in batches.
pub fn fetch_last_broadcasts(logins: &[String]) -> Lookup {
    let agent = agent();
    let mut found = HashMap::new();
    let mut error = None;
    for chunk in logins.chunks(BATCH_SIZE) {
        match fetch_batch(&agent, chunk) {
            Ok(times) => {
                for (login, at) in chunk.iter().zip(times) {
                    if let Some(at) = at {
                        found.insert(login.clone(), at);
                    }
                }
            }
            Err(e) => error = Some(e),
        }
    }
    Lookup { found, error }
}

/// One POST holding a query per login; returns the start times in the same order.
fn fetch_batch(agent: &ureq::Agent, logins: &[String]) -> Result<Vec<Option<u64>>, String> {
    let ops: Vec<Value> = logins
        .iter()
        .map(|l| json!({ "operationName": "ChannelLastBroadcast", "variables": { "login": l }, "query": QUERY }))
        .collect();
    let resp: Value = agent
        .post(GQL_URL)
        .set("Client-Id", CLIENT_ID)
        .send_json(Value::Array(ops))
        .map_err(|e| format!("Twitch GQL request failed: {e}"))?
        .into_json()
        .map_err(|e| format!("Twitch GQL response unreadable: {e}"))?;
    let results = resp.as_array().ok_or_else(|| "Twitch GQL response was not a list".to_string())?;
    Ok((0..logins.len())
        .map(|i| {
            results
                .get(i)
                .and_then(|r| r.pointer("/data/user/lastBroadcast/startedAt"))
                .and_then(Value::as_str)
                .and_then(parse_started_at)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_comes_from_the_first_path_segment() {
        assert_eq!(login_from_url("https://twitch.tv/Emiru").as_deref(), Some("emiru"));
        assert_eq!(login_from_url("https://www.twitch.tv/some_one/").as_deref(), Some("some_one"));
        assert_eq!(login_from_url("http://m.twitch.tv/name?x=1").as_deref(), Some("name"));
    }

    #[test]
    fn non_twitch_or_invalid_urls_have_no_login() {
        assert_eq!(login_from_url("https://kick.com/name"), None);
        assert_eq!(login_from_url("https://nottwitch.tv/name"), None);
        assert_eq!(login_from_url("https://twitch.tv/"), None);
        assert_eq!(login_from_url("https://twitch.tv/bad name"), None);
        assert_eq!(login_from_url("twitch.tv"), None);
    }

    #[test]
    fn timestamps_parse_with_or_without_fractions() {
        assert_eq!(parse_started_at("2026-10-28T01:38:20Z"), Some(1793151500));
        assert_eq!(parse_started_at("2026-10-28T01:38:20.55557Z"), Some(1793151500));
        assert_eq!(parse_started_at("2026-10-28T01:38:20.5Z"), Some(1793151500));
        assert_eq!(parse_started_at("garbage"), None);
    }
}
