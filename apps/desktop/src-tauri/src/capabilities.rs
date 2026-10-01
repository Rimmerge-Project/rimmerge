//! Regression coverage for `capabilities/default.json`.
//!
//! `PendingChangesCloseGuard.vue` registers a `getCurrentWindow()`
//! `onCloseRequested` listener. Once any close-requested listener is
//! registered, Tauri v2 stops closing the window itself on the native X:
//! the `@tauri-apps/api` wrapper runs the JS handler, then calls
//! `window.destroy()` from JavaScript when the handler didn't prevent the
//! close. That `destroy()` IPC call is denied unless
//! `core:window:allow-destroy` is granted — `core:window:default` (part of
//! `core:default`) does not include it — and a denied call is silently
//! swallowed, so the window never closes. This test catches a regression
//! where that permission line is removed or renamed.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde_json::Value;

    /// Every `capabilities/*.json` file, read at test time (not
    /// `include_str!`'d — a compile-time embed names one file, and a
    /// second capability file added later would silently sit outside
    /// both checks below unless this loop covers it too).
    fn every_capability() -> Vec<(String, Value)> {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("capabilities");
        let mut files: Vec<(String, Value)> = fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()))
            .map(|entry| entry.unwrap_or_else(|error| panic!("read dir entry: {error}")))
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "json"))
            .map(|entry| {
                let path = entry.path();
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_else(|| panic!("non-UTF-8 file name: {}", path.display()))
                    .to_string();
                let text = fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
                let value: Value = serde_json::from_str(&text).unwrap_or_else(|error| {
                    panic!("{} is not valid JSON: {error}", path.display())
                });
                (name, value)
            })
            .collect();
        assert!(
            !files.is_empty(),
            "capabilities/ must contain at least one .json file"
        );
        files.sort_by(|a, b| a.0.cmp(&b.0));
        files
    }

    #[test]
    fn every_capability_grants_window_destroy() {
        for (name, capability) in every_capability() {
            let permissions = capability["permissions"]
                .as_array()
                .unwrap_or_else(|| panic!("{name}: permissions is an array"));

            assert!(
                permissions
                    .iter()
                    .any(|permission| permission == "core:window:allow-destroy"),
                "{name} must grant core:window:allow-destroy — \
                 PendingChangesCloseGuard.vue's onCloseRequested listener makes Tauri \
                 route every window close through a JS `destroy()` call, which this \
                 permission is what allows"
            );
        }
    }

    /// The mod info panel's workshop/homepage links and the app's own
    /// fixed external links (About section, sidebar support link) all go
    /// through `open_mod_link`/`open_app_link` — Rust-only commands that
    /// call `tauri_plugin_opener::open_url` directly, never through the
    /// plugin's own IPC surface. This pins that the frontend gets no
    /// `opener:*` permission of its own: a later "just call `openUrl`
    /// from JS" change (or registering `.plugin(tauri_plugin_opener::init())`
    /// with a granted permission) would let a compromised or buggy
    /// webview open an arbitrary URL directly, bypassing every one of
    /// this app's own closed link-target enums.
    #[test]
    fn every_capability_grants_no_opener_permission() {
        for (name, capability) in every_capability() {
            let permissions = capability["permissions"]
                .as_array()
                .unwrap_or_else(|| panic!("{name}: permissions is an array"));

            assert!(
                permissions.iter().all(|permission| !permission
                    .as_str()
                    .is_some_and(|p| p.starts_with("opener:"))),
                "{name} must never grant an opener:* permission — \
                 every link this app opens goes through open_mod_link/open_app_link, \
                 which call tauri_plugin_opener::open_url directly from Rust, never \
                 through the plugin's own JS-callable IPC surface"
            );
        }
    }
}
