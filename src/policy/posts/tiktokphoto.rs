//! Reads the image links of a TikTok photo post from its web page, which yt-dlp cannot download.

use serde_json::Value;
use std::io::Read;
use std::time::Duration;

const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:153.0) Gecko/20100101 Firefox/153.0";
const MAX_PAGE_BYTES: u64 = 8 * 1024 * 1024;
/// Ids of the script tags that hold a post page's data, newest layout first.
const DATA_SCRIPT_IDS: [&str; 2] = ["__UNIVERSAL_DATA_FOR_REHYDRATION__", "SIGI_STATE"];

/// What a post page says about its images.
#[derive(Debug, Clone, PartialEq)]
pub enum PageKind {
    /// A photo post; the links are in slide order.
    Photos(Vec<String>),
    /// A post with no images, such as a video.
    NotPhotos,
}

/// Why an image could not be downloaded.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageError {
    TooLarge,
    Failed(String),
}

/// True for a tiktok.com URL.
pub fn is_tiktok_url(url: &str) -> bool {
    let Some((_, rest)) = url.trim().split_once("://") else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("").to_ascii_lowercase();
    host == "tiktok.com" || host.ends_with(".tiktok.com")
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout_read(Duration::from_secs(20)).build()
}

fn agent_name(user_agent: &Option<String>) -> &str {
    user_agent.as_deref().map(str::trim).filter(|u| !u.is_empty()).unwrap_or(DEFAULT_USER_AGENT)
}

/// Downloads the post's web page.
pub async fn fetch_page(url: String, user_agent: Option<String>) -> Result<String, String> {
    tokio::task::spawn_blocking(move || {
        let resp = agent()
            .get(&url)
            .set("User-Agent", agent_name(&user_agent))
            .set("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .set("Accept-Language", "en-US,en;q=0.9")
            .call()
            .map_err(|e| format!("could not load the post page: {e}"))?;
        let mut buf = Vec::new();
        resp.into_reader().take(MAX_PAGE_BYTES).read_to_end(&mut buf).map_err(|e| format!("could not read the post page: {e}"))?;
        Ok(String::from_utf8_lossy(&buf).into_owned())
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()))
}

/// Downloads one image, failing with `TooLarge` when it is over `max_bytes`.
pub async fn fetch_image(url: String, user_agent: Option<String>, max_bytes: u64) -> Result<Vec<u8>, ImageError> {
    tokio::task::spawn_blocking(move || {
        let resp = agent()
            .get(&url)
            .set("User-Agent", agent_name(&user_agent))
            .set("Referer", "https://www.tiktok.com/")
            .call()
            .map_err(|e| ImageError::Failed(format!("could not download an image: {e}")))?;
        let mut buf = Vec::new();
        resp.into_reader().take(max_bytes.saturating_add(1)).read_to_end(&mut buf).map_err(|e| ImageError::Failed(format!("could not read an image: {e}")))?;
        if buf.len() as u64 > max_bytes {
            return Err(ImageError::TooLarge);
        }
        Ok(buf)
    })
    .await
    .unwrap_or_else(|e| Err(ImageError::Failed(e.to_string())))
}

/// The file extension for image bytes, `None` when they are not a known image format.
pub fn image_ext(bytes: &[u8]) -> Option<&'static str> {
    let at = |from: usize, to: usize| bytes.get(from..to);
    if bytes.starts_with(&[0xFF, 0xD8]) {
        Some("jpg")
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("png")
    } else if at(0, 4) == Some(b"RIFF".as_slice()) && at(8, 12) == Some(b"WEBP".as_slice()) {
        Some("webp")
    } else if at(4, 8) == Some(b"ftyp".as_slice()) {
        Some("heic")
    } else {
        None
    }
}

/// The text inside the page's script tag with this id.
fn script_json<'a>(html: &'a str, id: &str) -> Option<&'a str> {
    let tag = html.find(&format!("id=\"{id}\""))?;
    let start = tag + html[tag..].find('>')? + 1;
    let end = start + html[start..].find("</script>")?;
    Some(&html[start..end])
}

/// The data of post `id` in either page layout.
fn item_struct<'a>(data: &'a Value, id: &str) -> Option<&'a Value> {
    let detail = data
        .pointer("/__DEFAULT_SCOPE__/webapp.video-detail/itemInfo/itemStruct")
        .filter(|item| item.get("id").and_then(Value::as_str).is_none_or(|found| found == id));
    detail.or_else(|| data.get("ItemModule")?.get(id))
}

