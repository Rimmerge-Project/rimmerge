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

    /// Whether a permission entry grants an identifier starting with
    /// `prefix`. Tauri accepts a bare string or, for scoped grants, an
    /// object `{"identifier": "...", "allow": [...]}`; both are read.
    fn grants_prefix(permission: &Value, prefix: &str) -> bool {
        permission
            .as_str()
            .or_else(|| permission["identifier"].as_str())
            .is_some_and(|identifier| identifier.starts_with(prefix))
    }

    #[test]
    fn a_scoped_object_entry_is_read_by_its_identifier() {
        let scoped = serde_json::json!({"identifier": "shell:allow-spawn", "allow": []});
        let plain = serde_json::json!("shell:allow-open");
        let unrelated = serde_json::json!({"identifier": "dialog:allow-open"});

        assert!(grants_prefix(&scoped, "shell:"));
        assert!(grants_prefix(&plain, "shell:"));
        assert!(!grants_prefix(&unrelated, "shell:"));
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

    /// Exporting a load order opens the native save dialog
    /// (`@tauri-apps/plugin-dialog`'s `save()`), which the webview may only
    /// call with `dialog:allow-save`; `dialog:allow-open` does not cover it,
    /// and a denied call is silently swallowed. The command that then
    /// writes the file is Rust's: the webview gets no filesystem plugin.
    #[test]
    fn every_capability_grants_the_save_dialog_and_no_filesystem_permission() {
        for (name, capability) in every_capability() {
            let permissions = capability["permissions"]
                .as_array()
                .unwrap_or_else(|| panic!("{name}: permissions is an array"));

            assert!(
                permissions
                    .iter()
                    .any(|permission| permission == "dialog:allow-save"),
                "{name} must grant dialog:allow-save — the Export menu's save dialog \
                 is denied without it"
            );
            assert!(
                permissions
                    .iter()
                    .all(|permission| !grants_prefix(permission, "fs:")),
                "{name} must never grant an fs:* permission — export_order_file \
                 writes the chosen path from Rust"
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
                permissions
                    .iter()
                    .all(|permission| !grants_prefix(permission, "opener:")),
                "{name} must never grant an opener:* permission — \
                 every link this app opens goes through open_mod_link/open_app_link, \
                 which call tauri_plugin_opener::open_url directly from Rust, never \
                 through the plugin's own JS-callable IPC surface"
            );
        }
    }

    /// Launching RimWorld is the `launch_game` command: Rust derives the
    /// route, the executable path and the `steam://` URL, and the frontend
    /// sends one enum. A `shell:*` permission would let the webview start
    /// any program or open any URL itself, bypassing all of that.
    #[test]
    fn every_capability_grants_no_shell_permission() {
        for (name, capability) in every_capability() {
            let permissions = capability["permissions"]
                .as_array()
                .unwrap_or_else(|| panic!("{name}: permissions is an array"));

            assert!(
                permissions
                    .iter()
                    .all(|permission| !grants_prefix(permission, "shell:")),
                "{name} must never grant a shell:* permission — \
                 launching RimWorld goes through launch_game, which derives the \
                 program and URL in Rust from the install folder"
            );
        }
    }
}
