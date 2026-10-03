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
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use serde::Deserialize;
use tauri::menu::{
    AboutMetadata, CheckMenuItem, ContextMenu, IsMenuItem, Menu, MenuItem, MenuItemKind,
    PredefinedMenuItem, Submenu,
};
use tauri::{AppHandle, Emitter, Manager, Runtime, State, Window};

use crate::command::{Command, CommandInfo, Kind, MenuName};
use crate::document::{can_revert, Document, DocumentState};
use crate::guard::{
    self, FILE_CLOSE, FILE_NEW, FILE_OPEN, FILE_RECENT, FILE_RECENT_CLEAR, FILE_REVERT, FILE_SAVE,
    FILE_SAVE_AS, QUIT,
};
use crate::history::{edit_titles, HistoryState};
use crate::recent::{self, Recents};

/// Fired at the main window for each command the app does not handle natively. Payload: the id.
pub const COMMAND_EVENT: &str = "command";
/// Fired at the focused window for Undo or Redo while a text field has focus. Payload: "undo" | "redo".
pub const TEXT_UNDO_EVENT: &str = "text-undo";
/// Emitted with a [`HistoryState`] whenever the history is refreshed.
pub const HISTORY_EVENT: &str = "app-kit://history";
/// Emitted at the main window with a [`DocumentState`] whenever the document is refreshed.
pub const DOCUMENT_EVENT: &str = "app-kit://document";

const UNDO: &str = "edit.undo";
const REDO: &str = "edit.redo";
const SETTINGS: &str = "app.settings";

type OnCommand<R> = Box<dyn Fn(&AppHandle<R>, &str) -> bool + Send + Sync>;
type Read<R> = Box<dyn Fn(&AppHandle<R>) -> HistoryState + Send + Sync>;
type Step<R> = Box<dyn Fn(&AppHandle<R>) -> Result<(), String> + Send + Sync>;
type Item<R> = Box<dyn IsMenuItem<R>>;
/// A menu by kind and, for a domain menu, title.
type MenuKey = (MenuName, Option<String>);
type Placed<T> = HashMap<MenuKey, BTreeMap<u8, Vec<T>>>;
type Ask<R> = Box<dyn Fn(&AppHandle<R>) -> bool + Send + Sync>;

/// The app's [`Document`], reached through Tauri state like the history is.
pub(crate) struct DocFns<R: Runtime> {
    pub(crate) state: DocState<R>,
    pub(crate) save: DocPath<R>,
    pub(crate) open: DocPath<R>,
    pub(crate) new_document: DocNew<R>,
}
type DocState<R> = Box<dyn Fn(&AppHandle<R>) -> DocumentState + Send + Sync>;
type DocPath<R> = Box<dyn Fn(&AppHandle<R>, &Path) -> Result<(), String> + Send + Sync>;
type DocNew<R> = Box<dyn Fn(&AppHandle<R>, u32) -> Result<(), String> + Send + Sync>;

/// The managed state behind the commands.
pub struct Kit<R: Runtime> {
    table: Vec<Command>,
    items: Mutex<HashMap<String, MenuItemKind<R>>>,
    /// Whether a text field has focus in the webview (reported through `app_kit_menu_state`).
    text_focus: AtomicBool,
    pub(crate) main_window: String,
    on_command: Option<OnCommand<R>>,
    read: Read<R>,
    undo: Step<R>,
    redo: Step<R>,
    pub(crate) doc: DocFns<R>,
    /// "Ask to save changes when closing", from the app's preferences. None means on.
    pub(crate) ask: Option<Ask<R>>,
    /// The file type the Open and Save panels offer: (name, extension).
    pub(crate) file_type: Option<(String, String)>,
    /// A guard sequence is running (a dialog is up).
    pub(crate) busy: AtomicBool,
    /// The number of the last `Untitled-N` handed out. The first document is `Untitled-1`.
    pub(crate) untitled: AtomicU32,
    /// File > Open Recent's list, and the submenu rebuilt from it.
    pub(crate) recents: Mutex<Recents>,
    recent_menu: Submenu<R>,
}

/// Declares the app's commands and builds the menu bar from them.
pub struct AppKit<R: Runtime> {
    name: String,
    commands: Vec<Command>,
    settings: bool,
    main_window: String,
    on_command: Option<OnCommand<R>>,
    ask: Option<Ask<R>>,
    file_type: Option<(String, String)>,
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
            ask: None,
            file_type: None,
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

