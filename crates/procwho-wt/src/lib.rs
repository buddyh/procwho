//! Read-only git worktree inventory for macOS.
//!
//! Finds every git worktree under a set of roots and agent worktree containers, and reports
//! its size, git status, merge state, last activity and whether a live process uses it, with
//! a review label ([`Verdict`]) and the facts behind it.
//!
//! Read-only: procwho-wt runs only read commands (`worktree list`, `for-each-ref`,
//! `symbolic-ref`, `branch --merged`, `merge-base --is-ancestor`, `log`, `status`,
//! `rev-list`, `rev-parse`), each with `-c core.fsmonitor=false -c core.untrackedCache=false
//! --no-optional-locks`, so it never starts an fsmonitor daemon or takes the index lock. It
//! never fetches, prunes, removes or checks out anything. The one command that writes,
//! `merge-tree --write-tree` (squash/rebase merge detection), writes only into a temp object
//! directory under a sandbox that denies every other file write; see
//! [`git::content_merged`]. Procwho keeps its own size cache and bounded repository memory
//! (`<state>/worktree-sizes.json` and `<state>/known-repos.json`).
//!
//! ```no_run
//! let opts = procwho_wt::Options::default();
//! let report = procwho_wt::scan(&opts, None);
//! print!("{}", procwho_wt::render(&report, 20));
//! ```

pub mod discover;
pub mod git;
pub mod known;
pub mod pr;
mod render;
pub mod size;
pub mod verdict;

pub use render::{fmt_age, fmt_bytes, render};
pub use verdict::{Facts, Verdict};

use discover::{canon, Found};
use git::{At, BranchRef, WtEntry};
use serde::Serialize;
use size::{DirSize, SizeCache};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// A live process with a working directory, supplied by the caller (procwho-proc).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveUse {
    pub pid: i32,
    pub cwd: String,
    pub workload: String,
    pub kind: String,
    pub agent_related: bool,
}

/// Sort order for the worktree list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// Largest first (unknown sizes last).
    #[default]
    Size,
    /// Least recently active first.
    Age,
    /// By repo name, then path.
    Repo,
}

/// Scan options. [`Options::default`] gives the documented defaults.
#[derive(Debug, Clone)]
pub struct Options {
    /// Directories searched for main repos, each with a maximum depth.
    pub roots: Vec<(PathBuf, usize)>,
    /// Agent worktree containers searched for `.git` files, each with a maximum depth.
    pub containers: Vec<(PathBuf, usize)>,
    /// Include main checkouts (not only linked worktrees).
    pub include_main: bool,
    /// Measure disk usage.
    pub size: bool,
    /// Reuse cached sizes younger than this many minutes.
    pub size_ttl_min: u64,
    /// Clean, unmerged, idle worktrees older than this are `stale`.
    pub stale_days: u32,
    /// Worker threads for git calls and size walks.
    pub jobs: usize,
    /// Per git call timeout.
    pub git_timeout: Duration,
    /// State directory (size cache).
    pub state_dir: PathBuf,
    pub sort: Sort,
    /// Only report worktrees with this verdict.
    pub verdict: Option<Verdict>,
    /// Only report worktrees whose repo name contains this (case-insensitive).
    pub repo: Option<String>,
    /// Ask GitHub (via `gh`) which PRs were merged; off by default.
    pub pr: bool,
    /// The `gh` program used when `pr` is on.
    pub gh: PathBuf,
}

impl Default for Options {
    fn default() -> Self {
        let h = discover::home();
        let state_dir = std::env::var_os("PROCWHO_STATE")
            .map(PathBuf::from)
            .unwrap_or_else(|| h.join(".local/state/procwho"));
        Options {
            roots: discover::default_roots(&h),
            containers: discover::default_containers(&h),
            include_main: false,
            size: true,
            size_ttl_min: 30,
            stale_days: 7,
            jobs: 4,
            git_timeout: Duration::from_secs(10),
            state_dir,
            sort: Sort::Size,
            verdict: None,
            repo: None,
            pr: false,
            gh: PathBuf::from("gh"),
        }
    }
}

/// Per-repo summary.
#[derive(Debug, Clone, Serialize)]
pub struct RepoInfo {
    pub discovered_via: discover::DiscoverySource,
    pub name: String,
    /// Main checkout path (or the bare repo directory).
    pub path: String,
    pub git_dir: String,
    /// Ref the merge checks use, e.g. `origin/main`; `null` when none was found.
    pub default_branch: Option<String>,
    /// Unix time of `.git/FETCH_HEAD` (remote info is only as fresh as this).
    pub fetched_at: Option<i64>,
    pub fetch_age_secs: Option<i64>,
    pub has_remotes: Option<bool>,
    /// `owner/name` of the GitHub remote whose merged PRs were read (`--pr` only).
    pub github_repo: Option<String>,
    /// Linked worktrees git lists for this repo.
    pub worktree_count: usize,
    pub notes: Vec<String>,
}

