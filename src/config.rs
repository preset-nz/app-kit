//! Which of the menu standard's built-in items an app shows, from one TOML file the app
//! builds in (`include_str!("../menu.toml")`). Design: guidance
//! `projects/app-kit/design/menu-config.md`.
//!
//! Only on and off: labels, accelerators and positions belong to the standard. A missing key
//! takes the default (on for the document commands and Settings); an unknown key is an error,
//! so a typo fails the app's own test instead of silently doing nothing.

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct FileSlots {
    pub new: bool,
    pub open: bool,
    pub open_recent: bool,
    pub close: bool,
    pub save: bool,
    pub save_as: bool,
    pub revert: bool,
}

impl Default for FileSlots {
    fn default() -> Self {
        FileSlots {
            new: true,
            open: true,
            open_recent: true,
            close: true,
            save: true,
            save_as: true,
            revert: true,
        }
    }
}

impl FileSlots {
    /// The slots that only make sense with a [`Document`](crate::Document) behind them.
    pub fn document_slots_on(&self) -> Vec<&'static str> {
        [
            ("file.new", self.new),
            ("file.open", self.open),
            ("file.open_recent", self.open_recent),
            ("file.save", self.save),
            ("file.save_as", self.save_as),
            ("file.revert", self.revert),
        ]
        .into_iter()
        .filter_map(|(k, on)| on.then_some(k))
        .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct AppSlots {
    pub settings: bool,
}

impl Default for AppSlots {
    fn default() -> Self {
        AppSlots { settings: true }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct MenuConfig {
    pub file: FileSlots,
    pub app: AppSlots,
}

impl MenuConfig {
    /// Parse the app's `menu.toml`. Unknown sections and keys are errors.
    pub fn parse(source: &str) -> Result<Self, String> {
        toml::from_str(source).map_err(|e| format!("menu config: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_keys_take_the_defaults() {
        assert_eq!(MenuConfig::parse("").unwrap(), MenuConfig::default());
        let c = MenuConfig::parse("[file]\nnew = false\n").unwrap();
        assert!(!c.file.new && c.file.open && c.app.settings);
    }

    #[test]
    fn a_library_turns_the_document_set_off() {
        let c = MenuConfig::parse(
            "[file]\nnew = false\nopen = false\nopen_recent = false\nclose = false\nsave = false\nsave_as = false\nrevert = false\n",
        )
        .unwrap();
        assert!(c.file.document_slots_on().is_empty());
        assert_eq!(MenuConfig::default().file.document_slots_on().len(), 6);
    }

    #[test]
    fn typos_are_errors() {
        assert!(MenuConfig::parse("[file]\nsav = false\n").is_err());
        assert!(MenuConfig::parse("[flie]\nsave = false\n").is_err());
        assert!(MenuConfig::parse("[file]\nsave = 0\n").is_err());
    }
}
