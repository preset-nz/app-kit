//! The file commands, the close guard and the quit guard.
//!
//! Every path that can discard the document (closing the window, Cmd+Q, New, Open) goes through
//! [`confirm_discard`]: the decision is [`close_action`], the dialog is native
//! (`tauri-plugin-dialog`, three custom buttons), and Cancel at any step, the Save panel
//! included, stops the whole thing.
//!
//! The dialogs block, so the sequence runs on its own thread, one at a time (`exclusive`):
//! Cmd+W or Cmd+Q while a dialog is up does nothing rather than stacking a second one.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use tauri::{AppHandle, Manager, RunEvent, Runtime, Window, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::document::{
    choice_of, close_action, suggested_file_name, with_extension, Choice, CloseAction, CANCEL,
    DONT_SAVE, SAVE,
};
use crate::menu::{refresh_history, Kit};

pub(crate) const FILE_NEW: &str = "file.new";
pub(crate) const FILE_OPEN: &str = "file.open";
pub(crate) const FILE_SAVE: &str = "file.save";
pub(crate) const FILE_SAVE_AS: &str = "file.save_as";
pub(crate) const FILE_CLOSE: &str = "file.close";
pub(crate) const QUIT: &str = "app.quit";

/// Clears the busy flag when the sequence ends, however it ends.
struct Busy<R: Runtime>(AppHandle<R>);

impl<R: Runtime> Drop for Busy<R> {
    fn drop(&mut self) {
        self.0.state::<Kit<R>>().busy.store(false, Ordering::SeqCst);
    }
}

/// Run `f` on a thread, unless a sequence is already running.
fn exclusive<R: Runtime>(app: &AppHandle<R>, f: impl FnOnce(&AppHandle<R>) + Send + 'static) {
    if app.state::<Kit<R>>().busy.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let busy = Busy(app.clone());
        f(&app);
        drop(busy);
    });
}

fn ask_setting<R: Runtime>(app: &AppHandle<R>, kit: &Kit<R>) -> bool {
    kit.ask.as_ref().is_none_or(|f| f(app))
}

fn main_window_exists<R: Runtime>(app: &AppHandle<R>, kit: &Kit<R>) -> bool {
    app.get_webview_window(&kit.main_window).is_some()
}

/// Whether discarding the document now needs the guard. Cheap and synchronous, so the
/// close and exit handlers can decide whether to prevent the default.
fn needs_confirm<R: Runtime>(app: &AppHandle<R>, kit: &Kit<R>) -> bool {
    if !main_window_exists(app, kit) {
        return false;
    }
    let state = (kit.doc.state)(app);
    close_action(state.unsaved, state.path.is_some(), ask_setting(app, kit)) != CloseAction::Close
}

fn error_dialog<R: Runtime>(app: &AppHandle<R>, title: &str, message: &str) {
    app.dialog()
        .message(message)
        .title(title)
        .kind(MessageDialogKind::Error)
        .blocking_show();
}

/// Save the document. With a file and no `force_panel` it writes there; otherwise the Save
/// panel asks. Returns false when the panel was cancelled or the write failed (the failure is
/// shown), so a caller never goes on to discard what was not saved.
fn save_current<R: Runtime>(app: &AppHandle<R>, kit: &Kit<R>, force_panel: bool) -> bool {
    let state = (kit.doc.state)(app);
    let ext = kit.file_type.as_ref().map(|(_, e)| e.as_str());
    let existing = if force_panel {
        None
    } else {
        state.path.clone().map(PathBuf::from)
    };
    let path = match existing {
        Some(p) => p,
        None => {
            let mut panel = app
                .dialog()
                .file()
                .set_file_name(suggested_file_name(&state.name, ext));
            if let Some((name, ext)) = &kit.file_type {
                panel = panel.add_filter(name, &[ext.as_str()]);
            }
            if let Some(dir) = state.path.as_deref().and_then(|p| Path::new(p).parent()) {
                panel = panel.set_directory(dir);
            }
            match panel.blocking_save_file().and_then(|p| p.into_path().ok()) {
                Some(p) => with_extension(p, ext),
                None => return false,
            }
        }
    };
    match (kit.doc.save)(app, &path) {
        Ok(()) => {
            refresh_history(app);
            true
        }
        Err(e) => {
            error_dialog(app, "Could not save", &e);
            false
        }
    }
}

