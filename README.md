# app-kit

The behaviour of a desktop app, shared across the preset.nz family: one command table feeding the native menu and the toolbar, Undo and Redo that say what they undo, Cmd+Z that never fights a text field, and a small selection store. [`ux-kit`](https://github.com/preset-nz/ux-kit) holds the look; app-kit holds how the app acts.

An app declares its commands in Rust and plugs in its history. app-kit builds the menu bar, keeps Undo and Redo titled and gated, and hands the same table to the webview, so a label or an accelerator is written once.

Two packages, one repo, one version:

| Path | Package | What it is |
|---|---|---|
| `src/` | `preset-app-kit` (crate) | Command table, menu builder, `History` trait, Tauri commands |
| `src-ts/` | `@preset.nz/app-kit` (npm) | `useCommands`, toolbar bridge, history hook, text-field undo, selection store |

**Status:** 0.0.1, private. Not published; the crate has `publish = false` and the npm package `private: true`. The first consumer is the ux-kit playground, by path. Crate and npm versions move together, and the TypeScript types mirror the serde shape by hand.

app-kit depends on ux-kit (the toolbar bridge renders `ToolbarItems`). ux-kit never depends on app-kit. Neither depends on rhizome: each app adapts its own history to the trait.

## Rust

```rust
use preset_app_kit::{AppKit, Command, History, MenuName};

impl History for Doc { /* undo_label, redo_label, undo_labels, redo_labels, undo, redo */ }

tauri::Builder::default()
    .manage(Doc::new())
    .setup(|app| {
        AppKit::<tauri::Wry>::new("My App")
            .settings() // opt in: Settings… with Cmd+,
            .commands([
                Command::toggle("panel.left", "Show Outline")
                    .accelerator("CmdOrCtrl+Alt+S")
                    .toolbar_label("Outline")
                    .menu(MenuName::View)
                    .section(0),
                Command::item("doc.reset", "Reset").accelerator("CmdOrCtrl+Alt+R").disabled(),
            ])
            .on_command(|app, id| match id { "app.settings" => { /* open it */ true } _ => false })
            .install::<Doc>(app.handle())?; // Doc is the managed History
        Ok(())
    })
    .invoke_handler(tauri::generate_handler![
        preset_app_kit::app_kit_commands,
        preset_app_kit::app_kit_menu_state,
        preset_app_kit::app_kit_history,
        preset_app_kit::app_kit_undo,
        preset_app_kit::app_kit_redo,
        preset_app_kit::app_kit_text_menu,
    ])
```

- **`Command`**: id, menu label, optional toolbar label, Tauri accelerator, `MenuName` (App, File, Edit, View, Window, Help), `section` (items in a section sit together), item or toggle. Undo and Redo are built in; Settings is opt-in.
- **File menu**: New, Open…, Open Recent ▸, Close, Save, Save As… and Revert to Saved… are built in, run against the app's `Document` behind the close guard. Revert is enabled only for an unsaved document with a file, and asks first, because reloading starts an empty history. Open Recent keeps ten documents, newest first, in `recent-documents.json` in the app's data directory; a file that has gone says so and leaves the list. `note_recent(app, path)` adds a document opened some other way (from the Finder, or by restore), and `recent_documents(app)[0]` is the last document. `.documents_folder(|app| …)` names where the Open and Save panels start for an Untitled document (`~/preset-nz/<App>/…`); the app supplies the lookup, so app-kit doesn't depend on app-folders.
- **Menu config**: `.menu_config(include_str!("../menu.toml"))` switches the built-in items on and off: `[file] new, open, open_recent, close, save, save_as, revert` and `[app] settings`. A missing key takes the default (on); an unknown key is an error, so add a test that calls `MenuConfig::parse` on your file. Labels, accelerators and positions stay the standard's. Design: guidance `projects/app-kit/design/menu-config.md`.
- **Apps without documents**: `install_history::<H: History>` instead of `install::<H: Document>`, for a library such as Strata. The config must turn New, Open, Open Recent, Save, Save As and Revert off; with Close off too, Window gains Close Window on Cmd+W. No close guard, no window-title handling, and it doesn't register `tauri-plugin-dialog`, so the app keeps its own.
- **`History`**: `undo_label`, `redo_label`, `undo_labels` (oldest first), `redo_labels` (next first), `undo`, `redo`. Implement it on the type you manage as Tauri state. The reads may lock, so call `refresh_history(app)` after a change and never while holding that lock.
- **`edit_titles`**: the pure rule behind Edit > Undo: "Undo Set Opacity" when there is a step, plain and disabled when there is none, plain and enabled while a text field has focus.
- **Events**: `command` (id, to the main window), `text-undo` (to the focused window), `app-kit://history` (a `HistoryState`).
- Commands are plain Tauri commands, not a plugin, so no capability entries are needed.

## TypeScript

```tsx
import { CommandToolbar, useCommands, nativeContextMenu } from "@preset.nz/app-kit"

const { commands, run, shortcut } = useCommands(bindings) // bindings: Record<id, { icon, run, enabled, pressed, ... }>
<CommandToolbar commands={commands} layout={{ leading: ["panel.left"], groups: [["edit.undo", "edit.redo"]] }} run={run} />
```

- **`useCommands(bindings)`**: loads the table from Rust, binds `run`, icon and gating by id, follows the history, routes text-field undo and keeps the menu's enabled and checked state in step. `edit.undo` and `edit.redo` need only an icon. `bindCommands`, `useCommandTable`, `useMenuSync`, `toItems` are the parts.
- **`CommandToolbar`**: ux-kit's `ToolbarItems` fed from the commands.
- **`@preset.nz/app-kit/core`**: everything except `CommandToolbar` and `toItems`, with no ux-kit import, for apps that use only the menu half. ux-kit is an optional peer.
- **`useHistory`, `undo`, `redo`**: the live `HistoryState`, and the same undo the menu takes. The Inspector's History list reads `undoLabels` and `redoLabels`.
- **`useTextUndo`, `useTextFocus`, `isTextField`, `blurField`**: Cmd+Z undoes typing while a text field has focus, the document otherwise.
- **`createSelection<T>(none)`**: single-select store with `get`, `use`, `select`, `clear`. **`createMultiSelection<T>(key)`**: the opt-in multi-select. **`createStore`, `useStore`**: the small store under both. Wrap them in your own narrow setters; there is no `setSelection`, and selection is not an undo step.
- **`nativeContextMenu()`**: no WebKit context menu. Right-click in a text field pops up a short native menu (Undo, Redo, Cut, Copy, Paste, Select All; needs `app_kit_text_menu` registered); elsewhere nothing, and app-drawn menus still open.

The table arrives from Rust, so a plain browser tab (`just dev-web` in the playground) has no commands and an empty toolbar.

## Consuming it

```toml
# src-tauri/Cargo.toml
preset-app-kit = "0.1"
```

```sh
pnpm add @preset.nz/app-kit
```

`@preset.nz/app-kit/core` is everything except the toolbar, for an app that doesn't use ux-kit; ux-kit is an optional peer.

In Vite, add `@preset.nz/app-kit` (with `react`, `react-dom`, `@tauri-apps/api`, `@preset.nz/ux-kit`) to `resolve.dedupe`; a second copy of React fails only in the window. app-kit ships no Tailwind classes, so it needs no `@source` line.

## Development

`just check` runs `cargo fmt --check`, clippy, `cargo test`, `tsc` and eslint. MIT.