/// One worktree (or orphaned worktree-like directory).
#[derive(Debug, Clone, Serialize)]
pub struct Worktree {
    pub discovered_via: discover::DiscoverySource,
    pub path: String,
    pub repo: String,
    pub repo_path: String,
    /// `linked`, `main` or `orphan`.
    pub kind: &'static str,
    pub exists: bool,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub detached: bool,
    pub locked: bool,
    pub lock_reason: Option<String>,
    pub prunable: Option<String>,
    pub upstream: Option<String>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    pub upstream_gone: bool,
    pub tracked_changes: Option<u32>,
    pub untracked_files: Option<u32>,
    pub unpushed_commits: Option<u32>,
    /// Tip is an ancestor of the default branch.
    pub merged: Option<bool>,
    /// Every change on the branch is already in the default branch (squash or rebase merge),
    /// decided with an isolated `git merge-tree`. `null` when not checked or undeterminable.
    pub content_merged: Option<bool>,
    /// Merged GitHub PR from this branch (`--pr` only; `null` otherwise or when none).
    pub pr_merged: Option<pr::PrMerged>,
    pub default_branch: Option<String>,
    /// Unix time of the latest activity signal.
    pub last_activity_unix: Option<i64>,
    pub idle_secs: Option<i64>,
    pub size_bytes: Option<u64>,
    pub rebuildable_bytes: Option<u64>,
    pub size_cached: bool,
    pub size_measured_unix: Option<i64>,
    pub size_cache_age_secs: Option<i64>,
    pub in_use: Option<bool>,
    pub in_use_procs: Option<usize>,
    pub in_use_workloads: Vec<String>,
    /// Milliseconds spent in git calls for this worktree.
    pub git_ms: u64,
    pub verdict: Verdict,
    /// Facts worth seeing at a glance that are not verdicts: `upstream-gone` (the branch's
    /// upstream was deleted, often after a merged PR), `locked`, `detached`.
    pub flags: Vec<&'static str>,
    pub reasons: Vec<String>,
    /// What could not be determined, and why.
    pub notes: Vec<String>,
}

/// Count and bytes for one verdict.
#[derive(Debug, Clone, Serialize)]
pub struct VerdictTotal {
    pub verdict: Verdict,
    pub count: usize,
    pub size_bytes: u64,
    pub rebuildable_bytes: u64,
    /// Worktrees in this group whose size is unknown (not included in the bytes).
    pub size_unknown: usize,
}

/// Version of the JSON report layout. Bumped when a documented key changes meaning or name.
pub const SCHEMA_VERSION: u32 = 3;

/// Result of [`scan`].
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// [`SCHEMA_VERSION`].
    pub schema_version: u32,
    pub generated_at: i64,
    pub observed_at_unix: i64,
    pub collect_ms: u64,
    pub generated_at_utc: String,
    pub scan_secs: f64,
    pub repos_found: usize,
    /// Worktrees found before `verdict` / `repo` filters.
    pub worktrees_found: usize,
    /// Exact configured roots and agent containers searched by this scan.
    pub searched_folders: Vec<String>,
    /// Reclaimable is an upper bound, not guaranteed freed space.
    pub reclaimable_estimated: bool,
    pub reclaimable_note: &'static str,
    pub totals: Vec<VerdictTotal>,
    pub repos: Vec<RepoInfo>,
    pub worktrees: Vec<Worktree>,
    pub sizes_walked: usize,
    pub sizes_cached: usize,
    pub filter: Option<String>,
    /// Wall seconds per phase: discovery, repos, worktrees.
    pub phase_secs: BTreeMap<&'static str, f64>,
    pub notes: Vec<String>,
}

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn mtime(p: &Path) -> Option<i64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(p).ok().map(|m| m.mtime())
}

/// Run `f` over `items` on `jobs` threads; results come back in input order.
fn parallel_map<T: Sync, R: Send>(items: &[T], jobs: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let queue: Mutex<VecDeque<usize>> = Mutex::new((0..items.len()).collect());
    let results: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|s| {
        for _ in 0..jobs.clamp(1, 64).min(items.len().max(1)) {
            s.spawn(|| {
                // Walker threads must not download iCloud-evicted directories either.
                git::no_materialize_this_thread();
                loop {
                    let next = queue.lock().unwrap_or_else(|e| e.into_inner()).pop_front();
                    let Some(i) = next else { break };
                    let r = f(&items[i]);
                    results.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(r);
                }
            });
        }
    });
    results
        .into_inner()
        .unwrap_or_else(|e| e.into_inner())
        .into_iter()
        .flatten()
        .collect()
}

/// What phase 2 learned about one repo.
struct RepoScan {
    info: RepoInfo,
    git_dir: PathBuf,
    /// `None` when `worktree list` failed.
    entries: Option<Vec<WtEntry>>,
    branches: HashMap<String, BranchRef>,
    /// `None` when the merged set is unknown.
    merged: Option<HashSet<String>>,
    /// Detached HEAD sha -> contained in default branch.
    detached_merged: HashMap<String, Option<bool>>,
    commit_times: HashMap<String, i64>,
    /// Absolute path of the repository's object directory.
    objects: Option<PathBuf>,
    /// Commit and tree of the default branch ref.
    default_sha: Option<String>,
    default_tree: Option<String>,
    /// Merged PRs of the GitHub remote (`--pr` only).
    prs: Option<Vec<pr::MergedPr>>,
}

fn repo_display(git_dir: &Path) -> (String, PathBuf) {
    let path = if git_dir.file_name().is_some_and(|n| n == ".git") {
        git_dir.parent().unwrap_or(git_dir).to_path_buf()
    } else {
        git_dir.to_path_buf()
    };
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().trim_end_matches(".git").to_string())
        .unwrap_or_else(|| path.display().to_string());
    (name, path)
}

/// `owner/name` of the repo's GitHub remote: `origin` if it is on GitHub, else the first that is.
fn github_remote(remote_urls: &str) -> Option<String> {
    let mut found: Vec<(bool, String)> = remote_urls
        .lines()
        .filter_map(|l| l.split_once(' '))
        .filter_map(|(k, url)| pr::github_slug(url).map(|s| (k != "remote.origin.url", s)))
        .collect();
    found.sort();
    found.into_iter().next().map(|(_, s)| s)
}