    /// Whether to ask before discarding unsaved changes on close: the preference "Ask to save
    /// changes when closing", default on. app-kit does not depend on preferences; the app reads
    /// its own. Off means a document with a file saves itself on close; Untitled still asks.
    pub fn ask_to_save(
        mut self,
        f: impl Fn(&AppHandle<R>) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.ask = Some(Box::new(f));
        self
    }

    /// The file type of the app's documents, for the Open and Save panels: a name for the
    /// filter ("Rhizome document") and the extension without a dot ("rhizome"). The Save panel
    /// adds the extension to a name typed without one.
    pub fn file_type(mut self, name: &str, extension: &str) -> Self {
        self.file_type = Some((name.into(), extension.into()));
        self
    }

    /// Build and set the menu bar, and manage the kit's state. `H` is the app's [`Document`],
    /// already managed as Tauri state. Registers `tauri-plugin-dialog` for the close guard and
    /// the file panels, so the app must not register it too.
    ///
    /// The app also passes [`on_window_event`](crate::on_window_event) to its builder and
    /// [`on_run_event`](crate::on_run_event) to `run`, or the close guard never sees the window close.
    pub fn install<H: Document>(self, app: &AppHandle<R>) -> tauri::Result<()> {
        app.plugin(tauri_plugin_dialog::init())?;
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
        // File: New, Open… and Open Recent, then Close, then Save, Save As… and Revert to
        // Saved…. The app's own File commands (section 1 by default) sit between Open Recent
        // and Close. Revert has no accelerator (HIG) and Rust gates it from the document.
        table.push(file(FILE_NEW, "New", "CmdOrCtrl+N", 0));
        table.push(file(FILE_OPEN, "Open…", "CmdOrCtrl+O", 0));
        table.push(file(FILE_CLOSE, "Close", "CmdOrCtrl+W", 2));
        table.push(file(FILE_SAVE, "Save", "CmdOrCtrl+S", 3));
        table.push(file(FILE_SAVE_AS, "Save As…", "CmdOrCtrl+Shift+S", 3));
        table.push(
            Command::item(FILE_REVERT, "Revert to Saved…")
                .menu(MenuName::File)
                .section(3)
                .disabled(),
        );
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
        let mut placed: Placed<Slot<R>> = HashMap::new();
        let mut domains: Vec<String> = Vec::new();
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
            let domain = (c.menu == MenuName::Domain).then(|| c.domain.clone().unwrap_or_default());
            if let Some(d) = &domain {
                if !domains.contains(d) {
                    domains.push(d.clone());
                }
            }
            let slots = placed
                .entry((c.menu, domain))
                .or_default()
                .entry(c.section)
                .or_default();
            place(slots, c, boxed);
        }
        let mut placed: Placed<Item<R>> = placed
            .into_iter()
            .map(|(k, sections)| {
                let sections = sections
                    .into_iter()
                    .map(|(n, slots)| Ok((n, build_slots(app, slots)?)))
                    .collect::<tauri::Result<_>>()?;
                Ok((k, sections))
            })
            .collect::<tauri::Result<_>>()?;
        // Open Recent goes straight after New and Open…, the first two File items.
        let recent_menu = Submenu::new(app, "Open Recent", true)?;
        let file_first = placed
            .entry((MenuName::File, None))
            .or_default()
            .entry(0)
            .or_default();
        file_first.insert(file_first.len().min(2), Box::new(recent_menu.clone()));
        let mut take = |m: MenuName| placed.remove(&(m, None)).unwrap_or_default();

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
                vec![Box::new(MenuItem::with_id(
                    app,
                    QUIT,
                    format!("Quit {}", self.name),
                    true,
                    Some("CmdOrCtrl+Q"),
                )?)],
            ],
        )?;
        let file_menu = assemble(app, "File", vec![], take(MenuName::File), vec![])?;
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
        let domain_menus = domains
            .into_iter()
            .map(|d| {
                let sections = placed
                    .remove(&(MenuName::Domain, Some(d.clone())))
                    .unwrap_or_default();
                assemble(app, &d, vec![], sections, vec![])
            })
            .collect::<tauri::Result<Vec<_>>>()?;

        let menu = Menu::new(app)?;
        for m in [&app_menu, &file_menu, &edit_menu, &view_menu] {
            menu.append(m)?;
        }
        for m in &domain_menus {
            menu.append(m)?;
        }
        menu.append(&window_menu)?;
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
            doc: DocFns {
                state: Box::new(|app| DocumentState::of(&*app.state::<H>())),
                save: Box::new(|app, path| app.state::<H>().save(app, path)),
                open: Box::new(|app, path| app.state::<H>().open(app, path)),
                new_document: Box::new(|app, n| app.state::<H>().new_document(app, n)),
            },
            ask: self.ask,
            file_type: self.file_type,
            busy: AtomicBool::new(false),
            untitled: AtomicU32::new(1),
            recents: Mutex::new(Recents::load(app)),
            recent_menu,
        });
        app.set_menu(menu)?;
        refresh_recents(app);
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
                id if guard::command(handle, id) => {}
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

