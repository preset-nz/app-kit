//! The menu builder and the Tauri commands the webview talks to.
//!
//! Rule 1 of `guidance/design/native-apps.md`: every command lives in the menu with its
//! accelerator, and a menu item fires an event; the toolbar runs the same command by id.
//!
//! - Undo, Redo and (opt-in) Settings are built in. Undo and Redo run the [`History`] and are
//!   titled and gated from it; while a text field has focus they become plain "Undo" and
//!   "Redo" and fire `text-undo` at the focused window instead of touching the document.
//! - Every other command is offered to the app's `on_command` first (for commands the app
//!   handles natively, such as opening a window), then forwarded to the main window as a
//!   `command` event whose payload is the id.
//! - The webview pushes enabled and checked state back through `app_kit_menu_state`.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Deserialize;
use tauri::menu::{
    AboutMetadata, CheckMenuItem, IsMenuItem, Menu, MenuItem, MenuItemKind, PredefinedMenuItem,
    Submenu,
};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::command::{Command, CommandInfo, Kind, MenuName};
use crate::history::{edit_titles, History, HistoryState};

/// Fired at the main window for each command the app does not handle natively. Payload: the id.
pub const COMMAND_EVENT: &str = "command";
/// Fired at the focused window for Undo or Redo while a text field has focus. Payload: "undo" | "redo".
pub const TEXT_UNDO_EVENT: &str = "text-undo";
/// Emitted with a [`HistoryState`] whenever the history is refreshed.
pub const HISTORY_EVENT: &str = "app-kit://history";

const UNDO: &str = "edit.undo";
const REDO: &str = "edit.redo";
const SETTINGS: &str = "app.settings";

type OnCommand<R> = Box<dyn Fn(&AppHandle<R>, &str) -> bool + Send + Sync>;
type Read<R> = Box<dyn Fn(&AppHandle<R>) -> HistoryState + Send + Sync>;
type Step<R> = Box<dyn Fn(&AppHandle<R>) -> Result<(), String> + Send + Sync>;
type Item<R> = Box<dyn IsMenuItem<R>>;

/// The managed state behind the commands.
pub struct Kit<R: Runtime> {
    table: Vec<Command>,
    items: Mutex<HashMap<String, MenuItemKind<R>>>,
    /// Whether a text field has focus in the webview (reported through `app_kit_menu_state`).
    text_focus: AtomicBool,
    main_window: String,
    on_command: Option<OnCommand<R>>,
    read: Read<R>,
    undo: Step<R>,
    redo: Step<R>,
}

/// Declares the app's commands and builds the menu bar from them.
pub struct AppKit<R: Runtime> {
    name: String,
    commands: Vec<Command>,
    settings: bool,
    main_window: String,
    on_command: Option<OnCommand<R>>,
}

impl<R: Runtime> AppKit<R> {
    /// `name` titles the app menu.
    pub fn new(name: &str) -> Self {
        AppKit {
            name: name.into(),
            commands: Vec::new(),
            settings: false,
            main_window: "main".into(),
            on_command: None,
        }
    }

    pub fn command(mut self, command: Command) -> Self {
        self.commands.push(command);
        self
    }

    pub fn commands(mut self, commands: impl IntoIterator<Item = Command>) -> Self {
        self.commands.extend(commands);
        self
    }

    /// Adds the app menu's Settings… item (`app.settings`, Cmd+,). Opt-in, for apps that have settings.
    pub fn settings(mut self) -> Self {
        self.settings = true;
        self
    }

    /// The window that receives `command` events. Default `main`.
    pub fn main_window(mut self, label: &str) -> Self {
        self.main_window = label.into();
        self
    }

    /// Handle some commands natively. Return true when the id was handled; otherwise it is
    /// forwarded to the webview. Settings and window-opening commands belong here.
    pub fn on_command(
        mut self,
        f: impl Fn(&AppHandle<R>, &str) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.on_command = Some(Box::new(f));
        self
    }