fn scan_repo(
    git_dir: &Path,
    opts: &Options,
    now: i64,
    prc: Option<&Mutex<pr::PrCache>>,
) -> RepoScan {
    let t = opts.git_timeout;
    let g = |args: &[&str]| git::run(At::GitDir(git_dir), args, t).ok();
    let (name, path) = repo_display(git_dir);
    let fetched_at = mtime(&git_dir.join("FETCH_HEAD"));
    let mut info = RepoInfo {
        discovered_via: discover::DiscoverySource::Root,
        name,
        path: path.display().to_string(),
        git_dir: git_dir.display().to_string(),
        default_branch: None,
        fetched_at,
        fetch_age_secs: fetched_at.map(|f| now - f),
        has_remotes: None,
        github_repo: None,
        worktree_count: 0,
        notes: Vec::new(),
    };
    let mut rs = RepoScan {
        info: info.clone(),
        git_dir: git_dir.to_path_buf(),
        entries: None,
        branches: HashMap::new(),
        merged: None,
        detached_merged: HashMap::new(),
        commit_times: HashMap::new(),
        objects: None,
        default_sha: None,
        default_tree: None,
        prs: None,
    };
    // `git worktree list` reads `<common>/worktrees/*`; with none there is nothing linked,
    // so skip the spawn (most repos on a machine have no linked worktrees).
    let has_linked =
        std::fs::read_dir(git_dir.join("worktrees")).is_ok_and(|mut d| d.next().is_some());
    if !has_linked && !opts.include_main {
        rs.entries = Some(Vec::new());
        rs.info = info;
        return rs;
    }
    let entries = match g(&["worktree", "list", "--porcelain", "-z"]) {
        Ok(o) => git::parse_worktree_list(&o),
        Err(e) => {
            info.notes.push(format!("worktree list: {e}"));
            rs.info = info;
            return rs;
        }
    };
    let wanted: Vec<&WtEntry> = entries
        .iter()
        .enumerate()
        .filter(|(i, e)| !e.bare && (*i > 0 || opts.include_main))
        .map(|(_, e)| e)
        .collect();
    info.worktree_count = entries.len().saturating_sub(1);
    if wanted.is_empty() {
        rs.entries = Some(entries);
        rs.info = info;
        return rs;
    }

    // Default branch: origin/HEAD, then origin/main, origin/master, main, master.
    let remotes = g(&[
        "for-each-ref",
        "--format=%(refname)%09%(objectname)",
        "refs/remotes",
    ]);
    let remote_refs: HashMap<String, String> = match &remotes {
        Ok(o) => o
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(r, s)| (r.to_string(), s.to_string()))
            .collect(),
        Err(e) => {
            info.notes.push(format!("remote refs: {e}"));
            HashMap::new()
        }
    };
    if remotes.is_ok() {
        info.has_remotes = Some(!remote_refs.is_empty());
    }
    match g(&["for-each-ref", git::BRANCH_FORMAT, "refs/heads"]) {
        Ok(o) => {
            rs.branches = git::parse_for_each_ref(&o)
                .into_iter()
                .map(|b| (b.name.clone(), b))
                .collect()
        }
        Err(e) => info.notes.push(format!("branches: {e}")),
    }
    let origin_head = g(&["symbolic-ref", "--short", "refs/remotes/origin/HEAD"])
        .ok()
        .map(|s| s.trim().to_string());
    let default = origin_head
        .filter(|s| !s.is_empty())
        .or_else(|| {
            ["main", "master"]
                .iter()
                .find(|b| remote_refs.contains_key(&format!("refs/remotes/origin/{b}")))
                .map(|b| format!("origin/{b}"))
        })
        .or_else(|| {
            ["main", "master"]
                .iter()
                .find(|b| rs.branches.contains_key(**b))
                .map(|b| b.to_string())
        });
    match &default {
        Some(d) => {
            if !d.starts_with("origin/") {
                info.notes.push(format!(
                    "no origin default branch; merge checks use local {d}"
                ));
            }
            match g(&["branch", "--merged", d, "--format=%(refname:short)"]) {
                Ok(o) => rs.merged = Some(o.lines().map(|l| l.trim().to_string()).collect()),
                Err(e) => info.notes.push(format!("branch --merged {d}: {e}")),
            }
        }
        None => info
            .notes
            .push("no default branch found (origin/HEAD, origin/main|master, main, master)".into()),
    }
    info.default_branch = default.clone();
    if let Some(d) = &default {
        let (c, t) = (format!("{d}^{{commit}}"), format!("{d}^{{tree}}"));
        match g(&[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "objects",
            &c,
            &t,
        ]) {
            Ok(o) => {
                let l: Vec<&str> = o.lines().map(str::trim).collect();
                if let [obj, sha, tree] = l[..] {
                    rs.objects = Some(PathBuf::from(obj));
                    rs.default_sha = Some(sha.to_string());
                    rs.default_tree = Some(tree.to_string());
                }
            }
            Err(e) => info.notes.push(format!("rev-parse {d}: {e}")),
        }
    }

    if let Some(cache) = prc {
        let urls = g(&["config", "--get-regexp", r"^remote\..*\.url$"]).unwrap_or_default();
        match github_remote(&urls) {
            Some(slug) => match pr::merged_prs(&opts.gh, &slug, cache, now) {
                Ok((prs, age)) => {
                    if let Some(a) = age {
                        info.notes
                            .push(format!("merged PRs from cache ({} old)", fmt_age(a)));
                    }
                    rs.prs = Some(prs);
                    info.github_repo = Some(slug);
                }
                Err(e) => info.notes.push(format!("PR state not checked: {e}")),
            },
            None => info
                .notes
                .push("PR state not checked: no GitHub remote".into()),
        }
    }

    // Detached worktrees: ancestry check and commit time per sha.
    let detached: Vec<&str> = wanted
        .iter()
        .filter(|e| e.detached)
        .filter_map(|e| e.head.as_deref())
        .filter(|h| !is_zero(h))
        .collect();
    if let Some(d) = &default {
        for sha in &detached {
            let v = match git::run(
                At::GitDir(git_dir),
                &["merge-base", "--is-ancestor", sha, d],
                t,
            ) {
                git::GitOut::Done { code: 0, .. } => Some(true),
                git::GitOut::Done { code: 1, .. } => Some(false),
                other => {
                    info.notes.push(format!(
                        "merge-base {}: {}",
                        &sha[..sha.len().min(10)],
                        other.ok().err().unwrap_or_default()
                    ));
                    None
                }
            };
            rs.detached_merged.insert(sha.to_string(), v);
        }
    }
    if !detached.is_empty() {
        let mut args = vec!["log", "--no-walk=unsorted", "--format=%H%x09%ct"];
        args.extend(detached.iter().copied());
        match g(&args) {
            Ok(o) => {
                for l in o.lines() {
                    if let Some((h, ct)) = l.split_once('\t') {
                        if let Ok(ct) = ct.trim().parse() {
                            rs.commit_times.insert(h.to_string(), ct);
                        }
                    }
                }
            }
            Err(e) => info.notes.push(format!("commit times: {e}")),
        }
    }
    rs.entries = Some(entries);
    rs.info = info;
    rs
}

