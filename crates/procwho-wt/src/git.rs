//! Read-only git invocation with a timeout, and parsers for the outputs procwho reads.
//!
//! Every call passes `-c core.fsmonitor=false -c core.untrackedCache=false --no-optional-locks`
//! so procwho never starts an fsmonitor daemon and never takes or writes the index lock.

use std::ffi::OsStr;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

/// Flags that go before every git subcommand.
pub const SAFE_FLAGS: [&str; 5] = [
    "-c",
    "core.fsmonitor=false",
    "-c",
    "core.untrackedCache=false",
    "--no-optional-locks",
];

/// Where a git command runs.
#[derive(Debug, Clone, Copy)]
pub enum At<'a> {
    /// `--git-dir=<common dir>`: repository-level reads (refs, worktree list).
    GitDir(&'a Path),
    /// `-C <worktree>`: commands that need a work tree (status).
    WorkTree(&'a Path),
}

/// Result of one git call.
#[derive(Debug)]
pub enum GitOut {
    /// Exit status code and stdout.
    Done {
        code: i32,
        stdout: String,
        stderr: String,
    },
    /// Killed after the timeout.
    TimedOut,
    /// The executable is not installed.
    NotInstalled,
    /// Could not start git at all.
    SpawnFailed(String),
}

impl GitOut {
    /// Stdout when the command exited 0, otherwise a short error description.
    pub fn ok(self) -> Result<String, String> {
        match self {
            GitOut::Done {
                code: 0, stdout, ..
            } => Ok(stdout),
            GitOut::Done { code, stderr, .. } => {
                let first = stderr.lines().next().unwrap_or("").trim().to_string();
                let hint = if stderr.contains("Resource deadlock avoided") {
                    " (iCloud file not downloaded; procwho never triggers downloads)"
                } else {
                    ""
                };
                Err(format!("git exit {code}: {first}{hint}"))
            }
            GitOut::TimedOut => Err("git timed out".into()),
            GitOut::NotInstalled => Err("git is not installed".into()),
            GitOut::SpawnFailed(e) => Err(format!("git failed to start: {e}")),
        }
    }
}

/// Run one read-only git command. Never hangs past `timeout`: on expiry the whole process
/// group of the child is killed and [`GitOut::TimedOut`] is returned.
pub fn run(at: At<'_>, args: &[&str], timeout: Duration) -> GitOut {
    run_env(at, args, &[], timeout)
}

/// [`run`] with extra environment variables for the child.
pub fn run_env(at: At<'_>, args: &[&str], env: &[(&str, &OsStr)], timeout: Duration) -> GitOut {
    exec(git_command(Command::new("git"), at, args, env), timeout)
}

/// Add the safe flags, location, arguments and environment for one git call to `cmd`, which
/// is either `git` itself or a wrapper whose next argument is `git`.
fn git_command(mut cmd: Command, at: At<'_>, args: &[&str], env: &[(&str, &OsStr)]) -> Command {
    cmd.args(SAFE_FLAGS);
    match at {
        At::GitDir(p) => {
            cmd.arg(format!("--git-dir={}", p.display()));
        }
        At::WorkTree(p) => {
            cmd.arg("-C").arg(p);
        }
    }
    cmd.args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES");
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd
}

/// Run any command with stdin closed, its own process group, iCloud materialization off and
/// a hard timeout. Used for git and for the optional `gh` call.
pub fn exec(mut cmd: Command, timeout: Duration) -> GitOut {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .process_group(0);
    // SAFETY: the closure runs in the forked child before exec and only makes one raw
    // syscall wrapper (setiopolicy_np), which is async-signal-safe and touches no Rust state.
    unsafe {
        cmd.pre_exec(|| {
            no_materialize();
            Ok(())
        });
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return GitOut::NotInstalled,
        Err(e) => return GitOut::SpawnFailed(e.to_string()),
    };
    let pgid = child.id() as i32;
    let mut out = child.stdout.take().expect("piped stdout");
    let mut err = child.stderr.take().expect("piped stderr");
    let (tx, rx) = mpsc::channel();
    // stderr is drained on its own thread so a chatty stderr can never block stdout.
    let err_thread = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = err.by_ref().take(64 * 1024).read_to_end(&mut s);
        let _ = std::io::copy(&mut err, &mut std::io::sink());
        s
    });
    std::thread::spawn(move || {
        let mut s = Vec::new();
        let r = out.read_to_end(&mut s);
        let _ = tx.send(r.map(|_| s));
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(stdout)) => {
            let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
            let stderr = err_thread.join().unwrap_or_default();
            GitOut::Done {
                code,
                stdout: String::from_utf8_lossy(&stdout).into_owned(),
                stderr: String::from_utf8_lossy(&stderr).into_owned(),
            }
        }
        Ok(Err(e)) => {
            kill_group(pgid);
            let _ = child.wait();
            GitOut::SpawnFailed(format!("reading output: {e}"))
        }
        Err(_) => {
            kill_group(pgid);
            let _ = child.wait();
            GitOut::TimedOut
        }
    }
}