/// The best link of an image: JPEG first, then WebP, then whatever is listed.
fn best_url<'a>(urls: impl Iterator<Item = &'a str> + Clone) -> Option<String> {
    let path = |u: &str| u.split('?').next().unwrap_or("").to_ascii_lowercase();
    let with_ext = |exts: &[&str]| {
        urls.clone().find(|u| {
            let p = path(*u);
            exts.iter().any(|e| p.ends_with(*e))
        })
    };
    with_ext(&[".jpeg", ".jpg"]).or_else(|| with_ext(&[".webp"])).or_else(|| urls.clone().next()).map(str::to_string)
}

/// The image links of a post's data, empty when it has none.
fn image_urls(item: &Value) -> Vec<String> {
    let images = item.pointer("/imagePost/images").and_then(Value::as_array);
    images
        .into_iter()
        .flatten()
        .filter_map(|image| {
            let list = image.pointer("/imageURL/urlList").or_else(|| image.pointer("/displayImage/urlList"))?.as_array()?;
            best_url(list.iter().filter_map(Value::as_str))
        })
        .collect()
}

/// Reads post `id` from its page; an error when the page holds no data for the post.
pub fn parse_page(html: &str, id: &str) -> Result<PageKind, String> {
    let mut has_data = false;
    for script in DATA_SCRIPT_IDS {
        let Some(data) = script_json(html, script).and_then(|text| serde_json::from_str::<Value>(text).ok()) else { continue };
        has_data = true;
        if let Some(item) = item_struct(&data, id) {
            let urls = image_urls(item);
            return Ok(if urls.is_empty() { PageKind::NotPhotos } else { PageKind::Photos(urls) });
        }
    }
    Err(if has_data { "the page has no data for this post".into() } else { "the page has no post data; TikTok may be asking for a check".into() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(json: &str) -> String {
        format!(r#"<html><script id="__UNIVERSAL_DATA_FOR_REHYDRATION__" type="application/json">{json}</script></html>"#)
    }

    #[test]
    fn tiktok_hosts_are_recognised() {
        assert!(is_tiktok_url("https://www.tiktok.com/@a/video/1"));
        assert!(is_tiktok_url("https://vm.tiktok.com/abc/"));
        assert!(!is_tiktok_url("https://nottiktok.com/@a"));
        assert!(!is_tiktok_url("https://x.com/a/status/1"));
        assert!(!is_tiktok_url("tiktok.com/@a"));
    }

    #[test]
    fn photo_posts_give_their_images_in_order() {
        let json = r#"{"__DEFAULT_SCOPE__":{"webapp.video-detail":{"itemInfo":{"itemStruct":{"id":"7","imagePost":{"images":[
            {"imageURL":{"urlList":["https://c/a.heic?x=1","https://c/a.jpeg?x=1"]}},
            {"imageURL":{"urlList":["https://c/b.webp?x=1"]}}]}}}}}}"#;
        let kind = parse_page(&page(json), "7").unwrap();
        assert_eq!(kind, PageKind::Photos(vec!["https://c/a.jpeg?x=1".into(), "https://c/b.webp?x=1".into()]));
    }

    #[test]
    fn video_posts_have_no_photos() {
        let json = r#"{"__DEFAULT_SCOPE__":{"webapp.video-detail":{"itemInfo":{"itemStruct":{"id":"7","video":{"duration":9}}}}}}"#;
        assert_eq!(parse_page(&page(json), "7"), Ok(PageKind::NotPhotos));
    }

    #[test]
    fn pages_without_the_post_are_errors() {
        assert!(parse_page("<html></html>", "7").is_err());
        let other = r#"{"__DEFAULT_SCOPE__":{"webapp.video-detail":{"itemInfo":{"itemStruct":{"id":"8","imagePost":{"images":[]}}}}}}"#;
        assert!(parse_page(&page(other), "7").is_err());
    }

    #[test]
    fn older_layout_is_read_by_post_id() {
        let json = r#"{"ItemModule":{"7":{"id":"7","imagePost":{"images":[{"imageURL":{"urlList":["https://c/a.jpg"]}}]}}}}"#;
        let html = format!(r#"<script id="SIGI_STATE" type="application/json">{json}</script>"#);
        assert_eq!(parse_page(&html, "7"), Ok(PageKind::Photos(vec!["https://c/a.jpg".into()])));
    }

    #[test]
    fn image_formats_are_told_by_their_bytes() {
        assert_eq!(image_ext(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("jpg"));
        assert_eq!(image_ext(b"\x89PNG\r\n\x1a\n"), Some("png"));
        assert_eq!(image_ext(b"RIFF\x00\x00\x00\x00WEBPVP8 "), Some("webp"));
        assert_eq!(image_ext(b"\x00\x00\x00\x18ftypheic"), Some("heic"));
        assert_eq!(image_ext(b"<html>"), None);
    }
}
