//! Finding repositories and worktree directories on disk, without running git.

use crate::git::{parse_git_file, split_worktree_gitdir};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Directory names never entered while walking roots and containers.
const SKIP_NAMES: [&str; 4] = ["node_modules", "target", "Library", ".Trash"];
/// Dot-directories that are entered anyway.
const DOT_ALLOWED: [&str; 5] = [".claude", ".t3", ".codex", ".gemini", ".kilo"];

/// A directory holding a `.git` file.
#[derive(Debug, Clone)]
pub struct GitFileDir {
    pub dir: PathBuf,
    /// Absolute gitdir from the file, if it parsed.
    pub gitdir: Option<PathBuf>,
}

/// Everything found by the walk.
#[derive(Debug, Default)]
pub struct Found {
    pub access: Access,
    pub protected_skipped: usize,
    /// Common git dirs (`<repo>/.git`, or a bare repo dir), canonicalized, deduplicated.
    pub repos: BTreeSet<PathBuf>,
    pub git_files: Vec<GitFileDir>,
    pub sources: BTreeMap<PathBuf, DiscoverySource>,
    /// Directories that were configured but could not be read.
    pub missing_roots: Vec<PathBuf>,
}

impl Found {
    fn add_git_file(&mut self, dir: &Path) {
        if self.git_files.iter().any(|g| g.dir == dir) {
            return;
        }
        let gitdir = std::fs::read_to_string(dir.join(".git"))
            .ok()
            .and_then(|t| parse_git_file(&t, dir))
            .map(|g| g.gitdir);
        if let Some(gd) = &gitdir {
            if !self.access.allows(gd) {
                self.protected_skipped += 1;
                return;
            }
            if let Some(common) = self.access.common_dir(gd) {
                self.repos.insert(canon(&common));
            }
        }
        self.git_files.push(GitFileDir {
            dir: dir.to_path_buf(),
            gitdir,
        });
    }
}

/// Canonical path when it resolves, the input otherwise.
pub fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// The common git dir for a linked worktree's gitdir: read `<gitdir>/commondir`, or fall back
/// to the `<common>/worktrees/<name>` layout. Only returns a directory that exists.
pub fn common_dir_of(gitdir: &Path) -> Option<PathBuf> {
    Access::default().common_dir(gitdir)
}

/// Protected paths require an explicit root/container choice, including indirect Git targets.
#[derive(Debug)]
pub struct Access {
    home: PathBuf,
    selected: Vec<PathBuf>,
}

impl Default for Access {
    fn default() -> Self {
        Self::new(home(), &[])
    }
}

impl Access {
    pub fn new(home: PathBuf, selected: &[PathBuf]) -> Self {
        Self {
            home,
            selected: selected.iter().map(|p| normalized(p)).collect(),
        }
    }

    pub fn allows(&self, path: &Path) -> bool {
        let path = normalized(path);
        let allowed = |p: &Path| {
            !protected(p, &self.home) || self.selected.iter().any(|root| p.starts_with(root))
        };
        if !allowed(&path) {
            return false;
        }
        allowed(&canon(&path))
    }

    pub fn common_dir(&self, gitdir: &Path) -> Option<PathBuf> {
        if !self.allows(gitdir) {
            return None;
        }
        if let Ok(t) = std::fs::read_to_string(gitdir.join("commondir")) {
            let t = t.trim();
            if !t.is_empty() {
                let p = Path::new(t);
                let c = if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    gitdir.join(p)
                };
                if self.allows(&c) && c.is_dir() {
                    return Some(c);
                }
            }
        }
        let (c, _) = split_worktree_gitdir(gitdir)?;
        (self.allows(&c) && c.is_dir()).then_some(c)
    }
}

fn normalized(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        if component == std::path::Component::ParentDir {
            result.pop();
        } else {
            result.push(component);
        }
    }
    result
}

fn enter(name: &str) -> bool {
    if SKIP_NAMES.contains(&name) {
        return false;
    }
    !name.starts_with('.') || DOT_ALLOWED.contains(&name)
}

