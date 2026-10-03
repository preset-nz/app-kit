//! The command table: ids, labels, accelerators, menu placement and kind, declared once.

use serde::Serialize;

/// Which menu a command lives in. App-kit adds the standard items of each menu around
/// the app's own. The bar runs App · File · Edit · View · [domain menus] · Window · Help
/// (`menu-standard.md`); a domain menu is named with [`Command::domain`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum MenuName {
    App,
    File,
    Edit,
    View,
    /// A domain menu, one per collection, named in the singular: Layer, Effect, Mesh.
    Domain,
    Window,
    Help,
}

/// The family's reserved accelerators (`menu-standard.md`, decisions 2 to 4). Use these rather
/// than writing the strings, so every app binds the same chord to the same meaning.
pub mod shortcut {
    /// File > Import…. `Cmd+I` stays free for Inspector.
    pub const IMPORT: &str = "CmdOrCtrl+Shift+I";
    /// Move the selected item earlier in its collection.
    pub const MOVE_EARLIER: &str = "Alt+Up";
    /// Move the selected item later in its collection.
    pub const MOVE_LATER: &str = "Alt+Down";
    /// Navigation Back, as in Finder and Safari. Not for reordering.
    pub const BACK: &str = "CmdOrCtrl+[";
    /// Navigation Forward.
    pub const FORWARD: &str = "CmdOrCtrl+]";
    /// View > Actual Size.
    pub const ZOOM_RESET: &str = "CmdOrCtrl+0";
    pub const ZOOM_IN: &str = "CmdOrCtrl+=";
    pub const ZOOM_OUT: &str = "CmdOrCtrl+-";
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Kind {
    /// A plain item.
    Item,
    /// A check item; the webview reports its state.
    Toggle,
}

/// One command. The id is shared by the menu item, the `command` event and the
/// toolbar item; the accelerator is written here and nowhere else.
#[derive(Clone, Debug)]
pub struct Command {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) toolbar_label: Option<String>,
    pub(crate) accelerator: Option<String>,
    pub(crate) menu: MenuName,
    pub(crate) section: u8,
    pub(crate) kind: Kind,
    pub(crate) enabled: bool,
    pub(crate) checked: bool,
    /// The domain menu's title, when `menu` is [`MenuName::Domain`].
    pub(crate) domain: Option<String>,
    /// A submenu of the section this command sits in ("Add"), by title.
    pub(crate) submenu: Option<String>,
    /// The op's own category ("Colour", "Tone"). Inside a submenu, items group by it.
    pub(crate) category: Option<String>,
    /// The op's own tags, for the library and the command palette.
    pub(crate) tags: Vec<String>,
}

impl Command {
    fn new(id: &str, label: &str, kind: Kind) -> Self {
        Command {
            id: id.into(),
            label: label.into(),
            toolbar_label: None,
            accelerator: None,
            menu: MenuName::Edit,
            section: 1,
            kind,
            enabled: true,
            checked: false,
            domain: None,
            submenu: None,
            category: None,
            tags: Vec::new(),
        }
    }

    /// A plain item. `label` is the menu text.
    pub fn item(id: &str, label: &str) -> Self {
        Self::new(id, label, Kind::Item)
    }

    /// A check item (a panel shown or hidden), checked to start with.
    pub fn toggle(id: &str, label: &str) -> Self {
        let mut c = Self::new(id, label, Kind::Toggle);
        c.checked = true;
        c
    }

    /// A Tauri accelerator string, for example `CmdOrCtrl+Alt+S`.
    pub fn accelerator(mut self, accelerator: &str) -> Self {
        self.accelerator = Some(accelerator.into());
        self
    }

    /// The text for the toolbar, when it differs from the menu's ("Outline" for "Show Outline").
    pub fn toolbar_label(mut self, label: &str) -> Self {
        self.toolbar_label = Some(label.into());
        self
    }

    pub fn menu(mut self, menu: MenuName) -> Self {
        self.menu = menu;
        self
    }

    /// Items with the same section sit together, separated from other sections. Default 1.
    pub fn section(mut self, section: u8) -> Self {
        self.section = section;
        self
    }

    /// Start disabled; the webview enables it through `app_kit_menu_state`.
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    pub fn unchecked(mut self) -> Self {
        self.checked = false;
        self
    }

    /// Put the command in a domain menu, named in the singular for its collection: "Effect".
    /// Domain menus sit between View and Window in the order they are first declared.
    pub fn domain(mut self, title: &str) -> Self {
        self.menu = MenuName::Domain;
        self.domain = Some(title.into());
        self
    }