fn is_zero(sha: &str) -> bool {
    sha.bytes().all(|b| b == b'0')
}

/// One unit of phase 3 work.
struct WtJob {
    repo_idx: Option<usize>,
    /// Path as git records it (or the orphan directory).
    path: PathBuf,
    canon: PathBuf,
    kind: &'static str,
    entry: Option<WtEntry>,
    orphan: Option<String>,
    repo_name: String,
    repo_path: String,
}

struct Shared<'a> {
    opts: &'a Options,
    now: i64,
    repos: &'a [RepoScan],
    live: Option<Vec<(PathBuf, &'a LiveUse)>>,
    cache: Mutex<SizeCache>,
    skip: HashSet<PathBuf>,
    /// `Err(why)` when `git merge-tree --write-tree` is not available.
    merge_tree: Result<(), String>,
}

fn in_use(sh: &Shared<'_>, job: &WtJob) -> (Option<bool>, usize, Vec<String>) {
    let Some(live) = &sh.live else {
        return (None, 0, Vec::new());
    };
    let mut n = 0;
    let mut names: Vec<String> = Vec::new();
    for (cwd, u) in live {
        let hit = [&job.canon, &job.path].iter().any(|p| cwd.starts_with(p));
        if hit {
            n += 1;
            if names.len() < 5 && !names.contains(&u.workload) {
                names.push(u.workload.clone());
            }
        }
    }
    (Some(n > 0), n, names)
}

fn measure(sh: &Shared<'_>, job: &WtJob, w: &mut Worktree) -> (bool, bool) {
    // returns (walked, cached)
    if !sh.opts.size {
        w.notes.push("size not measured (--no-size)".into());
        return (false, false);
    }
    let key = job.canon.display().to_string();
    let ttl = sh.opts.size_ttl_min as i64 * 60;
    let cached = sh
        .cache
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .fresh(&key, sh.now, ttl);
    if let Some((s, age)) = cached {
        apply_size(w, s);
        w.size_cached = true;
        w.size_measured_unix = Some(sh.now - age);
        w.size_cache_age_secs = Some(age);
        return (false, true);
    }
    match size::dir_size(&job.canon, &sh.skip) {
        Ok(s) => {
            apply_size(w, s);
            let measured = now_unix();
            w.size_measured_unix = Some(measured);
            sh.cache
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .put(key, s, measured);
            (true, false)
        }
        Err(e) => {
            w.notes.push(format!("size: {e}"));
            (false, false)
        }
    }
}

fn apply_size(w: &mut Worktree, s: DirSize) {
    w.size_bytes = Some(s.bytes);
    w.rebuildable_bytes = Some(s.rebuildable_bytes);
    if s.unreadable > 0 {
        w.notes.push(format!(
            "size: {} entries unreadable (not counted)",
            s.unreadable
        ));
    }
}

/// Content-merge check for one tip; `None` (with a note) when it cannot be decided.
fn check_content(
    sh: &Shared<'_>,
    r: &RepoScan,
    tip: Option<&str>,
    notes: &mut Vec<String>,
) -> Option<bool> {
    if let Err(why) = &sh.merge_tree {
        notes.push(format!("content merge not checked: {why}"));
        return None;
    }
    let (Some(tip), Some(d), Some(obj), Some(sha), Some(tree)) = (
        tip,
        &r.info.default_branch,
        &r.objects,
        &r.default_sha,
        &r.default_tree,
    ) else {
        notes.push("content merge not checked: default branch tree unknown".into());
        return None;
    };
    if tip == sha {
        return Some(true);
    }
    match git::content_merged(&r.git_dir, obj, d, tree, tip, sh.opts.git_timeout) {
        Ok(v) => Some(v),
        Err(e) => {
            notes.push(format!("content merge: {e}"));
            None
        }
    }
}

