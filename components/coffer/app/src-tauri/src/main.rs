//! Coffer shell (scaffold). The shell is deliberately thin: it starts
//! coffer-server when it finds it beside the executable, then opens one
//! system-webview window onto the loopback server. All product behaviour
//! lives in the server and the core, never in the shell, so the engine
//! stays borrowed and swappable.
//!
//! v0 tab model (design determination 1): one live webview; tabs are
//! kept as suspended states in the UI layer until multi-webview support
//! in Tauri/Wry is mature enough to trust.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::{Child, Command};
use std::sync::Mutex;

use tauri::Manager;

struct ServerChild(Mutex<Option<Child>>);

fn spawn_server() -> Option<Child> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let server = dir.join("coffer-server");
    if !server.exists() {
        eprintln!("coffer: coffer-server not found beside the shell; expecting an external one");
        return None;
    }
    let shelf = std::env::var("COFFER_SHELF").unwrap_or_else(|_| "shelf".to_string());
    Command::new(server)
        .arg("--shelf")
        .arg(shelf)
        .spawn()
        .ok()
}

fn main() {
    let child = ServerChild(Mutex::new(spawn_server()));
    tauri::Builder::default()
        .manage(child)
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                if let Some(state) = window.try_state::<ServerChild>() {
                    if let Some(mut child) = state.0.lock().unwrap().take() {
                        let _ = child.kill();
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running coffer");
}
