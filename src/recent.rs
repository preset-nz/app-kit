//! File > Open Recent: the documents opened or saved most recently, newest first.
//!
//! The list lives in `recent-documents.json` in the app's data directory, so it survives a
//! relaunch. Its first entry is the last document, which relaunch restore reads rather than
//! keeping a second store (`menu-standard.md`, decision 10). Neither muda nor a Tauri plugin
//! keeps recents, so app-kit does.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

/// How many documents the submenu holds.
pub const LIMIT: usize = 10;

const FILE_NAME: &str = "recent-documents.json";

/// The recent documents, newest first, no repeats, at most [`LIMIT`].
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recents {
    paths: Vec<PathBuf>,
}

impl Recents {
    pub fn paths(&self) -> &[PathBuf] {
        &self.paths
    }

    /// The document was just opened or saved: it moves to the top.
    pub fn note(&mut self, path: &Path) {
        self.paths.retain(|p| p != path);
        self.paths.insert(0, path.to_path_buf());
        self.paths.truncate(LIMIT);
    }

    /// Drop one, for a file that has gone.
    pub fn forget(&mut self, path: &Path) {
        self.paths.retain(|p| p != path);
    }

    pub fn clear(&mut self) {
        self.paths.clear();
    }

    /// Read the list from the app's data directory. A missing or unreadable file is an empty list.
    pub(crate) fn load<R: Runtime>(app: &AppHandle<R>) -> Self {
        store_path(app)
            .and_then(|p| fs::read(p).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Write the list. Failing loses only the menu's memory, so it is not reported.
    pub(crate) fn store<R: Runtime>(&self, app: &AppHandle<R>) {
        let Some(path) = store_path(app) else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_vec_pretty(self) {
            let _ = fs::write(path, json);
        }
    }
}

fn store_path<R: Runtime>(app: &AppHandle<R>) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join(FILE_NAME))
}

/// The submenu's labels: the file name, and where two share a name, its folder after a dash,
/// as the Finder's Open Recent does ("report.rhizome — Drafts").
pub fn labels(paths: &[PathBuf]) -> Vec<String> {
    let name = |p: &PathBuf| {
        p.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| p.to_string_lossy().into_owned())
    };
    paths
        .iter()
        .map(|p| {
            let n = name(p);
            let shared = paths.iter().filter(|q| name(q) == n).count() > 1;
            match p.parent().and_then(|d| d.file_name()) {
                Some(dir) if shared => format!("{n} \u{2014} {}", dir.to_string_lossy()),
                _ => n,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn noting_moves_to_the_top_without_repeats() {
        let mut r = Recents::default();
        r.note(&p("/a/one.x"));
        r.note(&p("/a/two.x"));
        r.note(&p("/a/one.x"));
        assert_eq!(r.paths(), [p("/a/one.x"), p("/a/two.x")]);
    }

    #[test]
    fn the_list_keeps_the_newest_ten() {
        let mut r = Recents::default();
        for i in 0..12 {
            r.note(&p(&format!("/d/{i}.x")));
        }
        assert_eq!(r.paths().len(), LIMIT);
        assert_eq!(r.paths()[0], p("/d/11.x"));
        assert_eq!(r.paths()[LIMIT - 1], p("/d/2.x"));
    }

    #[test]
    fn forget_and_clear() {
        let mut r = Recents::default();
        r.note(&p("/a/one.x"));
        r.note(&p("/a/two.x"));
        r.forget(&p("/a/one.x"));
        assert_eq!(r.paths(), [p("/a/two.x")]);
        r.clear();
        assert!(r.paths().is_empty());
    }

    #[test]
    fn labels_name_the_folder_only_when_two_files_share_a_name() {
        let paths = [
            p("/w/Drafts/report.x"),
            p("/w/Final/report.x"),
            p("/w/notes.x"),
        ];
        assert_eq!(
            labels(&paths),
            [
                "report.x \u{2014} Drafts",
                "report.x \u{2014} Final",
                "notes.x"
            ]
        );
    }

    #[test]
    fn the_stored_shape_round_trips() {
        let mut r = Recents::default();
        r.note(&p("/a/one.x"));
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, r#"{"paths":["/a/one.x"]}"#);
        assert_eq!(serde_json::from_str::<Recents>(&json).unwrap(), r);
    }
}