    /// Build and set the menu bar, and manage the kit's state. `H` is the app's [`History`],
    /// already managed as Tauri state.
    pub fn install<H: History>(self, app: &AppHandle<R>) -> tauri::Result<()> {
        let mut table = vec![
            Command::item(UNDO, "Undo")
                .accelerator("CmdOrCtrl+Z")
                .menu(MenuName::Edit)
                .section(0)
                .disabled(),
            Command::item(REDO, "Redo")
                .accelerator("CmdOrCtrl+Shift+Z")
                .menu(MenuName::Edit)
                .section(0)
                .disabled(),
        ];
        if self.settings {
            table.push(
                Command::item(SETTINGS, "Settings…")
                    .accelerator("CmdOrCtrl+,")
                    .menu(MenuName::App)
                    .section(0),
            );
        }
        table.extend(self.commands);

        let mut items = HashMap::new();
        let mut placed: HashMap<MenuName, BTreeMap<u8, Vec<Item<R>>>> = HashMap::new();
        for c in &table {
            let accel = c.accelerator.as_deref();
            let (kind, boxed): (MenuItemKind<R>, Item<R>) = match c.kind {
                Kind::Item => {
                    let i = MenuItem::with_id(app, &c.id, &c.label, c.enabled, accel)?;
                    (MenuItemKind::MenuItem(i.clone()), Box::new(i))
                }
                Kind::Toggle => {
                    let i =
                        CheckMenuItem::with_id(app, &c.id, &c.label, c.enabled, c.checked, accel)?;
                    (MenuItemKind::Check(i.clone()), Box::new(i))
                }
            };
            items.insert(c.id.clone(), kind);
            placed
                .entry(c.menu)
                .or_default()
                .entry(c.section)
                .or_default()
                .push(boxed);
        }
        let mut take = |m: MenuName| placed.remove(&m).unwrap_or_default();

        let about = AboutMetadata {
            name: Some(self.name.clone()),
            version: Some(app.package_info().version.to_string()),
            ..Default::default()
        };

        let app_menu = assemble(
            app,
            &self.name,
            vec![vec![Box::new(PredefinedMenuItem::about(
                app,
                None,
                Some(about),
            )?)]],
            take(MenuName::App),
            vec![
                vec![Box::new(PredefinedMenuItem::services(app, None)?)],
                vec![
                    Box::new(PredefinedMenuItem::hide(app, None)?),
                    Box::new(PredefinedMenuItem::hide_others(app, None)?),
                ],
                vec![Box::new(PredefinedMenuItem::quit(app, None)?)],
            ],
        )?;
        let file_menu = assemble(
            app,
            "File",
            vec![],
            take(MenuName::File),
            vec![vec![Box::new(PredefinedMenuItem::close_window(app, None)?)]],
        )?;
        let edit_menu = assemble(
            app,
            "Edit",
            vec![],
            take(MenuName::Edit),
            vec![vec![
                Box::new(PredefinedMenuItem::cut(app, None)?),
                Box::new(PredefinedMenuItem::copy(app, None)?),
                Box::new(PredefinedMenuItem::paste(app, None)?),
                Box::new(PredefinedMenuItem::select_all(app, None)?),
            ]],
        )?;
        let view_menu = assemble(
            app,
            "View",
            vec![],
            take(MenuName::View),
            vec![vec![Box::new(PredefinedMenuItem::fullscreen(app, None)?)]],
        )?;
        let window_menu = assemble(
            app,
            "Window",
            vec![vec![
                Box::new(PredefinedMenuItem::minimize(app, None)?),
                Box::new(PredefinedMenuItem::maximize(app, None)?),
            ]],
            take(MenuName::Window),
            vec![],
        )?;
        let help = take(MenuName::Help);

        let menu = Menu::new(app)?;
        for m in [&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu] {
            menu.append(m)?;
        }
        if !help.is_empty() {
            menu.append(&assemble(app, "Help", vec![], help, vec![])?)?;
        }

        app.manage(Kit::<R> {
            table,
            items: Mutex::new(items),
            text_focus: AtomicBool::new(false),
            main_window: self.main_window,
            on_command: self.on_command,
            read: Box::new(|app| HistoryState::of(&*app.state::<H>())),
            undo: Box::new(|app| app.state::<H>().undo(app)),
            redo: Box::new(|app| app.state::<H>().redo(app)),
        });
        app.set_menu(menu)?;
        refresh_history(app);

        app.on_menu_event(|handle, event| {
            let id = event.id().0.as_str();
            let kit = handle.state::<Kit<R>>();
            match id {
                UNDO | REDO => {
                    let undo = id == UNDO;
                    if kit.text_focus.load(Ordering::Relaxed) {
                        // A text field has focus: the webview runs execCommand on it.
                        let target = handle
                            .webview_windows()
                            .into_iter()
                            .find(|(_, w)| w.is_focused().unwrap_or(false))
                            .map_or(kit.main_window.clone(), |(label, _)| label);
                        let _ = handle.emit_to(
                            target,
                            TEXT_UNDO_EVENT,
                            if undo { "undo" } else { "redo" },
                        );
                    } else {
                        let _ = if undo {
                            (kit.undo)(handle)
                        } else {
                            (kit.redo)(handle)
                        };
                        refresh_history(handle);
                    }
                }
                id => {
                    let handled = kit.on_command.as_ref().is_some_and(|f| f(handle, id));
                    if !handled {
                        let _ = handle.emit_to(kit.main_window.as_str(), COMMAND_EVENT, id);
                    }
                }
            }
        });
        Ok(())
    }
}

