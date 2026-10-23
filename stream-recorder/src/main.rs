#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod api;
mod engine;
mod gui;
mod settings;

fn main() {
    let parts = engine::start();
    gui::install_event_stream(parts.events);
    let client = parts.client.clone();
    let result = gui::run(client.clone());
    if let Err(e) = result {
        eprintln!("GUI error: {e}");
    }
    // Stop recorders gracefully before exiting.
    client.send(api::Command::Shutdown);
    let _ = parts.thread.join();
}
