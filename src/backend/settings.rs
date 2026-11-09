//! User settings, stored as `settings.json` in the data folder.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SortKey {
    #[default]
    LastCommit,
    Name,
    Commits,
    Lines,
    Stars,
}

impl SortKey {
    pub const ALL: [SortKey; 5] = [SortKey::LastCommit, SortKey::Name, SortKey::Commits, SortKey::Lines, SortKey::Stars];
    pub fn label(self) -> &'static str {
        match self {
            SortKey::LastCommit => "Last commit",
            SortKey::Name => "Name",
            SortKey::Commits => "Commits",
            SortKey::Lines => "Lines of code",
            SortKey::Stars => "Stars",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub auto_check_updates: bool,
    /// Count forks in the overview and maintenance pages.
    pub include_forks: bool,
    /// Count archived repositories in the overview and maintenance pages.
    pub include_archived: bool,
    /// Only repositories the signed-in user owns (not org or collaborator repos).
    pub only_owned: bool,
    /// Sync automatically when the app starts.
    pub sync_on_start: bool,
    pub sort: SortKey,
    pub sort_desc: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_check_updates: true,
            include_forks: false,
            include_archived: false,
            only_owned: false,
            sync_on_start: true,
            sort: SortKey::LastCommit,
            sort_desc: true,
        }
    }
}

impl Settings {
    fn path(root: &Path) -> PathBuf {
        root.join("settings.json")
    }

    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(Self::path(root)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self, root: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(root)?;
        write_atomic(&Self::path(root), serde_json::to_string_pretty(self)?.as_bytes())
    }
}

/// Write via a temp file and rename, so a crash never leaves a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).or_else(|_| {
        let _ = std::fs::remove_file(path);
        std::fs::rename(&tmp, path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_defaults() {
        let d = tempfile::tempdir().unwrap();
        assert!(Settings::load(d.path()).auto_check_updates);
        let s = Settings { include_forks: true, sort: SortKey::Stars, ..Default::default() };
        s.save(d.path()).unwrap();
        let l = Settings::load(d.path());
        assert!(l.include_forks);
        assert_eq!(l.sort, SortKey::Stars);
        // Unknown/missing fields fall back to defaults.
        std::fs::write(d.path().join("settings.json"), r#"{"include_forks":true}"#).unwrap();
        assert!(Settings::load(d.path()).sync_on_start);
    }
}
