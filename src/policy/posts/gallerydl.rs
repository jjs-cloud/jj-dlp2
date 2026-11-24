//! Reads gallery-dl's JSON listing of an X/Twitter account into posts.

use super::xprofile::MAX_CANDIDATES;
use serde_json::Value;

/// One post found in the listing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Tweet {
    pub id: String,
    pub text: String,
    pub posted_at: Option<u64>,
    /// True when a file of the post is not a photo.
    pub has_video: bool,
    /// Full-size photo links in order.
    pub photos: Vec<String>,
}

fn id_of(v: &Value) -> Option<String> {
    match v {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// Unix seconds of a gallery-dl `YYYY-MM-DD HH:MM:SS` date, which is UTC.
fn date_secs(s: &str) -> Option<u64> {
    let t = chrono::NaiveDateTime::parse_from_str(s.trim(), "%Y-%m-%d %H:%M:%S").ok()?;
    u64::try_from(t.and_utc().timestamp()).ok()
}

/// The posts of `user` in `gallery-dl -j` output, in listing order without repeats.
pub fn parse_listing(json: &str, user: &str) -> Result<Vec<Tweet>, String> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let root: Value = serde_json::from_str(json.trim()).map_err(|e| format!("could not read gallery-dl's output: {e}"))?;
    let items = root.as_array().ok_or("gallery-dl's output was not a list")?;
    let mut tweets: Vec<Tweet> = Vec::new();
    for item in items {
        let Some(parts) = item.as_array() else { continue };
        let (url, meta) = match parts.first().and_then(Value::as_i64) {
            Some(2) => (None, parts.get(1)),
            Some(3) => (parts.get(1).and_then(Value::as_str), parts.get(2)),
            _ => continue,
        };
        let Some(meta) = meta else { continue };
        let author = meta.get("author").and_then(|a| a.get("name")).and_then(Value::as_str);
        if author.is_some_and(|a| !a.eq_ignore_ascii_case(user)) {
            continue;
        }
        let Some(id) = meta.get("tweet_id").and_then(id_of) else { continue };
        let idx = match tweets.iter().position(|t| t.id == id) {
            Some(i) => i,
            None => {
                tweets.push(Tweet {
                    id,
                    text: meta.get("content").and_then(Value::as_str).unwrap_or("").trim().to_string(),
                    posted_at: meta.get("date").and_then(Value::as_str).and_then(date_secs),
                    ..Default::default()
                });
                tweets.len() - 1
            }
        };
        let Some(url) = url else { continue };
        match meta.get("type").and_then(Value::as_str) {
            Some("photo") if !tweets[idx].photos.iter().any(|p| p == url) => tweets[idx].photos.push(url.to_string()),
            Some("photo") | None => {}
            Some(_) => tweets[idx].has_video = true,
        }
    }
    Ok(tweets)
}

/// The newest `MAX_CANDIDATES` posts, newest first.
pub fn newest(mut tweets: Vec<Tweet>) -> Vec<Tweet> {
    tweets.sort_by_key(|t| std::cmp::Reverse(t.id.parse::<u64>().unwrap_or(0)));
    tweets.truncate(MAX_CANDIDATES);
    tweets
}

/// The card title of a photo post: the first line of its text.
pub fn photo_title(text: &str) -> String {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    if line.is_empty() { "Photo post".to_string() } else { line.to_string() }
}

/// A small version of a full-size photo link, for the card thumbnail.
pub fn thumb_url(photo: &str) -> String {
    photo.replace("name=orig", "name=small")
}

/// The most useful error line of gallery-dl's output, if it printed one.
pub fn error_line(stderr: &str) -> Option<String> {
    let line = stderr.lines().map(str::trim).rev().find(|l| l.contains("[error]"))?;
    let msg = line.split_once("[error]").map_or(line, |(_, rest)| rest).trim();
    (!msg.is_empty()).then(|| msg.chars().take(220).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const LISTING: &str = r#"[
      [2, {"author": {"name": "SmeepsX"}, "content": "treat me like a princess", "date": "2026-11-20 20:01:35", "tweet_id": 2123869382409388331}],
      [3, "https://pbs.twimg.com/media/A?format=jpg&name=orig", {"author": {"name": "smeepsx"}, "content": "treat me like a princess", "date": "2026-11-20 20:01:35", "tweet_id": 2123869382409388331, "type": "photo"}],
      [3, "https://pbs.twimg.com/media/B?format=jpg&name=orig", {"author": {"name": "smeepsx"}, "content": "treat me like a princess", "date": "2026-11-20 20:01:35", "tweet_id": 2123869382409388331, "type": "photo"}],
      [2, {"author": {"name": "smeepsx"}, "content": "me and gta6?", "date": "2026-11-20 07:48:50", "tweet_id": 2123684983131045967}],
      [3, "https://video.twimg.com/amplify_video/1/vid/V.mp4?tag=29", {"author": {"name": "smeepsx"}, "content": "me and gta6?", "date": "2026-11-20 07:48:50", "tweet_id": 2123684983131045967, "type": "video"}],
      [3, "https://pbs.twimg.com/media/C?format=jpg&name=orig", {"author": {"name": "someoneelse"}, "content": "not mine", "tweet_id": 5, "type": "photo"}]
    ]"#;

    #[test]
    fn listing_gives_photo_and_video_posts_of_the_user() {
        let tweets = parse_listing(LISTING, "smeepsx").unwrap();
        assert_eq!(tweets.len(), 2);
        let photo = &tweets[0];
        assert_eq!((photo.id.as_str(), photo.has_video, photo.posted_at), ("2123869382409388331", false, Some(1795204895)));
        assert_eq!(photo.photos, ["https://pbs.twimg.com/media/A?format=jpg&name=orig", "https://pbs.twimg.com/media/B?format=jpg&name=orig"]);
        assert_eq!(photo.text, "treat me like a princess");
        let video = &tweets[1];
        assert_eq!((video.id.as_str(), video.has_video, video.photos.is_empty()), ("2123684983131045967", true, true));
    }

    #[test]
    fn empty_or_broken_output_is_handled() {
        assert_eq!(parse_listing("  \n", "a"), Ok(Vec::new()));
        assert_eq!(parse_listing("[]", "a"), Ok(Vec::new()));
        assert!(parse_listing("not json", "a").is_err());
        assert!(parse_listing(r#"{"a":1}"#, "a").is_err());
    }

    #[test]
    fn newest_sorts_by_id_and_caps() {
        let tweets: Vec<Tweet> = (1..=12).map(|n| Tweet { id: n.to_string(), ..Default::default() }).collect();
        let top = newest(tweets);
        assert_eq!((top.len(), top[0].id.as_str(), top[9].id.as_str()), (MAX_CANDIDATES, "12", "3"));
    }

    #[test]
    fn photo_titles_and_thumbnails() {
        assert_eq!(photo_title("  hi\nthere "), "hi");
        assert_eq!(photo_title(""), "Photo post");
        assert_eq!(thumb_url("https://pbs.twimg.com/media/A?format=jpg&name=orig"), "https://pbs.twimg.com/media/A?format=jpg&name=small");
    }

    #[test]
    fn error_line_takes_the_last_error() {
        let stderr = "[cookies][info] Extracted 5 cookies\n[twitter][error] AuthorizationError: nope\n[twitter][error] NotFoundError: gone\n";
        assert_eq!(error_line(stderr).as_deref(), Some("NotFoundError: gone"));
        assert_eq!(error_line("[twitter][info] fine"), None);
    }
}
