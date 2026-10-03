//! The behaviour of a preset.nz desktop app, shared across the family.
//!
//! An app declares its [`Command`]s and plugs in its [`Document`] (a [`History`] that can be
//! saved); app-kit builds the
//! native menu bar from them, keeps Edit > Undo and Redo titled and gated from the
//! history, routes Cmd+Z to a focused text field, and hands the command table to the
//! webview so the toolbar never repeats a label or an accelerator. It also owns the File
//! commands, the unsaved mark in the window title and the status bar, and the close and quit
//! guard (a native Save / Don't Save / Cancel dialog). The app passes [`on_window_event`] to
//! its builder and [`on_run_event`] to `run`.
//!
//! Like `preset-preferences` this is not a Tauri plugin: the app registers the
//! commands listed in [`menu`] in its own `generate_handler!`, so no capability
//! entries are needed.
//!
//! ```ignore
//! tauri::Builder::default()
//!     .manage(MyDoc::new())
//!     .setup(|app| {
//!         AppKit::<tauri::Wry>::new("My App")
//!             .settings()
//!             .command(Command::item("doc.reset", "Reset").accelerator("CmdOrCtrl+Alt+R").menu(MenuName::Edit))
//!             .install::<MyDoc>(app.handle())?;
//!         Ok(())
//!     })
//!     .invoke_handler(tauri::generate_handler![
//!         preset_app_kit::app_kit_commands,
//!         preset_app_kit::app_kit_menu_state,
//!         preset_app_kit::app_kit_document,
//!         preset_app_kit::app_kit_history,
//!         preset_app_kit::app_kit_undo,
//!         preset_app_kit::app_kit_redo,
//!         preset_app_kit::app_kit_text_menu,
//!     ])
//! ```

mod command;
mod document;
mod guard;
mod history;
mod menu;

pub use command::{display_shortcut, shortcut, Command, CommandInfo, Kind, MenuName};
pub use document::{
    choice_of, close_action, display_name, window_title, Choice, CloseAction, Document,
    DocumentState,
};
pub use guard::{on_run_event, on_window_event};
pub use history::{edit_titles, EditTitles, History, HistoryState};
pub use menu::{
    app_kit_commands, app_kit_document, app_kit_history, app_kit_menu_state, app_kit_redo,
    app_kit_text_menu, app_kit_undo, refresh_document, refresh_history, AppKit, CommandState,
    COMMAND_EVENT, DOCUMENT_EVENT, HISTORY_EVENT, TEXT_UNDO_EVENT,
};