fn scan_worktree(sh: &Shared<'_>, job: &WtJob) -> (Worktree, bool, bool) {
    let t = sh.opts.git_timeout;
    let repo = job.repo_idx.map(|i| &sh.repos[i]);
    let e = job.entry.clone().unwrap_or_default();
    let exists = std::fs::symlink_metadata(&job.path).is_ok_and(|m| m.is_dir());
    let mut w = Worktree {
        discovered_via: repo.map(|r| r.info.discovered_via).unwrap_or_else(|| {
            if sh
                .opts
                .containers
                .iter()
                .any(|(p, _)| job.path.starts_with(p))
            {
                discover::DiscoverySource::Container
            } else {
                discover::DiscoverySource::Root
            }
        }),
        path: job.path.display().to_string(),
        repo: job.repo_name.clone(),
        repo_path: job.repo_path.clone(),
        kind: job.kind,
        exists,
        head: e.head.clone().filter(|h| !is_zero(h)),
        branch: e.branch.clone(),
        detached: e.detached,
        locked: e.locked.is_some(),
        lock_reason: e.locked.clone().filter(|s| !s.is_empty()),
        prunable: e.prunable.clone(),
        upstream: None,
        ahead: None,
        behind: None,
        upstream_gone: false,
        tracked_changes: None,
        untracked_files: None,
        unpushed_commits: None,
        merged: None,
        content_merged: None,
        pr_merged: None,
        default_branch: repo.and_then(|r| r.info.default_branch.clone()),
        last_activity_unix: None,
        idle_secs: None,
        size_bytes: None,
        rebuildable_bytes: None,
        size_cached: false,
        size_measured_unix: None,
        size_cache_age_secs: None,
        in_use: None,
        in_use_procs: None,
        in_use_workloads: Vec::new(),
        git_ms: 0,
        verdict: Verdict::Unknown,
        flags: Vec::new(),
        reasons: Vec::new(),
        notes: Vec::new(),
    };
    let mut context = Vec::new();
    let (mut walked, mut cached) = (false, false);
    if exists {
        let (iu, n, names) = in_use(sh, job);
        w.in_use = iu;
        w.in_use_procs = iu.map(|_| n);
        w.in_use_workloads = names;
        if iu.is_none() {
            w.notes.push("in-use not checked (--no-procs)".into());
        }

        let mut activity: Vec<i64> = vec![];
        if let Some(m) = mtime(&job.path) {
            activity.push(m);
        }
        if let Some(r) = repo.filter(|_| job.orphan.is_none()) {
            let tg = Instant::now();
            // git status: tracked changes and untracked files.
            match git::run(
                At::WorkTree(&job.path),
                &["status", "--porcelain=v1", "--untracked-files=normal"],
                t,
            )
            .ok()
            {
                Ok(o) => {
                    let c = git::parse_status(&o);
                    w.tracked_changes = Some(c.tracked);
                    w.untracked_files = Some(c.untracked);
                }
                Err(err) => w.notes.push(format!("status: {err}")),
            }
            // Commits reachable from HEAD that are on no remote-tracking ref.
            match &w.head {
                Some(h) => match git::run(
                    At::GitDir(&r.git_dir),
                    &["rev-list", "--count", h, "--not", "--remotes"],
                    t,
                )
                .ok()
                {
                    Ok(o) => match o.trim().parse() {
                        Ok(n) => w.unpushed_commits = Some(n),
                        Err(_) => w
                            .notes
                            .push(format!("rev-list: unexpected output '{}'", o.trim())),
                    },
                    Err(err) => w.notes.push(format!("rev-list: {err}")),
                },
                None => {
                    // Unborn branch: there are no commits at all, so none can be unpushed.
                    w.unpushed_commits = Some(0);
                    context.push("branch has no commits yet (unborn)".into());
                }
            }
            w.git_ms = tg.elapsed().as_millis() as u64;
            if r.info.has_remotes == Some(false) {
                context.push("repo has no remotes, so every commit counts as unpushed".into());
            }
            // Branch facts.
            if let Some(b) = w.branch.as_ref().and_then(|b| r.branches.get(b)) {
                w.upstream = b.upstream.clone();
                if let (Some(_), Some(tr)) = (&b.upstream, b.track) {
                    w.ahead = Some(tr.ahead);
                    w.behind = Some(tr.behind);
                    w.upstream_gone = tr.gone;
                    if tr.gone {
                        context.push(format!(
                            "upstream {} is gone (remote branch deleted; often a merged PR, possibly squash-merged)",
                            b.upstream.as_deref().unwrap_or("?")
                        ));
                    }
                }
                if let Some(ct) = b.commit_time {
                    activity.push(ct);
                }
            }
            if let Some(h) = &w.head {
                if let Some(ct) = r.commit_times.get(h) {
                    activity.push(*ct);
                }
            }
            // Merged into the default branch?
            w.merged = if w.detached {
                w.head
                    .as_ref()
                    .and_then(|h| r.detached_merged.get(h).copied().flatten())
            } else {
                match (&w.branch, &r.merged) {
                    (Some(b), Some(set)) => Some(set.contains(b)),
                    _ => None,
                }
            };
            if w.merged.is_none() && r.info.default_branch.is_some() {
                w.notes.push("merge state unknown".into());
            }
            // Merged GitHub PR from this branch (--pr).
            if let (Some(prs), Some(b)) = (&r.prs, &w.branch) {
                w.pr_merged = pr::match_branch(prs, b, w.head.as_deref());
                if let Some(p) = w.pr_merged.as_ref().filter(|p| !p.head_matches_tip) {
                    context.push(format!(
                        "PR #{} from a branch named {b} was merged {}, but at a different tip",
                        p.number, p.merged_at
                    ));
                }
            }
            // Not an ancestor: are its changes in the default branch anyway?
            if w.merged == Some(false) {
                let tg = Instant::now();
                w.content_merged = check_content(sh, r, w.head.as_deref(), &mut w.notes);
                w.git_ms += tg.elapsed().as_millis() as u64;
            }
            // Activity signals from the worktree's gitdir.
            let gitdir = if job.kind == "main" {
                Some(r.git_dir.clone())
            } else {
                std::fs::read_to_string(job.path.join(".git"))
                    .ok()
                    .and_then(|t| git::parse_git_file(&t, &job.path))
                    .map(|g| g.gitdir)
            };
            if let Some(gd) = gitdir {
                for f in ["index", "HEAD", "logs/HEAD"] {
                    if let Some(m) = mtime(&gd.join(f)) {
                        activity.push(m);
                    }
                }
            }
        }
        w.last_activity_unix = activity.into_iter().max();
        w.idle_secs = w.last_activity_unix.map(|a| (sh.now - a).max(0));
        (walked, cached) = measure(sh, job, &mut w);
    }
    if job.kind == "main" {
        context.insert(0, "main checkout".into());
    }
    let facts = Facts {
        exists,
        prunable: w.prunable.clone(),
        orphan: job.orphan.clone(),
        in_use: w.in_use,
        in_use_workloads: w.in_use_workloads.clone(),
        in_use_procs: w.in_use_procs.unwrap_or(0),
        tracked: w.tracked_changes,
        untracked: w.untracked_files,
        unpushed: w.unpushed_commits,
        merged: w.merged,
        content_merged: w.content_merged,
        pr_merged_at_tip: w
            .pr_merged
            .as_ref()
            .filter(|p| p.head_matches_tip)
            .map(|p| p.number),
        default_ref: w.default_branch.clone(),
        locked: w.locked,
        lock_reason: w.lock_reason.clone(),
        idle_secs: w.idle_secs,
        stale_days: sh.opts.stale_days,
        context,
    };
    let (v, reasons) = verdict::decide(&facts);
    for (on, flag) in [
        (w.upstream_gone, "upstream-gone"),
        (w.locked, "locked"),
        (w.detached, "detached"),
    ] {
        if on {
            w.flags.push(flag);
        }
    }
    w.verdict = v;
    w.reasons = reasons;
    (w, walked, cached)
}

