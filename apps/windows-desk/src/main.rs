#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod native;
#[cfg_attr(not(windows), allow(dead_code))]
mod sizing;
#[cfg_attr(not(windows), allow(dead_code))]
mod text_file;

fn main() {
    #[cfg(windows)]
    native::run();
    #[cfg(not(windows))]
    eprintln!("Windows Desk requires Windows.");
}
