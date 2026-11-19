#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    if photo_viewer_advanced_lib::run_cli() { return; }
    photo_viewer_advanced_lib::run()
}