/// Find and classify every worktree. Read-only apart from the size cache.
///
/// `live` is the list of live processes with a cwd (from procwho-proc); `None` means in-use
/// was not checked, and no worktree can then be labelled idle (`merged-idle`, `stale`).
pub fn scan(opts: &Options, live: Option<&[LiveUse]>) -> Report {
    let t0 = Instant::now();
    let now = now_unix();
    let mut notes: Vec<String> = Vec::new();

    // 1. Discovery.
    // On its own thread so the no-download I/O policy does not leak into the caller's thread.
    let (found, known_error) = std::thread::scope(|s| {
        s.spawn(|| {
            git::no_materialize_this_thread();
            let home = discover::home();
            let selected: Vec<_> = opts
                .roots
                .iter()
                .chain(&opts.containers)
                .map(|(p, _)| p.clone())
                .collect();
            let mut found = Found {
                access: discover::Access::new(home.clone(), &selected),
                ..Found::default()
            };
            let mut known = known::prune_known(known::load_known(&opts.state_dir), &home);
            for repo in discover::live_repositories(live.unwrap_or_default(), &home) {
                found
                    .sources
                    .insert(repo.clone(), discover::DiscoverySource::LiveProcess);
                found.repos.insert(repo.clone());
                known.push(known::KnownRepo {
                    path: repo_display(&repo).1,
                    last_seen_unix: now,
                });
            }
            for (folders, source) in [
                (&opts.containers, discover::DiscoverySource::Container),
                (&opts.roots, discover::DiscoverySource::Root),
            ] {
                for (root, depth) in folders {
                    discover::walk(root, *depth, &mut found);
                    for repo in &found.repos {
                        found.sources.entry(repo.clone()).or_insert(source);
                    }
                }
            }
            known = known::prune_known(known, &home);
            for entry in &known {
                if let Some(repo) = discover::repository_of(&entry.path) {
                    found
                        .sources
                        .entry(repo.clone())
                        .or_insert(discover::DiscoverySource::Remembered);
                    found.repos.insert(repo);
                }
            }
            let save_error =
                if !known.is_empty() || opts.state_dir.join("known-repos.json").exists() {
                    known::save_known(&opts.state_dir, &known).err()
                } else {
                    None
                };
            found.repos.retain(|repo| found.access.allows(repo));
            (found, save_error)
        })
        .join()
        .unwrap_or_default()
    });
    if found.protected_skipped > 0 {
        notes.push(format!("{} agent worktree Git pointer(s) into protected folders were not read; choose those folders explicitly to include them.", found.protected_skipped));
    }
    if let Some(e) = known_error {
        notes.push(format!("known repositories not saved: {e}"));
    }
    for r in &found.missing_roots {
        notes.push(format!(
            "not found or unreadable, skipped: {}",
            tilde(&r.display().to_string())
        ));
    }

    let mut phase_secs = BTreeMap::new();
    phase_secs.insert("1-discovery", t0.elapsed().as_secs_f64());
    let t1 = Instant::now();

    // 2. Per repo.
    let repo_dirs: Vec<PathBuf> = found.repos.iter().cloned().collect();
    let pr_cache = opts
        .pr
        .then(|| Mutex::new(pr::PrCache::load(&opts.state_dir)));
    let repos: Vec<RepoScan> = parallel_map(&repo_dirs, opts.jobs, |d| {
        let mut repo = scan_repo(d, opts, now, pr_cache.as_ref());
        repo.info.discovered_via = found
            .sources
            .get(d)
            .copied()
            .unwrap_or(discover::DiscoverySource::Root);
        repo
    });
    if let Some(c) = pr_cache {
        if let Err(e) = c.into_inner().unwrap_or_else(|e| e.into_inner()).save(now) {
            notes.push(format!("PR cache not saved: {e}"));
        }
    }
    let repo_index: HashMap<&Path, usize> = repos
        .iter()
        .enumerate()
        .map(|(i, r)| (r.git_dir.as_path(), i))
        .collect();

    // 3. Jobs: listed worktrees, then orphans among the `.git` files found on disk.
    let mut jobs: Vec<WtJob> = Vec::new();
    let mut listed: HashSet<PathBuf> = HashSet::new();
    for (ri, r) in repos.iter().enumerate() {
        let Some(entries) = &r.entries else { continue };
        for (i, e) in entries.iter().enumerate() {
            let path = PathBuf::from(&e.path);
            if !found.access.allows(&path) {
                notes.push("A linked worktree in a protected folder was not read; choose that folder explicitly to include it.".into());
                continue;
            }
            let c = canon(&path);
            listed.insert(c.clone());
            listed.insert(path.clone());
            if e.bare || (i == 0 && !opts.include_main) {
                continue;
            }
            jobs.push(WtJob {
                repo_idx: Some(ri),
                path,
                canon: c,
                kind: if i == 0 { "main" } else { "linked" },
                entry: Some(e.clone()),
                orphan: None,
                repo_name: r.info.name.clone(),
                repo_path: r.info.path.clone(),
            });
        }
    }
    let mut unparsed = 0;
    for gf in &found.git_files {
        let c = canon(&gf.dir);
        if listed.contains(&c) || listed.contains(&gf.dir) {
            continue;
        }
        let Some(gd) = &gf.gitdir else {
            unparsed += 1;
            continue;
        };
        if !found.access.allows(gd) {
            continue;
        }
        if gd.components().any(|x| x.as_os_str() == "modules") {
            continue; // submodule checkout, not a worktree
        }
        let common = found.access.common_dir(gd).map(|p| canon(&p));
        let (orphan, repo_idx, repo_dir) = match &common {
            Some(cd) => match repo_index.get(cd.as_path()) {
                Some(&ri) if repos[ri].entries.is_some() => {
                    let mut why = format!(
                        "main repo {} does not list this directory as a worktree",
                        tilde(&repos[ri].info.path)
                    );
                    // A copied worktree keeps the original's .git file: git run here would act on
                    // the other worktree's index and HEAD.
                    if let Ok(back) = std::fs::read_to_string(gd.join("gitdir")) {
                        let owner = Path::new(back.trim()).parent().map(canon);
                        if owner.as_ref().is_some_and(|o| *o != c) {
                            why.push_str(&format!(
                                "; its .git file points at the entry of {} (a copy of that worktree)",
                                tilde(&owner.unwrap_or_default().display().to_string())
                            ));
                        }
                    }
                    (why, Some(ri), repo_display(cd).1)
                }
                Some(&ri) => {
                    notes.push(format!(
                        "{}: repo {} could not be listed, not classified",
                        tilde(&gf.dir.display().to_string()),
                        tilde(&repos[ri].info.path)
                    ));
                    continue;
                }
                None => continue,
            },
            None => {
                let guess = git::split_worktree_gitdir(gd)
                    .map(|(c, _)| c)
                    .unwrap_or_else(|| gd.clone());
                let why = if guess.exists() {
                    format!(
                        "its gitdir {} no longer exists (worktree entry pruned)",
                        tilde(&gd.display().to_string())
                    )
                } else {
                    format!(
                        "main repo {} no longer exists",
                        tilde(&repo_display(&guess).1.display().to_string())
                    )
                };
                (why, None, repo_display(&guess).1)
            }
        };
        if !gf.dir.join(".git").is_file() {
            continue;
        }
        listed.insert(c.clone());
        jobs.push(WtJob {
            repo_idx,
            path: gf.dir.clone(),
            canon: c,
            kind: "orphan",
            entry: None,
            orphan: Some(orphan),
            repo_name: repo_display(&repo_dir.join(".git")).0,
            repo_path: repo_dir.display().to_string(),
        });
    }
    if unparsed > 0 {
        notes.push(format!(
            "{unparsed} director(ies) with an unreadable .git file were skipped"
        ));
    }

    phase_secs.insert("2-repos", t1.elapsed().as_secs_f64());
    let t2 = Instant::now();

    // 4. Per worktree, in the pool.
    let me = std::process::id() as i32;
    let live_canon: Option<Vec<(PathBuf, &LiveUse)>> = live.map(|l| {
        l.iter()
            .filter(|u| u.pid != me && !u.cwd.is_empty())
            .map(|u| (PathBuf::from(&u.cwd), u))
            .collect()
    });
    if live.is_none() {
        notes.push(
            "process snapshot skipped (--no-procs): in-use unknown, no worktree is labelled idle"
                .into(),
        );
    }
    let skip: HashSet<PathBuf> = jobs.iter().map(|j| j.canon.clone()).collect();
    let merge_tree = match git::git_version(opts.git_timeout) {
        Some(_) if !Path::new(git::SANDBOX_EXEC).exists() => Err(format!(
            "{} not found, merge-tree cannot be isolated",
            git::SANDBOX_EXEC
        )),
        Some(v) if v >= git::MERGE_TREE_MIN => Ok(()),
        Some((a, b)) => Err(format!(
            "git {a}.{b} has no merge-tree --write-tree (needs 2.38+)"
        )),
        None => Err("git version unknown".to_string()),
    };
    if let Err(why) = &merge_tree {
        notes.push(format!("squash/rebase merges not detected: {why}"));
    }
    let sh = Shared {
        opts,
        now,
        repos: &repos,
        live: live_canon,
        cache: Mutex::new(if opts.size {
            SizeCache::load(&opts.state_dir)
        } else {
            SizeCache::default()
        }),
        skip,
        merge_tree,
    };
    let results = parallel_map(&jobs, opts.jobs, |j| scan_worktree(&sh, j));
    if opts.size {
        let mut cache = sh.cache.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(e) = cache.save(now_unix()) {
            notes.push(format!("size cache not saved: {e}"));
        }
    }
    phase_secs.insert("3-worktrees", t2.elapsed().as_secs_f64());
    let sizes_walked = results.iter().filter(|r| r.1).count();
    let sizes_cached = results.iter().filter(|r| r.2).count();
    let mut wts: Vec<Worktree> = results.into_iter().map(|r| r.0).collect();
    let worktrees_found = wts.len();

    // 5. Filters, sort, totals, notes.
    let mut filter = Vec::new();
    if let Some(v) = opts.verdict {
        wts.retain(|w| w.verdict == v);
        filter.push(format!("verdict={}", v.as_str()));
    }
    if let Some(r) = &opts.repo {
        let r = r.to_lowercase();
        wts.retain(|w| w.repo.to_lowercase().contains(&r));
        filter.push(format!("repo~{r}"));
    }
    match opts.sort {
        Sort::Size => wts.sort_by(|a, b| {
            b.size_bytes
                .cmp(&a.size_bytes)
                .then(b.idle_secs.cmp(&a.idle_secs))
        }),
        Sort::Age => wts.sort_by(|a, b| {
            b.idle_secs
                .cmp(&a.idle_secs)
                .then(b.size_bytes.cmp(&a.size_bytes))
        }),
        Sort::Repo => wts.sort_by(|a, b| a.repo.cmp(&b.repo).then(a.path.cmp(&b.path))),
    }
    let mut by: BTreeMap<Verdict, VerdictTotal> = BTreeMap::new();
    for w in &wts {
        let t = by.entry(w.verdict).or_insert(VerdictTotal {
            verdict: w.verdict,
            count: 0,
            size_bytes: 0,
            rebuildable_bytes: 0,
            size_unknown: 0,
        });
        t.count += 1;
        match w.size_bytes {
            Some(b) => {
                t.size_bytes += b;
                t.rebuildable_bytes += w.rebuildable_bytes.unwrap_or(0);
            }
            None => t.size_unknown += 1,
        }
    }
    let timeouts = wts
        .iter()
        .flat_map(|w| &w.notes)
        .filter(|n| n.contains("timed out"))
        .count()
        + repos
            .iter()
            .flat_map(|r| &r.info.notes)
            .filter(|n| n.contains("timed out"))
            .count();
    if timeouts > 0 {
        notes.push(format!(
            "{timeouts} git call(s) timed out after {}s; those fields are unknown",
            opts.git_timeout.as_secs()
        ));
    }
    let repo_errs = repos.iter().filter(|r| r.entries.is_none()).count();
    if repo_errs > 0 {
        notes.push(format!(
            "{repo_errs} repo(s) could not be listed (see repos[].notes)"
        ));
    }
    let used: HashSet<&str> = wts.iter().map(|w| w.repo_path.as_str()).collect();
    let oldest_fetch = repos
        .iter()
        .filter(|r| used.contains(r.info.path.as_str()))
        .filter_map(|r| r.info.fetch_age_secs)
        .max();
    let never = repos
        .iter()
        .filter(|r| used.contains(r.info.path.as_str()) && r.info.fetched_at.is_none())
        .count();
    let mut fetch_note =
        "merged / ahead / unpushed are only as fresh as each repo's last fetch (procwho never fetches)".to_string();
    if let Some(a) = oldest_fetch {
        fetch_note.push_str(&format!(
            "; oldest fetch among listed repos: {} ago",
            fmt_age(a)
        ));
    }
    if never > 0 {
        fetch_note.push_str(&format!("; {never} repo(s) never fetched"));
    }
    notes.push(fetch_note);
    notes.push(
        "merged = tip is an ancestor of the default branch; content_merged = merging the branch into the \
         default branch would change nothing (squash or rebase merge, or no net changes). Both compare \
         with the local copy of the default branch, so a branch merged after the last fetch shows as not \
         merged"
            .into(),
    );
    if opts.size {
        notes.push(
            "sizes are allocated blocks (lstat, hard links once, symlinks not followed); APFS clones can make \
             the sum larger than what removal would free"
                .into(),
        );
    }
    if opts.pr {
        let checked = repo_infos_with(&repos, &used, |r| r.prs.is_some());
        let skipped = repo_infos_with(&repos, &used, |r| r.prs.is_none());
        notes.push(format!(
            "PR state read with gh for {checked} repo(s), not for {skipped} (see repos[].notes); a merged PR \
             counts as merged only when it was merged at exactly the worktree's tip"
        ));
    } else {
        notes.push("PR state was not checked (use --pr to ask GitHub via gh)".into());
    }
    notes.push("verdicts are review labels, not instructions".into());

    let repo_infos: Vec<RepoInfo> = repos.into_iter().map(|r| r.info).collect();
    Report {
        reclaimable_estimated: true,
        reclaimable_note: "Up to the measured merged-idle size; APFS clones share blocks, so removal can free less. Unmeasured sizes are excluded.",
        schema_version: SCHEMA_VERSION,
        generated_at: now,
        observed_at_unix: now,
        collect_ms: t0.elapsed().as_millis() as u64,
        generated_at_utc: utc(now),
        scan_secs: t0.elapsed().as_secs_f64(),
        repos_found: found.repos.len(),
        worktrees_found,
        searched_folders: opts.roots.iter().chain(&opts.containers).map(|(p, _)| p.to_string_lossy().into_owned()).collect(),
        totals: by.into_values().collect(),
        repos: repo_infos,
        worktrees: wts,
        sizes_walked,
        sizes_cached,
        filter: (!filter.is_empty()).then(|| filter.join(", ")),
        phase_secs,
        notes,
    }
}

fn repo_infos_with(
    repos: &[RepoScan],
    used: &HashSet<&str>,
    f: impl Fn(&RepoScan) -> bool,
) -> usize {
    repos
        .iter()
        .filter(|r| used.contains(r.info.path.as_str()) && f(r))
        .count()
}

/// `~` for the home directory prefix.
pub fn tilde(p: &str) -> String {
    let h = discover::home();
    let h = h.to_string_lossy();
    if h.len() > 1 {
        if let Some(rest) = p.strip_prefix(h.as_ref()) {
            if rest.is_empty() || rest.starts_with('/') {
                return format!("~{rest}");
            }
        }
    }
    p.to_string()
}

/// Unix seconds as `YYYY-MM-DDTHH:MM:SSZ`.
///
/// ```
/// assert_eq!(procwho_wt::utc(0), "1970-01-01T00:00:00Z");
/// assert_eq!(procwho_wt::utc(1763000000), "2025-11-13T02:13:20Z");
/// ```
pub fn utc(t: i64) -> String {
    let (days, secs) = (t.div_euclid(86400), t.rem_euclid(86400));
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

#[cfg(test)]
mod tests;
