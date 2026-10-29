//! Single-instance guard backed by an OS file lock in the config dir.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

pub const MESSAGE: &str = "jj-dlp is already running.";

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
}
