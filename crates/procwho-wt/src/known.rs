//! Bounded, atomic repository memory in procwho's own state directory.

use crate::discover::{canon, protected, Access};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Procwho-owned bounded repository memory. Files larger than 256 KiB are ignored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownRepo {
    pub path: PathBuf,
    pub last_seen_unix: i64,
}

pub fn load_known(state: &Path) -> Vec<KnownRepo> {
    let path = state.join("known-repos.json");
    if std::fs::metadata(&path).is_ok_and(|m| m.len() <= 262_144) {
        std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

pub fn prune_known(entries: Vec<KnownRepo>, home: &Path) -> Vec<KnownRepo> {
    let mut unique = BTreeMap::<PathBuf, i64>::new();
    for e in entries.into_iter().take(1000) {
        if e.path.is_absolute()
            && !protected(&e.path, home)
            && Access::default().allows(&e.path)
            && e.path.is_dir()
        {
            unique
                .entry(canon(&e.path))
                .and_modify(|t| *t = (*t).max(e.last_seen_unix))
                .or_insert(e.last_seen_unix);
        }
    }
    let mut entries: Vec<_> = unique
        .into_iter()
        .map(|(path, last_seen_unix)| KnownRepo {
            path,
            last_seen_unix,
        })
        .collect();
    entries.sort_by(|a, b| {
        b.last_seen_unix
            .cmp(&a.last_seen_unix)
            .then(a.path.cmp(&b.path))
    });
    entries.truncate(500);
    entries
}

pub fn save_known(state: &Path, entries: &[KnownRepo]) -> std::io::Result<()> {
    std::fs::create_dir_all(state)?;
    let tmp = state.join(format!("known-repos-{}.tmp", std::process::id()));
    std::fs::write(&tmp, serde_json::to_vec(entries)?)?;
    std::fs::rename(tmp, state.join("known-repos.json"))
}

/// Clear only procwho's repository memory; the next scan can learn live repositories again.
pub fn forget_known(state: &Path) -> std::io::Result<()> {
    save_known(state, &[])
}
