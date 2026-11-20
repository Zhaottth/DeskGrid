#![cfg_attr(all(windows, not(test)), windows_subsystem = "windows")]

#[macro_use]
mod i18n;
mod directory;
mod layout;

#[cfg(windows)]
mod desktop;

#[cfg(windows)]
fn main() {
    if let Err(error) = desktop::run() {
        desktop::report_error(&error.to_string());
        std::process::exit(1);
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("DeskGrid requires Windows.");
}
