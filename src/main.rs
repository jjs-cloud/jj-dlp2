#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod api;
mod engine;
mod gui;
mod instance;
mod policy;
mod settings;
#[cfg(all(test, unix))]
mod testutil;

use tokio::sync::mpsc::UnboundedReceiver;

const USAGE: &str = "Usage: jj-dlp [--no-gui | --engine-only | --update [--rc]]\n  (default)     engine + policy + GUI\n  --no-gui      engine + policy, no window\n  --engine-only engine only, reads the config files at launch\n  --update      install the latest release and exit (--rc includes release candidates)";

enum Mode {
    Gui,
    NoGui,
    EngineOnly,
    Update(bool),
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        [] => Mode::Gui,
        ["--no-gui"] => Mode::NoGui,
        ["--engine-only"] => Mode::EngineOnly,
        ["--update"] => Mode::Update(false),
        ["--update", "--rc"] | ["--rc", "--update"] => Mode::Update(true),
        ["-h" | "--help"] => {
            attach_console();
            println!("{USAGE}");
            return;
        }
        _ => {
            attach_console();
            eprintln!("{USAGE}");
            std::process::exit(2);
        }
    };
    let _instance = if matches!(mode, Mode::Update(_)) {
        None
    } else {
        match instance::acquire() {
            Ok(lock) => Some(lock),
            Err(instance::AlreadyRunning) => {
                if matches!(mode, Mode::Gui) {
                    instance::show_running_dialog();
                } else {
                    attach_console();
                    eprintln!("{}", instance::MESSAGE);
                }
                std::process::exit(1);
            }
        }
    };
    match mode {
        Mode::Gui => run_gui(),
        Mode::NoGui => run_no_gui(),
        Mode::EngineOnly => run_engine_only(),
        Mode::Update(rc) => run_update(rc),
    }
}

fn run_gui() {
    let policy = policy::start(engine::start());
    gui::install_event_stream(policy.events);
    if let Err(e) = gui::run(policy.client.clone()) {
        eprintln!("GUI error: {e}");
    }
    // Stop recorders gracefully before exiting.
    policy.client.send(policy::PolicyCommand::Shutdown);
    let _ = policy.thread.join();
}

fn run_no_gui() {
    attach_console();
    let policy = policy::start(engine::start());
    let client = policy.client.clone();
    drive(
        policy.events,
        |ev| match ev {
            policy::PolicyEvent::Engine(api::Event::Log(m)) | policy::PolicyEvent::Log(m) => println!("{m}"),
            policy::PolicyEvent::Update(policy::UpdateEvent::Available { version, is_rc }) => {
                let kind = if is_rc { " (release candidate)" } else { "" };
                println!("Update available: v{version}{kind}. Run `jj-dlp --update` to install it.");
            }
            _ => {}
        },
        move || client.send(policy::PolicyCommand::Shutdown),
        policy.thread,
    );
}

fn run_update(rc: bool) {
    attach_console();
    std::process::exit(policy::update::run_cli(rc));
}

fn run_engine_only() {
    attach_console();
    let engine = engine::start();
    let client = engine.client.clone();
    drive(
        engine.events,
        |ev| {
            if let api::Event::Log(m) = ev {
                println!("{m}");
            }
        },
        move || client.send(api::Command::Shutdown),
        engine.thread,
    );
}

/// Handles events until `thread` ends; the first Ctrl-C calls `on_interrupt`.
fn drive<T>(
    mut rx: UnboundedReceiver<T>,
    mut on_event: impl FnMut(T),
    on_interrupt: impl Fn(),
    thread: std::thread::JoinHandle<()>,
) {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().expect("tokio runtime");
    rt.block_on(async {
        let mut interrupted = false;
        let mut finished = tokio::task::spawn_blocking(move || {
            let _ = thread.join();
        });
        loop {
            tokio::select! {
                ev = rx.recv() => match ev {
                    Some(ev) => on_event(ev),
                    None => break,
                },
                _ = &mut finished => break,
                _ = tokio::signal::ctrl_c(), if !interrupted => {
                    interrupted = true;
                    println!("shutting down...");
                    on_interrupt();
                }
            }
        }
        while let Ok(ev) = rx.try_recv() {
            on_event(ev);
        }
    });
}

/// Attaches to the parent console so headless output shows in the terminal on Windows.
fn attach_console() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}