extern "C" {
    // <sys/resource.h>; not exported by the libc crate.
    fn setiopolicy_np(iotype: libc::c_int, scope: libc::c_int, policy: libc::c_int) -> libc::c_int;
}
const IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES: libc::c_int = 3;
const IOPOL_SCOPE_PROCESS: libc::c_int = 0;
const IOPOL_MATERIALIZE_DATALESS_FILES_OFF: libc::c_int = 1;

/// Make reads of iCloud "dataless" (evicted) files fail instead of downloading them. Without
/// this, `git status` in a worktree under an iCloud-synced folder (e.g. ~/Documents) blocks on
/// a download and writes the file's contents to disk.
fn no_materialize() {
    // SAFETY: plain syscall wrapper with integer arguments; failure is ignored (best effort).
    unsafe {
        setiopolicy_np(
            IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES,
            IOPOL_SCOPE_PROCESS,
            IOPOL_MATERIALIZE_DATALESS_FILES_OFF,
        );
    }
}

/// Same policy for the calling thread only (used by procwho's own walker threads, so reading
/// a dataless directory fails instead of downloading it, without changing the host process).
pub fn no_materialize_this_thread() {
    // SAFETY: as above, thread scope.
    unsafe {
        setiopolicy_np(
            IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES,
            1,
            IOPOL_MATERIALIZE_DATALESS_FILES_OFF,
        );
    }
}

fn kill_group(pgid: i32) {
    // SAFETY: kill(2) with a negative pid signals the process group we created for this
    // child (process_group(0) makes the child its own group leader); no memory is touched.
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
    }
}

/// `(major, minor)` from `git version 2.56.0` (Apple builds add ` (Apple Git-NNN)`).
///
/// ```
/// use procwho_wt::git::parse_git_version;
/// assert_eq!(parse_git_version("git version 2.56.0\n"), Some((2, 56)));
/// assert_eq!(parse_git_version("git version 2.39.5 (Apple Git-154)"), Some((2, 39)));
/// assert_eq!(parse_git_version("nope"), None);
/// ```
pub fn parse_git_version(s: &str) -> Option<(u32, u32)> {
    let v = s
        .trim()
        .strip_prefix("git version ")?
        .split_whitespace()
        .next()?;
    let mut it = v.split('.');
    Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
}

/// The installed git's `(major, minor)`, or `None` when it cannot be read.
pub fn git_version(timeout: Duration) -> Option<(u32, u32)> {
    let mut cmd = Command::new("git");
    cmd.arg("--version").env("LC_ALL", "C");
    exec(cmd, timeout)
        .ok()
        .ok()
        .and_then(|s| parse_git_version(&s))
}

/// `git merge-tree --write-tree` needs git 2.38 or newer.
pub const MERGE_TREE_MIN: (u32, u32) = (2, 38);

/// macOS sandbox wrapper used to make merge-tree unable to write outside its temp dir.
pub const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// Seatbelt profile: allow everything except file writes, which are allowed only under
/// `dir` (and to `/dev/null`).
fn deny_writes_except(dir: &Path) -> String {
    let d = dir
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!(
        "(version 1)(allow default)(deny file-write*)(allow file-write* (subpath \"{d}\"))\
         (allow file-write-data (literal \"/dev/null\"))"
    )
}

/// A fresh, empty object directory under the system temp dir, removed on drop.
struct TempObjects(PathBuf);

