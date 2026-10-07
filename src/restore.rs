//! Relaunch: the window comes back where it was, and so does the last document, unless the
//! Finder handed the app a file, which wins.
//!
//! Window geometry is `tauri-plugin-window-state`, which has to be on the builder to see the
//! windows the config creates, so the app adds [`window_state`] there. The last document is the
//! first entry of File > Open Recent (`menu-standard.md`, decision 10); [`on_run_event`] reopens
//! it at `RunEvent::Ready`. A file that has gone, or no longer loads, becomes a note for the
//! app to show (`app_kit_document_note`, `useDocumentNotes`), never a dialog.
//!
//! app-kit owns Finder opens for an app with a [`Document`]: `RunEvent::Opened` arrives at
//! [`on_run_event`] too. A POM app without app-kit uses rhizome-pom-tauri's hand-off instead.
//!
//! [`on_run_event`]: crate::on_run_event
//! [`Document`]: crate::Document

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Mutex;

use tauri::plugin::TauriPlugin;
use tauri::{AppHandle, Emitter, Manager, Runtime, Url};
use tauri_plugin_window_state::StateFlags;

use crate::menu::{forget_recent, note_recent, refresh_history, Kit};

/// Pushed to the main window when a note arrives after the webview asked for held ones.
pub const NOTE_EVENT: &str = "app-kit://note";

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

/// Whether relaunch should reopen the last document: only over the fresh Untitled one, and
/// never once the Finder has handed over a file. Decided from state, so it holds whichever
/// arrives first, `Opened` or `Ready`.
pub(crate) fn should_restore(path: Option<&str>, unsaved: bool, finder_opened: bool) -> bool {
    path.is_none() && !unsaved && !finder_opened
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

/// What to do with a note.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Offer {
    /// The webview has asked before: push it as [`NOTE_EVENT`].
    Emit(String),
    /// The webview hasn't asked yet: it gets it when it does.
    Held,
}

#[derive(Default)]
struct NotesInner {
    ready: bool,
    pending: Vec<String>,
}

/// Notes for the webview, which may not be listening yet at launch. Deciding to hold or emit
/// under the same lock as the webview's first ask is what stops a note falling between them.
#[derive(Default)]
pub(crate) struct Notes(Mutex<NotesInner>);

impl Notes {
    pub(crate) fn offer(&self, note: String) -> Offer {
        let mut inner = self.0.lock().expect("notes lock");
        if inner.ready {
            Offer::Emit(note)
        } else {
            inner.pending.push(note);
            Offer::Held
        }
    }

    /// The webview is listening. Returns whatever arrived before it, once.
    pub(crate) fn take(&self) -> Vec<String> {
        let mut inner = self.0.lock().expect("notes lock");
        inner.ready = true;
        std::mem::take(&mut inner.pending)
    }
}

/// Give the app a note to show.
fn note<R: Runtime>(app: &AppHandle<R>, kit: &Kit<R>, text: String) {
    if let Offer::Emit(text) = kit.notes.offer(text) {
        let _ = app.emit_to(kit.main_window.as_str(), NOTE_EVENT, text);
    }
}

fn quoted_name(path: &Path) -> String {
    let name = path
        .file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy();
    format!("\u{201c}{name}\u{201d}")
}

/// Reopen the last document, at launch. Synchronous and without dialogs, so nothing it does
/// can block a Finder open behind the busy flag.
pub(crate) fn restore_last_document<R: Runtime>(app: &AppHandle<R>) {
    let Some(kit) = app.try_state::<Kit<R>>() else {
        return;
    };
    if !kit.has_document {
        return;
    }
    let state = (kit.doc.state)(app);
    let finder = kit.finder_opened.load(Ordering::SeqCst);
    if !should_restore(state.path.as_deref(), state.unsaved, finder) {
        return;
    }
    let Some(path) = kit
        .recents
        .lock()
        .expect("recents lock")
        .paths()
        .first()
        .cloned()
    else {
        return;
    };
    if !path.exists() {
        forget_recent(app, &path);
        note(
            app,
            &kit,
            format!(
                "{} is no longer where it was, so it wasn't reopened.",
                quoted_name(&path)
            ),
        );
        return;
    }
    match (kit.doc.open)(app, &path) {
        Ok(()) => {
            note_recent(app, &path);
            refresh_history(app);
        }
        // It stays in Open Recent: the file is there, and may open once whatever broke is fixed.
        Err(e) => note(
            app,
            &kit,
            format!("{} couldn't be reopened: {e}", quoted_name(&path)),
        ),
    }
}

/// Notes held for the webview, taken once. The webview calls this when it starts listening,
/// through `useDocumentNotes`.
#[tauri::command]
pub fn app_kit_document_note<R: Runtime>(app: AppHandle<R>) -> Vec<String> {
    app.try_state::<Kit<R>>()
        .map(|kit| kit.notes.take())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn restore_only_over_a_fresh_untitled_document() {
        assert!(should_restore(None, false, false));
        assert!(!should_restore(Some("/a/b.x"), false, false));
        assert!(!should_restore(None, true, false));
    }

    #[test]
    fn a_finder_open_wins_whichever_event_comes_first() {
        // Opened first: the flag is set before Ready asks.
        assert!(!should_restore(None, false, true));
        // Ready first: the restored document is clean, so the Finder file replaces it without
        // asking, and a later Ready (there isn't one) would see a path.
        assert!(!should_restore(Some("/a/restored.x"), false, false));
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

    #[test]
    fn notes_wait_for_the_webview_then_go_straight_out() {
        let notes = Notes::default();
        assert_eq!(notes.offer("one".into()), Offer::Held);
        assert_eq!(notes.offer("two".into()), Offer::Held);
        assert_eq!(notes.take(), ["one", "two"]);
        assert_eq!(notes.take(), Vec::<String>::new());
        assert_eq!(notes.offer("three".into()), Offer::Emit("three".into()));
    }
}
