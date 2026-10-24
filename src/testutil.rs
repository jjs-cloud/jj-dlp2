//! Shared helpers for tests that run against a fake yt-dlp.

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

const FAKE: &str = r#"#!/bin/bash
case "$1" in
  --dump-json)
    for u in "$@"; do
      if [[ $u == *live1* ]]; then
        echo "{\"id\":\"vid1\",\"title\":\"T\",\"is_live\":true,\"webpage_url\":\"https://x.test/watch/vid1\",\"original_url\":\"$u\"}"
      fi
    done
    exit 1;;
  RECORD)
    if [[ -z "$BOOT" ]]; then BOOT=1 "$0" "$@" & wait; exit $?; fi
    "$(dirname "$0")/ffmpeg" 300 & FF=$!
    trap 'kill $FF 2>/dev/null; sleep 1; exit 0' INT TERM
    wait;;
esac
"#;

/// Fake yt-dlp whose checker reports offline and broken channels and whose recorder fails on missing ffmpeg.
pub const FAKE_FAILING: &str = r#"#!/bin/bash
case "$1" in
  --dump-json)
    for u in "$@"; do
      case "$u" in
        *offline*) echo "ERROR: [fake:stream] offline: The channel is not currently live" >&2;;
        *broken*) echo "ERROR: [fake:stream] broken: Something unexpected" >&2;;
        *live*) echo "{\"id\":\"vid1\",\"title\":\"T\",\"is_live\":true,\"webpage_url\":\"https://x.test/watch/vid1\",\"original_url\":\"$u\"}";;
      esac
    done
    exit 1;;
  RECORD)
    echo "ERROR: m3u8 download detected but ffmpeg could not be found. Please install" >&2
    echo "ERROR: Unable to download video: [Errno 2] No such file or directory: 'x.mp4.part'" >&2
    exit 1;;
esac
"#;

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Temp directory with a fake yt-dlp and ffmpeg, and the config dir pointed inside it.
pub struct Sandbox {
    pub dir: PathBuf,
    pub script: PathBuf,
    _guard: MutexGuard<'static, ()>,
}

impl Sandbox {
    pub fn new(tag: &str) -> Self {
        Self::with_script(tag, FAKE)
    }

    pub fn with_script(tag: &str, script_text: &str) -> Self {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("jj-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", dir.join("cfg"));
            std::env::set_var("HOME", &dir);
        }
        let script = dir.join("fake-ytdlp");
        std::fs::write(&script, script_text).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::copy("/bin/sleep", dir.join("ffmpeg")).unwrap();
        Self { dir, script, _guard: guard }
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
