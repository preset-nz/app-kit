//! The behaviour of a preset.nz desktop app, shared across the family.
//!
//! An app declares its [`Command`]s and plugs in its [`History`]; app-kit builds the
//! native menu bar from them, keeps Edit > Undo and Redo titled and gated from the
//! history, routes Cmd+Z to a focused text field, and hands the command table to the
//! webview so the toolbar never repeats a label or an accelerator.
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
//!         preset_app_kit::app_kit_history,
//!         preset_app_kit::app_kit_undo,
//!         preset_app_kit::app_kit_redo,
//!     ])
//! ```

mod command;
mod history;
mod menu;

pub use command::{display_shortcut, Command, CommandInfo, Kind, MenuName};
pub use history::{edit_titles, EditTitles, History, HistoryState};
pub use menu::{
    app_kit_commands, app_kit_history, app_kit_menu_state, app_kit_redo, app_kit_undo,
    refresh_history, AppKit, CommandState, COMMAND_EVENT, HISTORY_EVENT, TEXT_UNDO_EVENT,
};
