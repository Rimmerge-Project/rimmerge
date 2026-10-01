//! Tauri build script: generates the context Tauri embeds into the
//! binary from `tauri.conf.json` and the capabilities directory.

fn main() {
    tauri_build::build();
}