/// head, then the app's sections in order, then the tail; a separator between non-empty groups.
fn assemble<R: Runtime>(
    app: &AppHandle<R>,
    name: &str,
    head: Vec<Vec<Item<R>>>,
    sections: BTreeMap<u8, Vec<Item<R>>>,
    tail: Vec<Vec<Item<R>>>,
) -> tauri::Result<Submenu<R>> {
    let menu = Submenu::new(app, name, true)?;
    let groups = head
        .into_iter()
        .chain(sections.into_values())
        .chain(tail)
        .filter(|g| !g.is_empty());
    for (i, group) in groups.enumerate() {
        if i > 0 {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
        }
        for item in &group {
            menu.append(item.as_ref())?;
        }
    }
    Ok(menu)
}

/// Re-read the history, retitle and regate Edit > Undo / Redo, and emit [`HISTORY_EVENT`].
/// Call it after any change to the document. It reads the history, so do not call it while
/// holding the lock the [`History`] implementation takes.
pub fn refresh_history<R: Runtime>(app: &AppHandle<R>) {
    let Some(kit) = app.try_state::<Kit<R>>() else {
        return;
    };
    let state = (kit.read)(app);
    let titles = edit_titles(&state, kit.text_focus.load(Ordering::Relaxed));
    {
        let items = kit.items.lock().expect("menu items lock");
        for (id, text, enabled) in [
            (UNDO, &titles.undo, titles.undo_enabled),
            (REDO, &titles.redo, titles.redo_enabled),
        ] {
            if let Some(item) = items.get(id).and_then(|i| i.as_menuitem()) {
                let _ = item.set_text(text);
                let _ = item.set_enabled(enabled);
            }
        }
    }
    let _ = app.emit(HISTORY_EVENT, &state);
}

#[derive(Deserialize)]
pub struct CommandState {
    id: String,
    enabled: Option<bool>,
    checked: Option<bool>,
}

/// The command table, labels and accelerators as declared in Rust.
#[tauri::command]
pub fn app_kit_commands<R: Runtime>(
    _app: AppHandle<R>,
    kit: State<'_, Kit<R>>,
) -> Vec<CommandInfo> {
    kit.table.iter().map(Command::info).collect()
}

/// The webview's command state: enabled and checked per command id, and whether a text field
/// has focus. Undo and redo are not sent; they follow the history.
#[tauri::command]
pub fn app_kit_menu_state<R: Runtime>(
    app: AppHandle<R>,
    kit: State<'_, Kit<R>>,
    states: Vec<CommandState>,
    text_focus: Option<bool>,
) {
    if let Some(t) = text_focus {
        kit.text_focus.store(t, Ordering::Relaxed);
        refresh_history(&app);
    }
    let items = kit.items.lock().expect("menu items lock");
    for s in states {
        match items.get(&s.id) {
            Some(MenuItemKind::MenuItem(i)) => {
                if let Some(e) = s.enabled {
                    let _ = i.set_enabled(e);
                }
            }
            Some(MenuItemKind::Check(i)) => {
                if let Some(e) = s.enabled {
                    let _ = i.set_enabled(e);
                }
                if let Some(c) = s.checked {
                    let _ = i.set_checked(c);
                }
            }
            _ => {}
        }
    }
}

/// The history as it stands.
#[tauri::command]
pub fn app_kit_history<R: Runtime>(app: AppHandle<R>, kit: State<'_, Kit<R>>) -> HistoryState {
    (kit.read)(&app)
}

/// Undo one step: what a toolbar Undo runs, the same step the menu item takes.
#[tauri::command]
pub fn app_kit_undo<R: Runtime>(app: AppHandle<R>, kit: State<'_, Kit<R>>) -> Result<(), String> {
    let out = (kit.undo)(&app);
    refresh_history(&app);
    out
}

#[tauri::command]
pub fn app_kit_redo<R: Runtime>(app: AppHandle<R>, kit: State<'_, Kit<R>>) -> Result<(), String> {
    let out = (kit.redo)(&app);
    refresh_history(&app);
    out
}
