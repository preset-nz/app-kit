//! The document contract: what saving, the unsaved mark and the close guard need to know.
//!
//! Like [`History`], this is a trait the app implements on the type it manages as Tauri state,
//! and app-kit depends on no document model. The playground's implementation is over a rhizome
//! `Tree` (`serialise`, `load`, `mark_saved`, `is_unsaved`); another app's can be anything that
//! writes a file and knows whether it differs from it.
//!
//! One document, held by the main window. Several documents is a later change that touches
//! this trait and [`History`] together (both would take a document id).

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::MessageDialogResult;

use crate::history::History;

/// The open document. A [`History`] that can also be saved.
///
/// Reads may lock; `refresh_history` calls them, so never call it while holding the same lock.
pub trait Document: History {
    /// The document differs from what is on disk. Derived, never a flag: undoing back to the
    /// saved state clears it (rhizome's `Tree::is_unsaved`).
    fn is_unsaved(&self) -> bool;
    /// The file it was opened from or saved to, or `None` for an Untitled document.
    fn path(&self) -> Option<PathBuf>;
    /// Which `Untitled-N` this is. Read only while `path` is `None`.
    fn untitled_number(&self) -> u32;
    /// Write the document to `path`, then record it as saved and remember the path.
    /// Failing must leave the document unsaved.
    fn save<R: Runtime>(&self, app: &AppHandle<R>, path: &Path) -> Result<(), String>;
    /// Replace the document with the file at `path`: saved, with an empty history. The
    /// implementation emits whatever commit event its webview listens to.
    fn open<R: Runtime>(&self, app: &AppHandle<R>, path: &Path) -> Result<(), String>;
    /// Replace the document with a new, empty one, numbered `Untitled-{untitled}`. app-kit
    /// counts: the first document is `Untitled-1`, so New gives 2, then 3.
    fn new_document<R: Runtime>(&self, app: &AppHandle<R>, untitled: u32) -> Result<(), String>;
}

/// The document as the webview reads it: from the `app_kit_document` command, and pushed on
/// every change as the `app-kit://document` event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentState {
    /// `Untitled-1` or the file's name.
    pub name: String,
    pub path: Option<String>,
    pub unsaved: bool,
    /// The window title: the name, then ` *` while unsaved.
    pub title: String,
}

impl DocumentState {
    pub fn of(doc: &impl Document) -> Self {
        let path = doc.path();
        let name = display_name(path.as_deref(), doc.untitled_number());
        let unsaved = doc.is_unsaved();
        DocumentState {
            title: window_title(&name, unsaved),
            name,
            path: path.map(|p| p.to_string_lossy().into_owned()),
            unsaved,
        }
    }
}

/// `report.rhizome` for a file, `Untitled-1` for none.
pub fn display_name(path: Option<&Path>, untitled: u32) -> String {
    path.and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| format!("Untitled-{untitled}"))
}

/// The window title: `Untitled-1 *` while unsaved, plain otherwise.
pub fn window_title(name: &str, unsaved: bool) -> String {
    if unsaved {
        format!("{name} *")
    } else {
        name.to_string()
    }
}

/// What closing (or quitting, or replacing) the document needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseAction {
    /// Nothing to lose: go ahead.
    Close,
    /// Show the Save / Don't Save / Cancel dialog.
    Ask,
    /// Save to its file, then go ahead. Only for a document that has a file.
    AutoSave,
}

/// The close decision, as a pure function (ux-patterns.md, "Unsaved changes on close").
///
/// - A saved document closes.
/// - With "Ask to save changes when closing" on, an unsaved one asks.
/// - With it off, an unsaved document that has a file saves automatically, and an Untitled one
///   still asks, because it has nowhere to go. Nothing is ever discarded without asking.
pub fn close_action(unsaved: bool, has_path: bool, ask_setting: bool) -> CloseAction {
    match (unsaved, ask_setting, has_path) {
        (false, _, _) => CloseAction::Close,
        (true, true, _) | (true, false, false) => CloseAction::Ask,
        (true, false, true) => CloseAction::AutoSave,
    }
}