impl TempObjects {
    fn new(parent: &Path) -> std::io::Result<TempObjects> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        let p = parent.join(format!(
            "procwho-mt-{}-{nanos}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p)?;
        Ok(TempObjects(p))
    }
}

impl Drop for TempObjects {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Is every change on `tip` already in `default_ref`? Decided by merging the two in memory
/// (`git merge-tree --write-tree`) and comparing the result with `default_tree` (the tree of
/// `default_ref`): equal trees mean merging would change nothing. A conflict counts as "not
/// contained".
///
/// merge-tree writes the merged trees as objects. To keep the repository untouched, the
/// child gets `GIT_OBJECT_DIRECTORY=<fresh temp dir>` and the repository's real object
/// directory `objects` only as an alternate, so new objects land in the temp dir, which is
/// removed before this returns. That alone is not enough: when an object it would write
/// already exists in the alternate, git "freshens" it (sets its mtime) instead. So the child
/// also runs under `sandbox-exec` with every file write denied except inside the temp dir;
/// the freshen then fails and git writes the object to the temp dir instead.
pub fn content_merged(
    git_dir: &Path,
    objects: &Path,
    default_ref: &str,
    default_tree: &str,
    tip: &str,
    timeout: Duration,
) -> Result<bool, String> {
    content_merged_in(
        &std::env::temp_dir(),
        git_dir,
        objects,
        default_ref,
        default_tree,
        tip,
        timeout,
    )
}

/// [`content_merged`] with the temp object dir created under `tmp_parent`.
pub(crate) fn content_merged_in(
    tmp_parent: &Path,
    git_dir: &Path,
    objects: &Path,
    default_ref: &str,
    default_tree: &str,
    tip: &str,
    timeout: Duration,
) -> Result<bool, String> {
    let tmp = TempObjects::new(tmp_parent).map_err(|e| format!("merge-tree temp dir: {e}"))?;
    let real_tmp =
        std::fs::canonicalize(&tmp.0).map_err(|e| format!("merge-tree temp dir: {e}"))?;
    let env = [
        ("GIT_OBJECT_DIRECTORY", real_tmp.as_os_str()),
        ("GIT_ALTERNATE_OBJECT_DIRECTORIES", objects.as_os_str()),
    ];
    let mut wrap = Command::new(SANDBOX_EXEC);
    wrap.arg("-p").arg(deny_writes_except(&real_tmp)).arg("git");
    let cmd = git_command(
        wrap,
        At::GitDir(git_dir),
        &[
            "merge-tree",
            "--write-tree",
            "--no-messages",
            "--name-only",
            default_ref,
            tip,
        ],
        &env,
    );
    let out = exec(cmd, timeout);
    // Exit 0 (clean) and 1 (conflicts) both print the result tree first; exit 1 with no tree
    // is an error (e.g. "not something we can merge").
    match out {
        GitOut::Done {
            code: code @ (0 | 1),
            ref stdout,
            ..
        } if is_oid(stdout.lines().next().unwrap_or("")) => {
            Ok(code == 0 && stdout.lines().next().unwrap_or("").trim() == default_tree)
        }
        other => Err(format!(
            "merge-tree: {}",
            other.ok().err().unwrap_or_else(|| "no result tree".into())
        )),
    }
}

fn is_oid(s: &str) -> bool {
    let s = s.trim();
    (s.len() == 40 || s.len() == 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// What a `.git` file says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFile {
    /// Absolute gitdir path (relative paths resolved against the worktree directory).
    pub gitdir: PathBuf,
}

/// Parse the contents of a `.git` file (`gitdir: <path>`). `dir` is the directory holding it.
///
/// ```
/// use procwho_wt::git::parse_git_file;
/// use std::path::Path;
/// let g = parse_git_file("gitdir: /r/.git/worktrees/x\n", Path::new("/w/x")).unwrap();
/// assert_eq!(g.gitdir, Path::new("/r/.git/worktrees/x"));
/// ```
pub fn parse_git_file(text: &str, dir: &Path) -> Option<GitFile> {
    let line = text.lines().next()?.trim_end_matches('\r');
    let rest = line.strip_prefix("gitdir:")?.trim();
    if rest.is_empty() {
        return None;
    }
    let p = Path::new(rest);
    let gitdir = if p.is_absolute() {
        p.to_path_buf()
    } else {
        dir.join(p)
    };
    Some(GitFile { gitdir })
}

/// If `gitdir` looks like `<common>/worktrees/<name>`, return `(common, name)`.
/// Submodule gitdirs (`.../modules/...`) return `None`.
pub fn split_worktree_gitdir(gitdir: &Path) -> Option<(PathBuf, String)> {
    let name = gitdir.file_name()?.to_str()?.to_string();
    let parent = gitdir.parent()?;
    if parent.file_name()? != "worktrees" {
        return None;
    }
    if gitdir.components().any(|c| c.as_os_str() == "modules") {
        return None;
    }
    Some((parent.parent()?.to_path_buf(), name))
}

/// One entry of `git worktree list --porcelain`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WtEntry {
    pub path: String,
    pub head: Option<String>,
    /// Short branch name (`refs/heads/` stripped).
    pub branch: Option<String>,
    pub detached: bool,
    pub bare: bool,
    pub locked: Option<String>,
    pub prunable: Option<String>,
}

/// Parse `git worktree list --porcelain` output, with `-z` (NUL separated) or without
/// (newline separated). The first entry is the main worktree.
pub fn parse_worktree_list(text: &str) -> Vec<WtEntry> {
    let sep = if text.contains('\0') { '\0' } else { '\n' };
    let mut out = Vec::new();
    let mut cur: Option<WtEntry> = None;
    for line in text.split(sep) {
        if line.is_empty() {
            if let Some(e) = cur.take() {
                out.push(e);
            }
            continue;
        }
        let (key, val) = match line.split_once(' ') {
            Some((k, v)) => (k, Some(v)),
            None => (line, None),
        };
        if key == "worktree" {
            if let Some(e) = cur.take() {
                out.push(e);
            }
            cur = Some(WtEntry {
                path: val.unwrap_or("").to_string(),
                ..Default::default()
            });
            continue;
        }
        let Some(e) = cur.as_mut() else { continue };
        match key {
            "HEAD" => e.head = val.map(str::to_string),
            "branch" => {
                e.branch = val.map(|b| b.strip_prefix("refs/heads/").unwrap_or(b).to_string())
            }
            "detached" => e.detached = true,
            "bare" => e.bare = true,
            "locked" => e.locked = Some(val.unwrap_or("").to_string()),
            "prunable" => e.prunable = Some(val.unwrap_or("").to_string()),
            _ => {}
        }
    }
    if let Some(e) = cur {
        out.push(e);
    }
    out
}

/// Parsed `%(upstream:track)`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Track {
    pub ahead: u32,
    pub behind: u32,
    pub gone: bool,
}

/// Parse `%(upstream:track)`: `[ahead 2, behind 1]`, `[ahead 2]`, `[behind 1]`, `[gone]` or
/// empty (in sync, or no upstream). Returns `None` for text it does not understand.
///
/// ```
/// use procwho_wt::git::{parse_track, Track};
/// assert_eq!(parse_track("[ahead 2, behind 1]"), Some(Track { ahead: 2, behind: 1, gone: false }));
/// assert_eq!(parse_track("[gone]"), Some(Track { ahead: 0, behind: 0, gone: true }));
/// assert_eq!(parse_track(""), Some(Track::default()));
/// ```
pub fn parse_track(s: &str) -> Option<Track> {
    let s = s.trim();
    if s.is_empty() {
        return Some(Track::default());
    }
    let inner = s.strip_prefix('[')?.strip_suffix(']')?;
    let mut t = Track::default();
    for part in inner.split(',').map(str::trim) {
        if part == "gone" {
            t.gone = true;
        } else if let Some(n) = part.strip_prefix("ahead ") {
            t.ahead = n.parse().ok()?;
        } else {
            let n = part.strip_prefix("behind ")?;
            t.behind = n.parse().ok()?;
        }
    }
    Some(t)
}

/// One local branch from `for-each-ref`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchRef {
    pub name: String,
    pub sha: String,
    pub upstream: Option<String>,
    pub track: Option<Track>,
    pub commit_time: Option<i64>,
}