    /// Put the command in a submenu of its section: "Add" builds `Add ▸`. Commands with the same
    /// submenu title in the same menu and section share it.
    pub fn submenu(mut self, title: &str) -> Self {
        self.submenu = Some(title.into());
        self
    }

    /// The op's own category. Inside a submenu, items group by category in the order first
    /// declared, separated; the library and palette read the same field.
    pub fn category(mut self, category: &str) -> Self {
        self.category = Some(category.into());
        self
    }

    pub fn tags<S: Into<String>>(mut self, tags: impl IntoIterator<Item = S>) -> Self {
        self.tags = tags.into_iter().map(Into::into).collect();
        self
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

/// A command as the webview sees it, with the accelerator already in display form.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfo {
    pub id: String,
    pub label: String,
    pub toolbar_label: Option<String>,
    pub accelerator: Option<String>,
    /// For tooltips: `⌥⌘S` on macOS, `Ctrl+Alt+S` elsewhere.
    pub shortcut: Option<String>,
    pub menu: MenuName,
    /// The domain menu's title, for `menu: domain`.
    pub domain: Option<String>,
    pub submenu: Option<String>,
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub kind: Kind,
}

impl Command {
    pub(crate) fn info(&self) -> CommandInfo {
        CommandInfo {
            id: self.id.clone(),
            label: self.label.clone(),
            toolbar_label: self.toolbar_label.clone(),
            accelerator: self.accelerator.clone(),
            shortcut: self
                .accelerator
                .as_deref()
                .map(|a| display_shortcut(a, cfg!(target_os = "macos"))),
            menu: self.menu,
            domain: self.domain.clone(),
            submenu: self.submenu.clone(),
            category: self.category.clone(),
            tags: self.tags.clone(),
            kind: self.kind,
        }
    }
}

/// A Tauri accelerator in the platform's own notation. macOS: modifiers in the order
/// ⌃⌥⇧⌘ then the key, no separators. Elsewhere: `Ctrl+Alt+Shift+Key`.
pub fn display_shortcut(accelerator: &str, mac: bool) -> String {
    let (mut ctrl, mut alt, mut shift, mut cmd) = (false, false, false, false);
    let mut key = String::new();
    for part in accelerator.split('+') {
        match part.to_ascii_lowercase().as_str() {
            "cmdorctrl" | "commandorcontrol" => {
                if mac {
                    cmd = true
                } else {
                    ctrl = true
                }
            }
            "cmd" | "command" | "super" => cmd = true,
            "ctrl" | "control" => ctrl = true,
            "alt" | "option" => alt = true,
            "shift" => shift = true,
            _ => {
                key = if part.chars().count() == 1 {
                    part.to_uppercase()
                } else {
                    part.into()
                }
            }
        }
    }
    if mac {
        let mut s = String::new();
        for (on, c) in [(ctrl, '⌃'), (alt, '⌥'), (shift, '⇧'), (cmd, '⌘')] {
            if on {
                s.push(c);
            }
        }
        s + &key
    } else {
        let mut parts = Vec::new();
        for (on, c) in [
            (ctrl, "Ctrl"),
            (alt, "Alt"),
            (shift, "Shift"),
            (cmd, "Super"),
        ] {
            if on {
                parts.push(c.to_string());
            }
        }
        parts.push(key);
        parts.join("+")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcuts_display_in_platform_notation() {
        for (accel, mac) in [
            ("CmdOrCtrl+Z", "⌘Z"),
            ("CmdOrCtrl+Shift+Z", "⇧⌘Z"),
            ("CmdOrCtrl+Alt+S", "⌥⌘S"),
            ("CmdOrCtrl+Shift+L", "⇧⌘L"),
            ("CmdOrCtrl+,", "⌘,"),
            ("Ctrl+Alt+Shift+Cmd+X", "⌃⌥⇧⌘X"),
        ] {
            assert_eq!(display_shortcut(accel, true), mac, "{accel}");
        }
        assert_eq!(display_shortcut("CmdOrCtrl+Alt+S", false), "Ctrl+Alt+S");
        assert_eq!(display_shortcut("CmdOrCtrl+Shift+Z", false), "Ctrl+Shift+Z");
    }

    #[test]
    fn info_carries_the_one_accelerator() {
        let info = Command::toggle("panel.left", "Show Outline")
            .accelerator("CmdOrCtrl+Alt+S")
            .toolbar_label("Outline")
            .menu(MenuName::View)
            .info();
        assert_eq!(info.accelerator.as_deref(), Some("CmdOrCtrl+Alt+S"));
        assert_eq!(info.toolbar_label.as_deref(), Some("Outline"));
        assert_eq!(info.kind, Kind::Toggle);
    }
}