pub(crate) const SAVE: &str = "Save";
pub(crate) const DONT_SAVE: &str = "Don't Save";
pub(crate) const CANCEL: &str = "Cancel";

/// The user's answer to the dialog.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Save,
    DontSave,
    Cancel,
}

/// Reads the dialog's result. macOS gives back the custom label; other platforms may give
/// Yes / No / Cancel. Anything unrecognised, including a dismissed dialog, is Cancel: the safe answer.
pub fn choice_of(result: &MessageDialogResult) -> Choice {
    match result {
        MessageDialogResult::Yes | MessageDialogResult::Ok => Choice::Save,
        MessageDialogResult::No => Choice::DontSave,
        MessageDialogResult::Custom(s) if s == SAVE => Choice::Save,
        MessageDialogResult::Custom(s) if s == DONT_SAVE => Choice::DontSave,
        _ => Choice::Cancel,
    }
}

pub(crate) const REVERT: &str = "Revert";

/// Whether Revert to Saved… may go ahead. Only the Revert button says yes (or Ok, where a
/// platform gives back the kind rather than the label); anything else, a dismissed dialog
/// included, keeps the changes.
pub fn revert_confirmed(result: &MessageDialogResult) -> bool {
    match result {
        MessageDialogResult::Ok | MessageDialogResult::Yes => true,
        MessageDialogResult::Custom(s) => s == REVERT,
        _ => false,
    }
}

/// Revert to Saved… is offered only for an unsaved document that has a file to go back to.
pub fn can_revert(state: &DocumentState) -> bool {
    state.unsaved && state.path.is_some()
}

/// The file name for the Save panel, with the extension the app declared.
pub(crate) fn suggested_file_name(name: &str, extension: Option<&str>) -> String {
    match extension {
        Some(ext) if !name.ends_with(&format!(".{ext}")) => format!("{name}.{ext}"),
        _ => name.to_string(),
    }
}