/// May the document be discarded? Blocking: call it off the main thread. True means go
/// ahead (nothing to lose, saved, auto-saved, or Don't Save); false means stop.
fn confirm_discard<R: Runtime>(app: &AppHandle<R>) -> bool {
    let kit = app.state::<Kit<R>>();
    if !main_window_exists(app, &kit) {
        return true;
    }
    let state = (kit.doc.state)(app);
    match close_action(state.unsaved, state.path.is_some(), ask_setting(app, &kit)) {
        CloseAction::Close => true,
        CloseAction::AutoSave => save_current(app, &kit, false),
        CloseAction::Ask => {
            let result = app
                .dialog()
                .message("Your changes will be lost if you don't save them.")
                .title(format!(
                    "Do you want to save the changes you made to \u{201c}{}\u{201d}?",
                    state.name
                ))
                .kind(MessageDialogKind::Warning)
                .buttons(MessageDialogButtons::YesNoCancelCustom(
                    SAVE.into(),
                    DONT_SAVE.into(),
                    CANCEL.into(),
                ))
                .blocking_show_with_result();
            match choice_of(&result) {
                Choice::Save => save_current(app, &kit, false),
                Choice::DontSave => true,
                Choice::Cancel => false,
            }
        }
    }
}

/// Run a built-in command (File menu, Quit). Returns false for any other id.
pub(crate) fn command<R: Runtime>(app: &AppHandle<R>, id: &str) -> bool {
    match id {
        FILE_NEW => exclusive(app, |app| {
            if !confirm_discard(app) {
                return;
            }
            let kit = app.state::<Kit<R>>();
            let n = kit.untitled.fetch_add(1, Ordering::SeqCst) + 1;
            match (kit.doc.new_document)(app, n) {
                Ok(()) => refresh_history(app),
                Err(e) => error_dialog(app, "Could not create a document", &e),
            }
        }),
        FILE_OPEN => exclusive(app, |app| {
            if !confirm_discard(app) {
                return;
            }
            let kit = app.state::<Kit<R>>();
            let mut panel = app.dialog().file();
            if let Some((name, ext)) = &kit.file_type {
                panel = panel.add_filter(name, &[ext.as_str()]);
            }
            let Some(path) = panel.blocking_pick_file().and_then(|p| p.into_path().ok()) else {
                return;
            };
            match (kit.doc.open)(app, &path) {
                Ok(()) => refresh_history(app),
                Err(e) => error_dialog(app, "Could not open the file", &e),
            }
        }),
        FILE_SAVE => exclusive(app, |app| {
            save_current(app, &app.state::<Kit<R>>(), false);
        }),
        FILE_SAVE_AS => exclusive(app, |app| {
            save_current(app, &app.state::<Kit<R>>(), true);
        }),
        FILE_CLOSE => {
            // The focused window closes; the main window's close is guarded by `on_window_event`.
            let kit = app.state::<Kit<R>>();
            let target = app
                .webview_windows()
                .into_values()
                .find(|w| w.is_focused().unwrap_or(false))
                .or_else(|| app.get_webview_window(&kit.main_window));
            if let Some(w) = target {
                let _ = w.close();
            }
        }
        QUIT => quit(app),
        _ => return false,
    }
    true
}

/// Quit, after the guard. With one document it asks once.
fn quit<R: Runtime>(app: &AppHandle<R>) {
    exclusive(app, |app| {
        if confirm_discard(app) {
            app.exit(0);
        }
    });
}

/// Pass the app's window events here: the main window's close button, Cmd+W and
/// File > Close are held while the guard runs, then the window is destroyed.
///
/// ```ignore
/// tauri::Builder::default().on_window_event(preset_app_kit::on_window_event)
/// ```
pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    let app = window.app_handle();
    let Some(kit) = app.try_state::<Kit<R>>() else {
        return;
    };
    if window.label() != kit.main_window || !needs_confirm(app, &kit) {
        return;
    }
    api.prevent_close();
    let window = window.clone();
    exclusive(app, move |app| {
        // `destroy`, not `close`: close would raise CloseRequested again.
        if confirm_discard(app) {
            let _ = window.destroy();
        }
    });
}

/// Pass the app's run events here, for quits that do not come through the menu (the Dock's
/// Quit, logging out). A quit the app asked for itself (`exit(0)`) carries a code and is let through.
///
/// ```ignore
/// builder.build(tauri::generate_context!())?.run(|app, event| preset_app_kit::on_run_event(app, &event));
/// ```
pub fn on_run_event<R: Runtime>(app: &AppHandle<R>, event: &RunEvent) {
    let RunEvent::ExitRequested {
        code: None, api, ..
    } = event
    else {
        return;
    };
    let Some(kit) = app.try_state::<Kit<R>>() else {
        return;
    };
    // After the main window is destroyed (Don't Save, then the last window closing) there is
    // nothing left to ask about, so this lets the exit through.
    if !needs_confirm(app, &kit) {
        return;
    }
    api.prevent_exit();
    quit(app);
}
