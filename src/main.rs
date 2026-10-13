#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#[cfg(windows)] fn main() { craft_app_launcher::ui::run(); }
#[cfg(not(windows))] fn main() { eprintln!("Craft App Launcher requires Windows."); }