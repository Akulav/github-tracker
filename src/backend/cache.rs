//! Last sync on disk (`cache.json`), so the app opens instantly and spares the rate limit.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::github::{Details, Repo, User};

/// Bumped when the format changes incompatibly; older caches are ignored.
const FORMAT: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cache {
    #[serde(default)]
    pub format: u32,
    pub user: User,
    pub repos: Vec<Repo>,
    pub details: HashMap<u64, Details>,
    pub synced_at: Option<DateTime<Utc>>,
}

fn path(root: &Path) -> PathBuf {
    root.join("cache.json")
}

impl Cache {
    /// The cache, if it belongs to `login` (or to anyone when `login` is `None`).
    pub fn load(root: &Path, login: Option<&str>) -> Option<Cache> {
        let c: Cache = serde_json::from_str(&std::fs::read_to_string(path(root)).ok()?).ok()?;
        (c.format == FORMAT && login.is_none_or(|l| l.eq_ignore_ascii_case(&c.user.login))).then_some(c)
    }

    pub fn save(&self, root: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(root)?;
        let mut c = self.clone();
        c.format = FORMAT;
        super::settings::write_atomic(&path(root), &serde_json::to_vec(&c)?)
    }

    pub fn clear(root: &Path) {
        let _ = std::fs::remove_file(path(root));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_per_user() {
        let d = tempfile::tempdir().unwrap();
        let mut c = Cache { user: User { login: "Akulav".into(), ..Default::default() }, ..Default::default() };
        c.details.insert(42, Details { commit_count: Some(9), ..Default::default() });
        c.save(d.path()).unwrap();
        let l = Cache::load(d.path(), Some("akulav")).unwrap();
        assert_eq!(l.details[&42].commit_count, Some(9));
        assert!(Cache::load(d.path(), Some("someone-else")).is_none());
    }
}
