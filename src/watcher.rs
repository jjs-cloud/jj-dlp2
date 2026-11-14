//! Watcher process: runs the app as a child process and relaunches it if it dies unexpectedly.

use crate::{crash, instance};
use std::ffi::OsString;
use std::process::{Child, Command, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// Set in the child process so it knows a watcher is supervising it.
pub const CHILD_ENV: &str = "JJ_DLP_CHILD";

/// Exit code the child uses when the GUI failed, so the watcher relaunches it.
pub const EXIT_GUI_ERROR: i32 = 70;

/// Wait before the first relaunch; doubles after each quick failure.
const FIRST_DELAY: Duration = Duration::from_secs(2);
/// Longest wait between relaunches.
const MAX_DELAY: Duration = Duration::from_secs(60);
/// A run this long counts as healthy and resets the relaunch delay.
const STABLE_AFTER: Duration = Duration::from_secs(120);
/// How long the child gets to shut down after the watcher is asked to stop.
const STOP_GRACE: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(250);

static STOP: AtomicBool = AtomicBool::new(false);

/// True in the process the watcher launched.
pub fn is_child() -> bool {
    std::env::var_os(CHILD_ENV).is_some()
}

/// Runs the app as a child and relaunches it until it exits on purpose; returns the process exit code.
pub fn run(gui: bool) -> i32 {
    let _lock = match instance::acquire_watcher() {
        Ok(lock) => lock,
        Err(instance::AlreadyRunning) => {
            if gui {
                instance::show_running_dialog();
            } else {
                crate::attach_console();
                eprintln!("{}", instance::MESSAGE);
            }
            return 1;
        }
    };
    if !gui {
        crate::attach_console();
    }
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(e) => {
            crash::note(&format!("Watcher cannot locate jj-dlp: {e}"));
            return 1;
        }
    };
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    install_stop_handlers();

    let mut delay = FIRST_DELAY;
    loop {
        let started = Instant::now();
        let status = match Command::new(&exe).args(&args).env(CHILD_ENV, "1").spawn() {
            Ok(mut child) => match wait_child(&mut child) {
                Some(status) => Some(status),
                None => return 0,
            },
            Err(e) => {
                crash::note(&format!("Watcher could not start jj-dlp: {e}"));
                None
            }
        };
        if let Some(status) = &status {
            if !should_restart(status) {
                return status.code().unwrap_or(0);
            }
        }
        if started.elapsed() >= STABLE_AFTER {
            delay = FIRST_DELAY;
        }
        if let Some(status) = &status {
            crash::note(&format!(
                "Watcher: jj-dlp exited unexpectedly ({}) after {}s; restarting in {}s.",
                describe(status),
                started.elapsed().as_secs(),
                delay.as_secs()
            ));
        }
        if sleep_or_stop(delay) {
            return 0;
        }
        delay = (delay * 2).min(MAX_DELAY);
    }
}

/// Waits for the child to exit; returns `None` if the watcher was asked to stop and the child has been shut down.
fn wait_child(child: &mut Child) -> Option<ExitStatus> {
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Some(status);
        }
        if STOP.load(Ordering::SeqCst) {
            stop_child(child);
            return None;
        }
        std::thread::sleep(POLL);
    }
}

/// Asks the child to stop, then kills it if it has not exited within `STOP_GRACE`.
fn stop_child(child: &mut Child) {
    #[cfg(unix)]
    unsafe {
        libc::kill(child.id() as i32, libc::SIGINT);
    }
    let deadline = Instant::now() + STOP_GRACE;
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(POLL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Sleeps for `total`; returns true if the watcher was asked to stop meanwhile.
fn sleep_or_stop(total: Duration) -> bool {
    let end = Instant::now() + total;
    while Instant::now() < end {
        if STOP.load(Ordering::SeqCst) {
            return true;
        }
        std::thread::sleep(POLL);
    }
    STOP.load(Ordering::SeqCst)
}

/// True unless the child finished normally, found another instance running, or was stopped by the user.
fn should_restart(status: &ExitStatus) -> bool {
    match status.code() {
        Some(code) => code != 0 && code != 1,
        None => !stopped_by_user(status),
    }
}

#[cfg(unix)]
fn stopped_by_user(status: &ExitStatus) -> bool {
    use std::os::unix::process::ExitStatusExt;
    matches!(status.signal(), Some(libc::SIGINT | libc::SIGTERM | libc::SIGHUP))
}

#[cfg(not(unix))]
fn stopped_by_user(_status: &ExitStatus) -> bool {
    false
}

/// A short description of how the child ended, for crash.log.
fn describe(status: &ExitStatus) -> String {
    if let Some(code) = status.code() {
        return format!("exit code {code} / 0x{:08X}", code as u32);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("signal {signal}");
        }
    }
    "unknown exit status".to_string()
}

#[cfg(unix)]
extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

/// Makes Ctrl-C and termination signals stop the child gracefully instead of killing the watcher outright.
fn install_stop_handlers() {
    #[cfg(unix)]
    unsafe {
        let handler = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::signal(signal, handler);
        }
    }
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Console::SetConsoleCtrlHandler(Some(on_ctrl), 1);
    }
}

#[cfg(windows)]
unsafe extern "system" fn on_ctrl(kind: u32) -> i32 {
    use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT};
    if kind == CTRL_C_EVENT || kind == CTRL_BREAK_EVENT {
        STOP.store(true, Ordering::SeqCst);
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn status(raw: i32) -> ExitStatus {
        std::os::unix::process::ExitStatusExt::from_raw(raw)
    }

    #[cfg(windows)]
    fn status(raw: i32) -> ExitStatus {
        std::os::windows::process::ExitStatusExt::from_raw(raw as u32)
    }

    #[test]
    fn clean_and_already_running_exits_are_not_restarted() {
        #[cfg(unix)]
        {
            assert!(!should_restart(&status(0)));
            assert!(!should_restart(&status(1 << 8)));
        }
        #[cfg(windows)]
        {
            assert!(!should_restart(&status(0)));
            assert!(!should_restart(&status(1)));
        }
    }

    #[test]
    fn failures_are_restarted() {
        #[cfg(unix)]
        {
            assert!(should_restart(&status(101 << 8)));
            assert!(should_restart(&status(libc::SIGSEGV)));
            assert!(should_restart(&status(libc::SIGKILL)));
            assert!(!should_restart(&status(libc::SIGINT)));
            assert!(!should_restart(&status(libc::SIGTERM)));
        }
        #[cfg(windows)]
        {
            assert!(should_restart(&status(101)));
            assert!(should_restart(&status(0xC000_0005_u32 as i32)));
        }
    }
}
