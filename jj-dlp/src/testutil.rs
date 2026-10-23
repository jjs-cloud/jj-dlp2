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

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Temp directory with a fake yt-dlp and ffmpeg, and the config dir pointed inside it.
pub struct Sandbox {
    pub dir: PathBuf,
    pub script: PathBuf,
    _guard: MutexGuard<'static, ()>,
}

impl Sandbox {
    pub fn new(tag: &str) -> Self {
        let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("jj-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", dir.join("cfg"));
            std::env::set_var("HOME", &dir);
        }
        let script = dir.join("fake-ytdlp");
        std::fs::write(&script, FAKE).unwrap();
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