/// `path` with `extension` added when it has none (a name typed into the Save panel).
pub(crate) fn with_extension(path: PathBuf, extension: Option<&str>) -> PathBuf {
    match extension {
        Some(ext) if path.extension().is_none() => path.with_extension(ext),
        _ => path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_document_just_closes() {
        for ask in [true, false] {
            for path in [true, false] {
                assert_eq!(close_action(false, path, ask), CloseAction::Close);
            }
        }
    }

    #[test]
    fn unsaved_asks_when_the_setting_is_on() {
        assert_eq!(close_action(true, true, true), CloseAction::Ask);
        assert_eq!(close_action(true, false, true), CloseAction::Ask);
    }

    #[test]
    fn with_the_setting_off_a_file_saves_itself() {
        assert_eq!(close_action(true, true, false), CloseAction::AutoSave);
    }

    #[test]
    fn with_the_setting_off_untitled_still_asks() {
        assert_eq!(close_action(true, false, false), CloseAction::Ask);
    }

    #[test]
    fn the_dialog_result_maps_by_label_or_by_kind() {
        let custom = |s: &str| MessageDialogResult::Custom(s.into());
        assert_eq!(choice_of(&custom("Save")), Choice::Save);
        assert_eq!(choice_of(&custom("Don't Save")), Choice::DontSave);
        assert_eq!(choice_of(&custom("Cancel")), Choice::Cancel);
        assert_eq!(choice_of(&MessageDialogResult::Yes), Choice::Save);
        assert_eq!(choice_of(&MessageDialogResult::No), Choice::DontSave);
        assert_eq!(choice_of(&MessageDialogResult::Cancel), Choice::Cancel);
        assert_eq!(choice_of(&custom("something else")), Choice::Cancel);
    }

    #[test]
    fn revert_goes_ahead_only_on_revert() {
        assert!(revert_confirmed(&MessageDialogResult::Custom(
            "Revert".into()
        )));
        assert!(revert_confirmed(&MessageDialogResult::Ok));
        assert!(!revert_confirmed(&MessageDialogResult::Custom(
            "Cancel".into()
        )));
        assert!(!revert_confirmed(&MessageDialogResult::Cancel));
    }

    #[test]
    fn revert_needs_unsaved_changes_and_a_file() {
        let state = |unsaved, path: Option<&str>| DocumentState {
            name: "a".into(),
            path: path.map(Into::into),
            unsaved,
            title: "a".into(),
        };
        assert!(can_revert(&state(true, Some("/a.x"))));
        assert!(!can_revert(&state(false, Some("/a.x"))));
        assert!(!can_revert(&state(true, None)));
    }

    #[test]
    fn names_and_titles() {
        assert_eq!(display_name(None, 1), "Untitled-1");
        assert_eq!(display_name(None, 12), "Untitled-12");
        assert_eq!(
            display_name(Some(Path::new("/tmp/notes.rhizome")), 3),
            "notes.rhizome"
        );
        assert_eq!(window_title("Untitled-1", true), "Untitled-1 *");
        assert_eq!(window_title("notes.rhizome", true), "notes.rhizome *");
        assert_eq!(window_title("notes.rhizome", false), "notes.rhizome");
    }

    #[test]
    fn the_save_panel_gets_the_declared_extension() {
        assert_eq!(
            suggested_file_name("Untitled-1", Some("rhizome")),
            "Untitled-1.rhizome"
        );
        assert_eq!(
            suggested_file_name("a.rhizome", Some("rhizome")),
            "a.rhizome"
        );
        assert_eq!(suggested_file_name("Untitled-1", None), "Untitled-1");
        assert_eq!(
            with_extension(PathBuf::from("/x/a"), Some("rhizome")),
            PathBuf::from("/x/a.rhizome")
        );
        assert_eq!(
            with_extension(PathBuf::from("/x/a.txt"), Some("rhizome")),
            PathBuf::from("/x/a.txt")
        );
    }

    struct Fake {
        path: Option<PathBuf>,
        unsaved: bool,
    }
    impl History for Fake {
        fn undo_label(&self) -> Option<String> {
            None
        }
        fn redo_label(&self) -> Option<String> {
            None
        }
        fn undo_labels(&self) -> Vec<String> {
            vec![]
        }
        fn redo_labels(&self) -> Vec<String> {
            vec![]
        }
        fn undo<R: Runtime>(&self, _: &AppHandle<R>) -> Result<(), String> {
            Ok(())
        }
        fn redo<R: Runtime>(&self, _: &AppHandle<R>) -> Result<(), String> {
            Ok(())
        }
    }
    impl Document for Fake {
        fn is_unsaved(&self) -> bool {
            self.unsaved
        }
        fn path(&self) -> Option<PathBuf> {
            self.path.clone()
        }
        fn untitled_number(&self) -> u32 {
            2
        }
        fn save<R: Runtime>(&self, _: &AppHandle<R>, _: &Path) -> Result<(), String> {
            Ok(())
        }
        fn open<R: Runtime>(&self, _: &AppHandle<R>, _: &Path) -> Result<(), String> {
            Ok(())
        }
        fn new_document<R: Runtime>(&self, _: &AppHandle<R>, _: u32) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn state_carries_the_name_and_the_marked_title() {
        let s = DocumentState::of(&Fake {
            path: None,
            unsaved: true,
        });
        assert_eq!(
            (s.name.as_str(), s.title.as_str()),
            ("Untitled-2", "Untitled-2 *")
        );
        assert!(s.unsaved && s.path.is_none());
        let s = DocumentState::of(&Fake {
            path: Some("/a/b.rhizome".into()),
            unsaved: false,
        });
        assert_eq!(
            (s.name.as_str(), s.title.as_str()),
            ("b.rhizome", "b.rhizome")
        );
        assert_eq!(s.path.as_deref(), Some("/a/b.rhizome"));
    }
}