/// One entry in a menu section: an item, or a submenu holding items grouped by category.
enum Slot<R: Runtime> {
    Item(Item<R>),
    Sub {
        title: String,
        groups: Vec<(Option<String>, Vec<Item<R>>)>,
    },
}

/// Add a command's item to its section, inside its submenu and category group when it has them.
fn place<R: Runtime>(slots: &mut Vec<Slot<R>>, c: &Command, item: Item<R>) {
    let Some(title) = &c.submenu else {
        slots.push(Slot::Item(item));
        return;
    };
    let at = slots
        .iter()
        .position(|s| matches!(s, Slot::Sub { title: t, .. } if t == title));
    let at = at.unwrap_or_else(|| {
        slots.push(Slot::Sub {
            title: title.clone(),
            groups: Vec::new(),
        });
        slots.len() - 1
    });
    let Slot::Sub { groups, .. } = &mut slots[at] else {
        unreachable!()
    };
    match groups.iter_mut().find(|(cat, _)| *cat == c.category) {
        Some((_, group)) => group.push(item),
        None => groups.push((c.category.clone(), vec![item])),
    }
}

fn build_slots<R: Runtime>(app: &AppHandle<R>, slots: Vec<Slot<R>>) -> tauri::Result<Vec<Item<R>>> {
    slots
        .into_iter()
        .map(|slot| match slot {
            Slot::Item(i) => Ok(i),
            Slot::Sub { title, groups } => {
                let groups = groups.into_iter().map(|(_, g)| g).collect();
                let sub = submenu(app, &title, groups)?;
                Ok(Box::new(sub) as Item<R>)
            }
        })
        .collect()
}

/// A submenu of groups, a separator between non-empty groups.
fn submenu<R: Runtime>(
    app: &AppHandle<R>,
    name: &str,
    groups: Vec<Vec<Item<R>>>,
) -> tauri::Result<Submenu<R>> {
    let menu = Submenu::new(app, name, true)?;
    for (i, group) in groups.into_iter().filter(|g| !g.is_empty()).enumerate() {
        if i > 0 {
            menu.append(&PredefinedMenuItem::separator(app)?)?;
        }
        for item in &group {
            menu.append(item.as_ref())?;
        }
    }
    Ok(menu)
}

/// head, then the app's sections in order, then the tail; a separator between non-empty groups.
fn assemble<R: Runtime>(
    app: &AppHandle<R>,
    name: &str,
    head: Vec<Vec<Item<R>>>,
    sections: BTreeMap<u8, Vec<Item<R>>>,
    tail: Vec<Vec<Item<R>>>,
) -> tauri::Result<Submenu<R>> {
    let groups = head
        .into_iter()
        .chain(sections.into_values())
        .chain(tail)
        .collect();
    submenu(app, name, groups)
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
    refresh_document(app);
}

/// Re-read the document: set the main window's title (`Untitled-1 *`) and emit
/// [`DOCUMENT_EVENT`] for the status bar. [`refresh_history`] calls it, so an app that calls that
/// after every change keeps both current. Same lock rule as `refresh_history`.
pub fn refresh_document<R: Runtime>(app: &AppHandle<R>) {
    let Some(kit) = app.try_state::<Kit<R>>() else {
        return;
    };
    let state = (kit.doc.state)(app);
    if let Some(window) = app.get_webview_window(&kit.main_window) {
        let _ = window.set_title(&state.title);
    }
    {
        let items = kit.items.lock().expect("menu items lock");
        if let Some(item) = items.get(FILE_REVERT).and_then(|i| i.as_menuitem()) {
            let _ = item.set_enabled(can_revert(&state));
        }
    }
    let _ = app.emit_to(kit.main_window.as_str(), DOCUMENT_EVENT, &state);
}

