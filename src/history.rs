//! The history contract: what Undo and Redo need to know, whatever the source.
//!
//! app-kit does not depend on any document model. The app implements [`History`] for
//! the type it manages as Tauri state (a document tree behind a mutex, an undo
//! stack) and passes that type to [`AppKit::install`](crate::AppKit::install).

use serde::Serialize;
use tauri::{AppHandle, Runtime};

/// A history source. Implement it on a type the app manages as Tauri state.
///
/// The reads may lock; `refresh_history` calls them, so never call it while holding
/// the same lock.
pub trait History: Send + Sync + 'static {
    /// What Undo would undo, or `None` when there is nothing to undo.
    fn undo_label(&self) -> Option<String>;
    /// What Redo would redo, or `None`.
    fn redo_label(&self) -> Option<String>;
    /// Every undo step, oldest first.
    fn undo_labels(&self) -> Vec<String>;
    /// Every redo step, next first.
    fn redo_labels(&self) -> Vec<String>;
    /// Undo one step. The implementation emits whatever commit event its webview listens to.
    fn undo<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), String>;
    /// Redo one step.
    fn redo<R: Runtime>(&self, app: &AppHandle<R>) -> Result<(), String>;
}

/// The history as the webview reads it: from the `app_kit_history` command, and pushed on
/// every change as the `app-kit://history` event.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryState {
    pub undo_label: Option<String>,
    pub redo_label: Option<String>,
    /// Steps done: the length of `undo_labels`.
    pub history_len: usize,
    /// Oldest first.
    pub undo_labels: Vec<String>,
    /// Next first.
    pub redo_labels: Vec<String>,
}

impl HistoryState {
    pub fn of(history: &impl History) -> Self {
        let undo_labels = history.undo_labels();
        HistoryState {
            undo_label: history.undo_label(),
            redo_label: history.redo_label(),
            history_len: undo_labels.len(),
            undo_labels,
            redo_labels: history.redo_labels(),
        }
    }
}

/// Text and gating for Edit > Undo and Redo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditTitles {
    pub undo: String,
    pub undo_enabled: bool,
    pub redo: String,
    pub redo_enabled: bool,
}

fn title(verb: &str, label: &Option<String>) -> String {
    match label {
        Some(l) => format!("{verb} {l}"),
        None => verb.to_string(),
    }
}

/// "Undo Set Opacity" and enabled when the history has a step; plain "Undo" and always
/// enabled while a text field has focus, because then the item belongs to the field's typing.
pub fn edit_titles(state: &HistoryState, text_focus: bool) -> EditTitles {
    if text_focus {
        return EditTitles {
            undo: "Undo".into(),
            undo_enabled: true,
            redo: "Redo".into(),
            redo_enabled: true,
        };
    }
    EditTitles {
        undo: title("Undo", &state.undo_label),
        undo_enabled: state.undo_label.is_some(),
        redo: title("Redo", &state.redo_label),
        redo_enabled: state.redo_label.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A history that is only labels, to show the contract needs no document model.
    struct Fake(Mutex<(Vec<String>, Vec<String>)>);

    impl Fake {
        fn new(done: &[&str], undone: &[&str]) -> Self {
            let own = |s: &[&str]| s.iter().map(|x| x.to_string()).collect();
            Fake(Mutex::new((own(done), own(undone))))
        }
    }

    impl History for Fake {
        fn undo_label(&self) -> Option<String> {
            self.0.lock().unwrap().0.last().cloned()
        }
        fn redo_label(&self) -> Option<String> {
            self.0.lock().unwrap().1.first().cloned()
        }
        fn undo_labels(&self) -> Vec<String> {
            self.0.lock().unwrap().0.clone()
        }
        fn redo_labels(&self) -> Vec<String> {
            self.0.lock().unwrap().1.clone()
        }
        fn undo<R: Runtime>(&self, _: &AppHandle<R>) -> Result<(), String> {
            Ok(())
        }
        fn redo<R: Runtime>(&self, _: &AppHandle<R>) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn titles_say_what_they_undo() {
        let state = HistoryState::of(&Fake::new(&["Add", "Set Opacity"], &["Reset"]));
        assert_eq!(state.history_len, 2);
        let t = edit_titles(&state, false);
        assert_eq!(t.undo, "Undo Set Opacity");
        assert!(t.undo_enabled);
        assert_eq!(t.redo, "Redo Reset");
        assert!(t.redo_enabled);
    }

    #[test]
    fn an_empty_history_is_plain_and_disabled() {
        let t = edit_titles(&HistoryState::of(&Fake::new(&[], &[])), false);
        assert_eq!((t.undo.as_str(), t.undo_enabled), ("Undo", false));
        assert_eq!((t.redo.as_str(), t.redo_enabled), ("Redo", false));
    }

    #[test]
    fn a_focused_text_field_takes_plain_enabled_items() {
        let t = edit_titles(&HistoryState::of(&Fake::new(&[], &[])), true);
        assert_eq!((t.undo.as_str(), t.undo_enabled), ("Undo", true));
        assert_eq!((t.redo.as_str(), t.redo_enabled), ("Redo", true));
        let t = edit_titles(&HistoryState::of(&Fake::new(&["Add"], &["Reset"])), true);
        assert_eq!(t.undo, "Undo");
        assert_eq!(t.redo, "Redo");
    }
}
