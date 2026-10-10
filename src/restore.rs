//! Relaunch: the window comes back where it was, and the app starts on a fresh Untitled
//! document. The last document is not reopened; it is the first entry of File > Open Recent
//! (`menu-standard.md`, decision 10).
//!
//! Window geometry is `tauri-plugin-window-state`, which has to be on the builder to see the
//! windows the config creates, so the app adds [`window_state`] there.
//!
//! app-kit owns Finder opens for an app with a [`Document`]: `RunEvent::Opened` arrives at
//! [`on_run_event`] too. A POM app without app-kit uses rhizome-pom-tauri's hand-off instead.
//!
//! [`on_run_event`]: crate::on_run_event
//! [`Document`]: crate::Document

use std::path::PathBuf;

use tauri::plugin::TauriPlugin;
use tauri::{Runtime, Url};
use tauri_plugin_window_state::StateFlags;

/// The window-state plugin, set up the family's way. Add it to the builder, not in `setup`:
/// the plugin restores a window as it is created, and the config's windows already exist by then.
///
/// ```ignore
/// tauri::Builder::default().plugin(preset_app_kit::window_state())
/// ```
///
/// Size, position, maximised and full screen. Not visibility: a window hidden at quit would
/// come back hidden.
pub fn window_state<R: Runtime>() -> TauriPlugin<R> {
    tauri_plugin_window_state::Builder::new()
        .with_state_flags(flags())
        .build()
}

fn flags() -> StateFlags {
    StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED | StateFlags::FULLSCREEN
}

/// The document among the URLs macOS sent: the last local file, with the app's extension when
/// it has one. A folder, a web link or another app's file is not ours to open.
// Called only from macOS's open-from-Finder handler; the tests run everywhere.
#[cfg_attr(not(any(target_os = "macos", target_os = "ios")), allow(dead_code))]
pub(crate) fn finder_path(urls: &[Url], extension: Option<&str>) -> Option<PathBuf> {
    urls.iter()
        .filter(|u| u.scheme() == "file")
        .filter_map(|u| u.to_file_path().ok())
        .rfind(|p| match extension {
            Some(ext) => p
                .extension()
                .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case(ext)),
            None => true,
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn finder_takes_the_last_local_file_with_the_extension() {
        let urls = [
            url("file:///a/one.rhizome"),
            url("https://example.com/two.rhizome"),
            url("file:///a/notes.txt"),
            url("file:///a/three.RHIZOME"),
            url("file:///a/folder/"),
        ];
        assert_eq!(
            finder_path(&urls, Some("rhizome")),
            Some(PathBuf::from("/a/three.RHIZOME"))
        );
        assert_eq!(finder_path(&urls[1..3], Some("rhizome")), None);
        assert_eq!(
            finder_path(&urls[..3], None),
            Some(PathBuf::from("/a/notes.txt"))
        );
    }
}

/// Kept so apps built against 0.1 still compile: nothing sends notes since launch stopped
/// reopening the last document. Remove it from `generate_handler!` when convenient.
pub const NOTE_EVENT: &str = "app-kit://note";

/// Kept so apps built against 0.1 still compile. Always empty, for the reason on [`NOTE_EVENT`].
#[tauri::command]
pub fn app_kit_document_note() -> Vec<String> {
    Vec::new()
}