/// Rebuild File > Open Recent from the list: one item per document, newest first, then Clear
/// Menu (disabled when there is nothing to clear).
fn refresh_recents<R: Runtime>(app: &AppHandle<R>) {
    let Some(kit) = app.try_state::<Kit<R>>() else {
        return;
    };
    let paths = kit.recents.lock().expect("recents lock").paths().to_vec();
    let menu = &kit.recent_menu;
    if let Ok(old) = menu.items() {
        for item in old {
            let _ = menu.remove(&item);
        }
    }
    for (n, label) in recent::labels(&paths).into_iter().enumerate() {
        let id = format!("{FILE_RECENT}{n}");
        if let Ok(item) = MenuItem::with_id(app, id, label, true, None::<&str>) {
            let _ = menu.append(&item);
        }
    }
    if !paths.is_empty() {
        if let Ok(sep) = PredefinedMenuItem::separator(app) {
            let _ = menu.append(&sep);
        }
    }
    if let Ok(clear) = MenuItem::with_id(
        app,
        FILE_RECENT_CLEAR,
        "Clear Menu",
        !paths.is_empty(),
        None::<&str>,
    ) {
        let _ = menu.append(&clear);
    }
}

/// Put `path` at the top of File > Open Recent and remember it. app-kit does this itself after
/// Open…, Open Recent and Save; an app calls it when it opens a document some other way (a file
/// handed over by the Finder, or relaunch restore).
pub fn note_recent<R: Runtime>(app: &AppHandle<R>, path: &Path) {
    change_recents(app, |r| r.note(path));
}

/// Take `path` off File > Open Recent, for a file that has gone.
pub fn forget_recent<R: Runtime>(app: &AppHandle<R>, path: &Path) {
    change_recents(app, |r| r.forget(path));
}

/// File > Open Recent > Clear Menu.
pub(crate) fn clear_recents<R: Runtime>(app: &AppHandle<R>) {
    change_recents(app, Recents::clear);
}

/// The recent documents, newest first. The first is the last document, the one relaunch restore reopens.
pub fn recent_documents<R: Runtime>(app: &AppHandle<R>) -> Vec<PathBuf> {
    app.try_state::<Kit<R>>()
        .map(|kit| kit.recents.lock().expect("recents lock").paths().to_vec())
        .unwrap_or_default()
}

fn change_recents<R: Runtime>(app: &AppHandle<R>, f: impl FnOnce(&mut Recents)) {
    let Some(kit) = app.try_state::<Kit<R>>() else {
        return;
    };
    {
        let mut recents = kit.recents.lock().expect("recents lock");
        f(&mut recents);
        recents.store(app);
    }
    refresh_recents(app);
}

fn file(id: &str, label: &str, accelerator: &str, section: u8) -> Command {
    Command::item(id, label)
        .accelerator(accelerator)
        .menu(MenuName::File)
        .section(section)
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

/// The document as it stands: name, path, unsaved, window title.
#[tauri::command]
pub fn app_kit_document<R: Runtime>(app: AppHandle<R>, kit: State<'_, Kit<R>>) -> DocumentState {
    (kit.doc.state)(&app)
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

/// Pops up the native text-field menu at the pointer: Undo, Redo, Cut, Copy, Paste, Select All.
/// Called from `nativeContextMenu` in the webview when a right-click lands in a text field,
/// in place of WebKit's menu (Look Up, Translate, Spelling, Services, Inspect Element...). The
/// predefined items act on the focused field through the responder chain, and macOS enables
/// and disables them itself (Copy with no selection, Paste with an empty pasteboard).
#[tauri::command]
pub fn app_kit_text_menu<R: Runtime>(window: Window<R>) -> Result<(), String> {
    let menu = text_menu(window.app_handle()).map_err(|e| e.to_string())?;
    menu.popup(window).map_err(|e| e.to_string())
}

fn text_menu<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<Menu<R>> {
    Menu::with_items(
        app,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::select_all(app, None)?,
        ],
    )
}