/// Walk `root` down to `max_depth` levels. A directory with a `.git` directory is a main
/// repo (not entered, except for `<repo>/.claude/worktrees`); a directory with a `.git` file is
/// a linked worktree (or submodule), recorded and not entered.
pub fn walk(root: &Path, max_depth: usize, found: &mut Found) {
    if !found.access.allows(root) || !root.is_dir() {
        found.missing_roots.push(root.to_path_buf());
        return;
    }
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        if !found.access.allows(&dir) {
            continue;
        }
        match std::fs::symlink_metadata(dir.join(".git")) {
            Ok(m) if m.is_dir() => {
                found.repos.insert(canon(&dir.join(".git")));
                // Documented repo-local containers: Claude Code/Desktop, Gemini CLI, Kilo Code.
                for product in [".claude", ".gemini", ".kilo"] {
                    let cw = dir.join(product).join("worktrees");
                    if cw.is_dir() {
                        walk(&cw, 2, found);
                    }
                }
                continue;
            }
            Ok(m) if m.is_file() => {
                found.add_git_file(&dir);
                continue;
            }
            _ => {}
        }
        if depth >= max_depth {
            continue;
        }
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for ent in rd.flatten() {
            let Ok(ft) = ent.file_type() else { continue };
            if !ft.is_dir() {
                continue; // symlinks are not followed
            }
            let name = ent.file_name();
            if name.to_str().is_some_and(enter) {
                stack.push((ent.path(), depth + 1));
            }
        }
    }
}

/// Home directory from `$HOME`.
pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// A deterministic first-discovery source. Live evidence wins over container and root walks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscoverySource {
    Root,
    Container,
    LiveProcess,
    Remembered,
}

pub const CANDIDATES: &[&str] = &[
    "repos",
    "code",
    "src",
    "dev",
    "Developer",
    "Projects",
    "projects",
    "workspace",
    "work",
    "git",
    "github",
    "Code",
    "Sites",
    "go/src",
    "Documents",
    "Desktop",
];

/// Protected project folders are opt-in, even when an ancestor is being walked.
pub fn protected(path: &Path, home: &Path) -> bool {
    ["Documents", "Desktop"].iter().any(|n| path.starts_with(home.join(n)))
        // Tests/tools can supply another HOME. Keep real macOS user folders protected too.
        || path.ancestors().any(|p| {
            p.file_name().is_some_and(|n| n == "Documents" || n == "Desktop")
                && p.parent().and_then(Path::parent) == Some(Path::new("/Users"))
        })
}

/// Existing conventional developer directories. Protected folders remain candidates only.
pub fn default_roots(home: &Path) -> Vec<(PathBuf, usize)> {
    let mut seen = BTreeSet::new();
    let mut roots = Vec::new();
    for name in CANDIDATES
        .iter()
        .filter(|n| !["Documents", "Desktop"].contains(n))
    {
        let path = home.join(name);
        if !protected(&canon(&path), home) && path.is_dir() && seen.insert(canon(&path)) {
            roots.push((path, 3));
        }
    }
    roots
}

/// Existing product worktree containers. Repo-local Claude Code worktrees are found by walk.
pub fn default_containers(home: &Path) -> Vec<(PathBuf, usize)> {
    [
        (home.join(".t3/worktrees"), 3), // T3 Code: observed managed worktree layout.
        (home.join(".codex/worktrees"), 3), // Codex app: observed managed worktrees.
        (home.join(".claude/worktrees"), 3), // Claude Code: observed global worktree layout.
        (home.join(".cursor/worktrees"), 3), // Cursor: observed on macOS; parallel-agent checkouts.
        (home.join(".windsurf/worktrees"), 3), // Windsurf/Cascade: documented in Devin Desktop docs.
        (home.join(".local/share/opencode/worktree"), 3), // OpenCode: documented in upstream WorktreeStrategies source.
        (std::env::temp_dir().join("vibe-kanban/worktrees"), 3), // Vibe Kanban: documented macOS default in upstream utils/path.rs.
        (std::env::temp_dir().join("vibe-kanban-dev/worktrees"), 3), // Vibe Kanban: documented debug-build container.
        (home.join(".factory/worktrees"), 3), // Factory desktop/Droid: documented default.
        (home.join("conductor/workspaces"), 3), // Conductor: documented default since 0.25.0.
    ]
    .into_iter()
    .filter(|(path, _)| Access::default().allows(path) && path.is_dir())
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_system_temp_is_never_a_default_container() {
        let containers = default_containers(Path::new("/nonexistent/procwho-home"));
        assert!(!containers
            .iter()
            .any(|(p, _)| p == Path::new("/private/tmp") || p == Path::new("/tmp")));
    }

    #[test]
    fn skip_rules() {
        assert!(enter("repos"));
        assert!(enter(".claude") && enter(".t3") && enter(".codex"));
        assert!(
            !enter(".cache") && !enter("node_modules") && !enter("Library") && !enter(".Trash")
        );
    }

    #[test]
    fn default_roots_use_only_existing_directories_with_literal_spaces() {
        let d = std::env::temp_dir().join(format!("procwho-default roots-{}", std::process::id()));
        std::fs::create_dir_all(d.join("code")).unwrap();
        std::fs::create_dir_all(d.join(".codex/worktrees")).unwrap();
        let roots = default_roots(&d);
        assert_eq!(roots, vec![(d.join("code"), 3)]);
        let containers = default_containers(&d);
        assert!(containers.contains(&(d.join(".codex/worktrees"), 3)));
        assert!(!containers
            .iter()
            .any(|(p, _)| p == &d.join(".t3/worktrees")));
        std::fs::remove_dir_all(d).unwrap();
    }
}

