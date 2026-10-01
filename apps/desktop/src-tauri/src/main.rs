//! `rimmerge-desktop` binary entry point: hands off to
//! `rimmerge_desktop_lib::run` immediately, so the actual application is
//! testable as a library.

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    rimmerge_desktop_lib::run();
}
