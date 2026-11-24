#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Os adaptadores são ligados à UI no grupo 4.
#[allow(dead_code)]
mod infrastructure;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    AppWindow::new()?.run()
}