/// Resolve cwd ancestry using Git's .git/commondir layout, without running a process.
pub fn repository_of(cwd: &Path) -> Option<PathBuf> {
    for dir in cwd.ancestors() {
        let dot = dir.join(".git");
        if dot.is_dir() {
            return Some(canon(&dot));
        }
        if dot.is_file() {
            let gd = parse_git_file(&std::fs::read_to_string(&dot).ok()?, dir)?.gitdir;
            if !Access::default().allows(&gd) {
                return None;
            }
            return common_dir_of(&gd).or(Some(gd)).map(|p| canon(&p));
        }
        if dir.join("HEAD").is_file() && dir.join("objects").is_dir() && dir.join("refs").is_dir() {
            return Some(canon(dir));
        }
    }
    None
}

/// Dedupe agent cwd paths before resolving, cap filesystem work at 256 directories.
pub fn live_repositories(live: &[crate::LiveUse], home: &Path) -> BTreeSet<PathBuf> {
    live.iter()
        .filter(|u| u.agent_related || u.kind.starts_with("agent-"))
        .map(|u| PathBuf::from(&u.cwd))
        .filter(|p| p.is_absolute() && !protected(p, home) && !protected(&canon(p), home))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(256)
        .filter_map(|p| repository_of(&p))
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct RootSuggestion {
    pub path: PathBuf,
    pub exists: bool,
    pub protected: bool,
    pub repo_count: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct RootSuggestions {
    pub candidates: Vec<RootSuggestion>,
    pub automatic_folders: Vec<PathBuf>,
}

/// Probe depth two with a shared deadline; protected folders are never opened or statted.
pub fn suggest_roots(home: &Path) -> RootSuggestions {
    suggest_roots_with_budget(home, Duration::from_secs(2))
}

/// A zero budget lists candidates without probing; used for the app's loading checklist.
pub fn suggest_roots_with_budget(home: &Path, budget: Duration) -> RootSuggestions {
    git_probe_policy();
    let deadline = Instant::now() + budget;
    let mut seen = BTreeSet::new();
    let candidates = CANDIDATES
        .iter()
        .filter_map(|name| {
            let path = home.join(name);
            let is_protected =
                ["Documents", "Desktop"].contains(name) || protected(&canon(&path), home);
            let exists = is_protected || path.is_dir();
            if exists && !is_protected && !seen.insert(canon(&path)) {
                return None;
            }
            let repo_count = if is_protected || !exists {
                None
            } else {
                probe(&path, deadline)
            };
            Some(RootSuggestion {
                path,
                exists,
                protected: is_protected,
                repo_count,
            })
        })
        .collect();
    RootSuggestions {
        candidates,
        automatic_folders: default_containers(home)
            .into_iter()
            .map(|(p, _)| p)
            .collect(),
    }
}

fn git_probe_policy() {
    crate::git::no_materialize_this_thread();
}

fn probe(root: &Path, deadline: Instant) -> Option<usize> {
    let mut count = 0;
    let mut stack = vec![(root.to_path_buf(), 0)];
    let mut entries = 0;
    while let Some((dir, depth)) = stack.pop() {
        if Instant::now() >= deadline || entries >= 20_000 {
            return None;
        }
        if dir.join(".git").exists() {
            count += 1;
            continue;
        }
        if depth == 2 {
            continue;
        }
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for ent in rd.flatten() {
                entries += 1;
                if Instant::now() >= deadline || entries >= 20_000 {
                    return None;
                }
                if ent.file_name().to_str().is_some_and(enter)
                    && ent.file_type().is_ok_and(|t| t.is_dir())
                {
                    stack.push((ent.path(), depth + 1));
                }
            }
        }
    }
    Some(count)
}