/// Format string for [`parse_for_each_ref`].
pub const BRANCH_FORMAT: &str =
    "--format=%(refname:short)%09%(objectname)%09%(upstream:short)%09%(upstream:track)%09%(committerdate:unix)";

/// Parse the output of `git for-each-ref refs/heads` with [`BRANCH_FORMAT`].
pub fn parse_for_each_ref(text: &str) -> Vec<BranchRef> {
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split('\t').collect();
            if f.len() < 5 {
                return None;
            }
            Some(BranchRef {
                name: f[0].to_string(),
                sha: f[1].to_string(),
                upstream: (!f[2].is_empty()).then(|| f[2].to_string()),
                track: parse_track(f[3]),
                commit_time: f[4].trim().parse().ok(),
            })
        })
        .collect()
}

/// Counts from `git status --porcelain=v1`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusCounts {
    pub tracked: u32,
    pub untracked: u32,
}

/// Count tracked changes and untracked files in `git status --porcelain=v1` output.
/// Ignored entries (`!!`) are not counted.
pub fn parse_status(text: &str) -> StatusCounts {
    let mut c = StatusCounts::default();
    for l in text.lines() {
        if l.starts_with("??") {
            c.untracked += 1;
        } else if l.starts_with("!!") || l.len() < 3 {
            continue;
        } else {
            c.tracked += 1;
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_file_parsing() {
        let d = Path::new("/w/x");
        assert_eq!(
            parse_git_file("gitdir: /r/.git/worktrees/x", d)
                .unwrap()
                .gitdir,
            Path::new("/r/.git/worktrees/x")
        );
        assert_eq!(
            parse_git_file("gitdir: ../r/.git/worktrees/x\r\n", d)
                .unwrap()
                .gitdir,
            Path::new("/w/x/../r/.git/worktrees/x")
        );
        assert!(parse_git_file("not a git file", d).is_none());
        assert!(parse_git_file("gitdir:   ", d).is_none());
        assert!(parse_git_file("", d).is_none());

        let (c, n) = split_worktree_gitdir(Path::new("/r/.git/worktrees/feat-1")).unwrap();
        assert_eq!(c, Path::new("/r/.git"));
        assert_eq!(n, "feat-1");
        assert!(split_worktree_gitdir(Path::new("/r/.git/modules/sub")).is_none());
        assert!(split_worktree_gitdir(Path::new("/r/.git/modules/sub/worktrees/x")).is_none());
    }

    #[test]
    fn worktree_list_porcelain() {
        let text = "worktree /r\nHEAD 1111\nbranch refs/heads/main\n\n\
                    worktree /w/a\nHEAD 2222\nbranch refs/heads/feat/a\nlocked\n\n\
                    worktree /w/b\nHEAD 3333\ndetached\nprunable gitdir file points to non-existent location\n\n\
                    worktree /w/c\nHEAD 4444\nbranch refs/heads/c\nlocked agent session 12\n\n";
        let e = parse_worktree_list(text);
        assert_eq!(e.len(), 4);
        assert_eq!(e[0].branch.as_deref(), Some("main"));
        assert_eq!(e[1].branch.as_deref(), Some("feat/a"));
        assert_eq!(e[1].locked.as_deref(), Some(""));
        assert!(e[2].detached && e[2].branch.is_none());
        assert_eq!(
            e[2].prunable.as_deref(),
            Some("gitdir file points to non-existent location")
        );
        assert_eq!(e[3].locked.as_deref(), Some("agent session 12"));

        let z =
            "worktree /r\0HEAD 1111\0bare\0\0worktree /w/with space\0HEAD 2\0detached\0locked\0\0";
        let e = parse_worktree_list(z);
        assert_eq!(e.len(), 2);
        assert!(e[0].bare);
        assert_eq!(e[1].path, "/w/with space");
        assert!(e[1].detached && e[1].locked.is_some());
    }

    #[test]
    fn upstream_track() {
        assert_eq!(
            parse_track("[ahead 2, behind 1]"),
            Some(Track {
                ahead: 2,
                behind: 1,
                gone: false
            })
        );
        assert_eq!(
            parse_track("[ahead 7]"),
            Some(Track {
                ahead: 7,
                behind: 0,
                gone: false
            })
        );
        assert_eq!(
            parse_track("[behind 3]"),
            Some(Track {
                ahead: 0,
                behind: 3,
                gone: false
            })
        );
        assert_eq!(
            parse_track("[gone]"),
            Some(Track {
                ahead: 0,
                behind: 0,
                gone: true
            })
        );
        assert_eq!(parse_track(""), Some(Track::default()));
        assert_eq!(parse_track("[weird]"), None);
    }

    #[test]
    fn for_each_ref_and_status() {
        let t = "main\tabc\torigin/main\t\t1700000000\nfeat\tdef\torigin/feat\t[gone]\t1700000100\nloc\t123\t\t\t1700000200\n";
        let b = parse_for_each_ref(t);
        assert_eq!(b.len(), 3);
        assert_eq!(b[0].upstream.as_deref(), Some("origin/main"));
        assert!(b[1].track.unwrap().gone);
        assert!(b[2].upstream.is_none());
        assert_eq!(b[2].commit_time, Some(1700000200));

        let s = parse_status(" M a.rs\nM  b.rs\n?? new.txt\n?? dir/\nR  old -> new\n!! ignored\n");
        assert_eq!(
            s,
            StatusCounts {
                tracked: 3,
                untracked: 2
            }
        );
    }
}
