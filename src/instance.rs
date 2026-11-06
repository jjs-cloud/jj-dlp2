//! Single-instance guard backed by an OS file lock in the config dir.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;
use std::time::{Duration, Instant};

pub const MESSAGE: &str = "jj-dlp is already running.";

/// Set on a relaunched process so it waits for the previous instance to release the lock.
pub const RELAUNCH_ENV: &str = "JJ_DLP_RELAUNCH";

/// Returned when another instance holds the lock.
#[derive(Debug)]
pub struct AlreadyRunning;

/// Holds the lock until dropped; the OS also releases it if the process dies.
pub struct InstanceLock(#[allow(dead_code)] Option<File>);

/// Takes the instance lock, or reports that another instance is running.
pub fn acquire() -> Result<InstanceLock, AlreadyRunning> {
    let dir = crate::settings::config_dir();
    let _ = std::fs::create_dir_all(&dir);
    acquire_at(&dir.join("instance.lock"))
}

/// Like `acquire`, but keeps retrying for up to `wait` while the previous instance exits.
pub fn acquire_waiting(wait: Duration) -> Result<InstanceLock, AlreadyRunning> {
    let dir = crate::settings::config_dir();
    let _ = std::fs::create_dir_all(&dir);
    acquire_at_waiting(&dir.join("instance.lock"), wait)
}

fn acquire_at_waiting(path: &Path, wait: Duration) -> Result<InstanceLock, AlreadyRunning> {
    let deadline = Instant::now() + wait;
    loop {
        match acquire_at(path) {
            Err(AlreadyRunning) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            other => return other,
        }
    }
}

fn acquire_at(path: &Path) -> Result<InstanceLock, AlreadyRunning> {
    let Ok(file) = OpenOptions::new().create(true).write(true).truncate(false).open(path) else {
        return Ok(InstanceLock(None));
    };
    match file.try_lock() {
        Ok(()) => Ok(InstanceLock(Some(file))),
        Err(TryLockError::WouldBlock) => Err(AlreadyRunning),
        Err(TryLockError::Error(_)) => Ok(InstanceLock(None)),
    }
}

/// Tells the user another instance is running (message box on Windows, stderr elsewhere).
pub fn show_running_dialog() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_OK, MessageBoxW};
        let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
        MessageBoxW(std::ptr::null_mut(), wide(MESSAGE).as_ptr(), wide("jj-dlp").as_ptr(), MB_OK | MB_ICONINFORMATION);
    }
    #[cfg(not(windows))]
    eprintln!("{MESSAGE}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_acquire_is_rejected_until_first_is_dropped() {
        let path = std::env::temp_dir().join(format!("jj-dlp-test-{}.lock", std::process::id()));
        let first = acquire_at(&path).expect("first lock");
        assert!(acquire_at(&path).is_err());
        drop(first);
        assert!(acquire_at(&path).is_ok());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn waiting_acquire_succeeds_once_the_holder_releases() {
        let path = std::env::temp_dir().join(format!("jj-dlp-test-wait-{}.lock", std::process::id()));
        let first = acquire_at(&path).expect("first lock");
        let releaser = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            drop(first);
        });
        assert!(acquire_at_waiting(&path, Duration::from_secs(10)).is_ok());
        releaser.join().unwrap();
        let held = acquire_at(&path).expect("lock");
        assert!(acquire_at_waiting(&path, Duration::from_millis(200)).is_err());
        drop(held);
        let _ = std::fs::remove_file(&path);
    }
}
