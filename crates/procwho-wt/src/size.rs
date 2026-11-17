//! Disk usage of a directory tree, and the small on-disk size cache.
//!
//! The walk uses `lstat` only: symlinks are counted as the link itself and never followed,
//! other devices are not entered, and a hard-linked inode is counted once per walk.
//! Allocated size is `st_blocks * 512`. On APFS, cloned files share blocks, so the sum can be
//! larger than the space that removing the tree would actually free.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Directory names whose contents can be rebuilt (dependencies, build output, caches).
pub const REBUILDABLE: [&str; 14] = [
    "node_modules",
    "target",
    ".next",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".turbo",
    ".cache",
    ".build",
    "DerivedData",
    ".parcel-cache",
    "coverage",
];

/// Result of one walk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirSize {
    pub bytes: u64,
    pub rebuildable_bytes: u64,
    /// Entries that could not be read (permission denied, vanished during the walk).
    #[serde(default)]
    pub unreadable: u64,
}

/// Walk `root` iteratively and sum allocated bytes. Directories whose path is in `skip`
/// (other worktrees nested inside this one) are not entered, so nothing is counted twice.
pub fn dir_size(root: &Path, skip: &HashSet<PathBuf>) -> std::io::Result<DirSize> {
    let meta = std::fs::symlink_metadata(root)?;
    let dev = meta.dev();
    let mut seen: HashSet<(u64, u64)> = HashSet::new();
    let mut out = DirSize {
        bytes: meta.blocks() * 512,
        ..Default::default()
    };
    if !meta.is_dir() {
        return Ok(out);
    }
    // (directory, inside a rebuildable directory)
    let mut stack: Vec<(PathBuf, bool)> = vec![(root.to_path_buf(), false)];
    while let Some((dir, rebuild)) = stack.pop() {
        let rd = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => {
                out.unreadable += 1;
                continue;
            }
        };
        for ent in rd {
            let Ok(ent) = ent else {
                out.unreadable += 1;
                continue;
            };
            let path = ent.path();
            // DirEntry::metadata does not traverse symlinks on Unix (fstatat + NOFOLLOW).
            let Ok(m) = ent.metadata() else {
                out.unreadable += 1;
                continue;
            };
            if m.dev() != dev {
                continue;
            }
            if m.nlink() > 1 && !m.is_dir() && !seen.insert((m.dev(), m.ino())) {
                continue;
            }
            let bytes = m.blocks() * 512;
            let is_dir = m.file_type().is_dir();
            if is_dir && skip.contains(&path) {
                continue;
            }
            let child_rebuild = rebuild
                || (is_dir
                    && ent
                        .file_name()
                        .to_str()
                        .is_some_and(|n| REBUILDABLE.contains(&n)));
            out.bytes += bytes;
            if child_rebuild {
                out.rebuildable_bytes += bytes;
            }
            if is_dir {
                stack.push((path, child_rebuild));
            }
        }
    }
    Ok(out)
}

/// One cached size.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
    #[serde(flatten)]
    pub size: DirSize,
    /// Unix seconds when the walk finished.
    pub scanned_at: i64,
}

/// `<state>/worktree-sizes.json`, keyed by worktree path.
#[derive(Debug, Default)]
pub struct SizeCache {
    pub path: PathBuf,
    pub entries: BTreeMap<String, CacheEntry>,
    pub dirty: bool,
}

/// Entries older than this are dropped when the cache is written, to keep the file small.
const CACHE_KEEP_SECS: i64 = 7 * 86400;

impl SizeCache {
    /// Load the cache; a missing or unreadable file gives an empty cache.
    pub fn load(state_dir: &Path) -> SizeCache {
        let path = state_dir.join("worktree-sizes.json");
        let entries = std::fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        SizeCache {
            path,
            entries,
            dirty: false,
        }
    }

    /// A cached entry younger than `ttl_secs`, with its age in seconds.
    pub fn fresh(&self, key: &str, now: i64, ttl_secs: i64) -> Option<(DirSize, i64)> {
        let e = self.entries.get(key)?;
        let age = now - e.scanned_at;
        (0..ttl_secs).contains(&age).then_some((e.size, age))
    }

    pub fn put(&mut self, key: String, size: DirSize, now: i64) {
        self.entries.insert(
            key,
            CacheEntry {
                size,
                scanned_at: now,
            },
        );
        self.dirty = true;
    }

    /// Write atomically (temp file + rename) if anything changed. This is the only file
    /// procwho-wt ever writes.
    pub fn save(&mut self, now: i64) -> std::io::Result<()> {
        if !self.dirty {
            return Ok(());
        }
        self.entries
            .retain(|_, e| now - e.scanned_at < CACHE_KEEP_SECS);
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        let body = serde_json::to_vec(&self.entries).map_err(std::io::Error::other)?;
        std::fs::write(&tmp, body)?;
        std::fs::rename(&tmp, &self.path)?;
        self.dirty = false;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("procwho-wt-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn blocks(p: &Path) -> u64 {
        std::fs::symlink_metadata(p).unwrap().blocks() * 512
    }

    #[test]
    fn walk_counts_hard_link_once_and_does_not_follow_symlinks() {
        let d = tmpdir("size");
        let big = d.join("big.bin");
        std::fs::File::create(&big)
            .unwrap()
            .write_all(&vec![7u8; 256 * 1024])
            .unwrap();
        std::fs::hard_link(&big, d.join("big-link.bin")).unwrap();
        // A symlink to a large directory outside the tree must count only as the link.
        let outside = tmpdir("size-outside");
        std::fs::File::create(outside.join("o.bin"))
            .unwrap()
            .write_all(&vec![1u8; 512 * 1024])
            .unwrap();
        std::os::unix::fs::symlink(&outside, d.join("ext")).unwrap();
        let nm = d.join("node_modules").join("pkg");
        std::fs::create_dir_all(&nm).unwrap();
        std::fs::File::create(nm.join("i.js"))
            .unwrap()
            .write_all(&vec![2u8; 64 * 1024])
            .unwrap();

        let s = dir_size(&d, &HashSet::new()).unwrap();
        let expect = blocks(&d)
            + blocks(&big)
            + blocks(&d.join("ext"))
            + blocks(&d.join("node_modules"))
            + blocks(&nm)
            + blocks(&nm.join("i.js"));
        assert_eq!(s.bytes, expect);
        assert_eq!(
            s.rebuildable_bytes,
            blocks(&d.join("node_modules")) + blocks(&nm) + blocks(&nm.join("i.js"))
        );
        assert!(s.bytes < 512 * 1024, "symlink target must not be counted");

        // A skipped nested directory is not counted.
        let mut skip = HashSet::new();
        skip.insert(d.join("node_modules"));
        let s2 = dir_size(&d, &skip).unwrap();
        assert_eq!(s2.rebuildable_bytes, 0);

        std::fs::remove_dir_all(&d).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn cache_roundtrip_and_ttl() {
        let d = tmpdir("cache");
        let mut c = SizeCache::load(&d);
        c.put(
            "/w/a".into(),
            DirSize {
                bytes: 10,
                rebuildable_bytes: 4,
                unreadable: 0,
            },
            1000,
        );
        c.save(1000).unwrap();
        let c = SizeCache::load(&d);
        assert_eq!(c.fresh("/w/a", 1100, 1800).unwrap().0.bytes, 10);
        assert_eq!(c.fresh("/w/a", 1100, 1800).unwrap().1, 100);
        assert!(c.fresh("/w/a", 5000, 1800).is_none());
        assert!(c.fresh("/w/b", 1100, 1800).is_none());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
