//! Data collection: libproc pass, helper commands, Codex rollout names, git project lookup.
//!
//! Helpers: `/bin/launchctl list`, `/usr/sbin/lsof` (listening TCP, established TCP,
//! git fsmonitor fd 4) and `tmux list-panes`. Label providers are explicitly configured.

use crate::fmt::sanitize;
use crate::model::*;
use crate::redact::redact;
use crate::sys::{self, run, Reader};
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

// ------------------------------------------------------------------ libproc pass

/// Reads every visible process. Same-uid processes get argv, cwd and counters;
/// other users' processes only get parent, uid and name.
pub fn read_procs() -> Vec<Proc> {
    let mut rd = Reader::new();
    let pids = rd.all_pids();
    let mut out = Vec::with_capacity(pids.len());
    for pid in pids {
        let (uniq, puniq) = rd.uniq(pid).unwrap_or((0, 0));
        let mut exe = rd.exe_path(pid);
        let Some((b, readable)) = rd.basic(pid) else {
            continue;
        };
        let mut p = Proc {
            pid,
            ppid: b.ppid,
            uid: b.uid,
            pgid: b.pgid,
            uniq,
            puniq,
            comm: b.comm,
            exe: String::new(),
            exe_gone: false,
            readable,
            argv: Vec::new(),
            raw: String::new(),
            env: Env::default(),
            cmd: String::new(),
            cwd: String::new(),
            tty: String::new(),
            fp: 0,
            rss: 0,
            usage_available: false,
            fp_max: 0,
            cpu_ns: 0,
            start: b.start,
            zombie: b.zombie,
            ports: Vec::new(),
            label: String::new(),
            resp: -1,
            wl: 0,
        };
        if readable {
            p.tty = sys::tty_name(b.tty_dev);
            if let Some((argv, env)) = rd.args(pid) {
                p.argv = argv;
                p.env = env;
            }
            p.cwd = rd.cwd(pid);
            if let Some(u) = rd.usage(pid) {
                p.usage_available = true;
                p.fp = u.footprint;
                p.rss = u.resident;
                p.cpu_ns = rd.ticks_to_ns(u.cpu_ticks);
                p.fp_max = u.life_max_footprint;
            }
            p.resp = rd.responsible(pid);
        }
        // The reads above are separate syscalls: if the PID was reused meanwhile, they may
        // mix two processes. The unique id is never reused, so compare it again.
        if !same_instance(uniq, rd.uniq(pid)) {
            continue;
        }
        p.raw = p.argv.join(" "); // matching only; never printed or stored
        if readable && exe.is_empty() && p.argv.first().is_some_and(|a| a.starts_with('/')) {
            // The kernel has no path: the file was replaced or removed after start.
            exe = display_argv0(&p.argv[0]);
            p.exe_gone = true;
        }
        p.exe = exe;
        // Display-only field: redacted and free of terminal control characters.
        let command = if !p.argv.is_empty() {
            p.raw.clone()
        } else if !p.exe.is_empty() {
            p.exe.clone()
        } else {
            format!("({})", p.comm)
        };
        p.cmd = sanitize(&redact(&command));
        out.push(p);
    }
    out
}

/// `argv[0]` is chosen by whoever started the process, so it is redacted and stripped of
/// control characters before it stands in for the executable or appears in evidence.
pub(crate) fn display_argv0(a: &str) -> String {
    sanitize(&redact(a))
}

/// True when a second unique-id read shows the same process instance as the first.
/// An unknown first id (0) cannot be checked and is accepted.
fn same_instance(before: u64, after: Option<(u64, u64)>) -> bool {
    before == 0 || after.is_some_and(|(u, _)| u == before)
}

/// Fresh (unique id, footprint-independent CPU ns) for a pid; used by the CPU rate pass.
pub fn read_cpu(rd: &Reader, pid: i32) -> Option<(u64, u64)> {
    let (u, _) = rd.uniq(pid)?;
    let r = rd.usage(pid)?;
    Some((u, rd.ticks_to_ns(r.cpu_ticks)))
}

// ------------------------------------------------------------------ helper commands

/// `launchctl list` -> pid -> label. Second value is false if the command failed.
pub fn launchd_labels() -> (HashMap<i32, String>, bool) {
    let Some(out) = run(&["/bin/launchctl", "list"], 15) else {
        return (HashMap::new(), false);
    };
    let mut m = HashMap::new();
    for line in out.lines().skip(1) {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() == 3 && !f[0].is_empty() && f[0].bytes().all(|c| c.is_ascii_digit()) {
            if let Ok(pid) = f[0].parse::<i32>() {
                m.insert(pid, f[2].to_string());
            }
        }
    }
    (m, true)
}

fn truthy(v: &plist::Value) -> bool {
    use plist::Value::*;
    match v {
        Boolean(b) => *b,
        Dictionary(d) => !d.is_empty(),
        Array(a) => !a.is_empty(),
        String(s) => !s.is_empty(),
        Integer(i) => i.as_signed().map(|x| x != 0).unwrap_or(true),
        Real(r) => *r != 0.0,
        Data(d) => !d.is_empty(),
        _ => true,
    }
}

/// Facts from the launchd job's plist (first plist found among the three standard dirs).
/// A label that contains `/` or starts with `.` could point outside the LaunchAgents
/// directories (`../x`), so it gets no plist lookup at all.
pub fn plist_meta(cfg: &Config, label: &str) -> PlistMeta {
    if label.is_empty() || label.contains('/') || label.starts_with('.') || label.contains('\0') {
        return PlistMeta::default();
    }
    let dirs = [
        format!("{}/Library/LaunchAgents", cfg.home),
        "/Library/LaunchAgents".to_string(),
        "/Library/LaunchDaemons".to_string(),
    ];
    for d in dirs {
        let f = Path::new(&d).join(format!("{label}.plist"));
        if !f.exists() {
            continue;
        }
        let shown = f.to_string_lossy().into_owned();
        let Ok(plist::Value::Dictionary(pl)) = plist::Value::from_file(&f) else {
            return PlistMeta {
                plist: Some(shown),
                error: Some("unreadable plist".into()),
                ..Default::default()
            };
        };
        let get = |k: &str| pl.get(k).filter(|v| truthy(v));
        let s = |k: &str| pl.get(k).and_then(|v| v.as_string()).map(str::to_string);
        let keepalive = get("KeepAlive").is_some();
        let schedule = if keepalive {
            "KeepAlive"
        } else if get("StartInterval").is_some() || get("StartCalendarInterval").is_some() {
            "scheduled intervals or calendar times"
        } else if get("RunAtLoad").is_some() {
            "RunAtLoad"
        } else {
            "on demand"
        };
        return PlistMeta {
            plist: Some(shown),
            error: None,
            workdir: s("WorkingDirectory"),
            stdout: s("StandardOutPath"),
            stderr: s("StandardErrorPath"),
            keepalive,
            schedule: Some(schedule.to_string()),
        };
    }
    PlistMeta::default()
}

/// A booted simulator as named by its `launchd_sim` command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimDevice {
    pub udid: String,
    /// `<...>/CoreSimulator/Devices/<udid>/device.plist`
    pub plist: String,
}

/// The device UDID from a `launchd_sim` argv:
/// `launchd_sim <...>/CoreSimulator/Devices/<UDID>/data/var/run/launchd_bootstrap.plist`.
/// Returns `None` for any other command, a relative or `..` path, or a malformed UDID.
///
/// ```
/// use procwho_proc::collect::sim_device_of;
/// let a: Vec<String> = ["launchd_sim", "/Users/user/Library/Developer/CoreSimulator/Devices/AAAAAAAA-1111-2222-3333-BBBBBBBBBBBB/data/var/run/launchd_bootstrap.plist"]
///     .iter().map(|s| s.to_string()).collect();
/// let d = sim_device_of(&a).unwrap();
/// assert_eq!(d.udid, "AAAAAAAA-1111-2222-3333-BBBBBBBBBBBB");
/// assert!(d.plist.ends_with("/Devices/AAAAAAAA-1111-2222-3333-BBBBBBBBBBBB/device.plist"));
/// assert!(sim_device_of(&a[..1]).is_none());
/// ```
pub fn sim_device_of(argv: &[String]) -> Option<SimDevice> {
    let first = argv.first()?;
    if first.rsplit('/').next() != Some("launchd_sim") {
        return None;
    }
    let path = argv.get(1)?;
    if !path.starts_with('/') || path.split('/').any(|c| c == "..") {
        return None;
    }
    let (head, rest) = path.split_once("/CoreSimulator/Devices/")?;
    let udid = rest.split('/').next()?;
    let valid = udid.len() == 36
        && udid.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        });
    valid.then(|| SimDevice {
        udid: udid.to_ascii_uppercase(),
        plist: format!("{head}/CoreSimulator/Devices/{udid}/device.plist"),
    })
}

/// `com.apple.CoreSimulator.SimRuntime.iOS-27-0` -> `iOS 27.0`; anything else is kept as is.
pub fn pretty_runtime(rt: &str) -> String {
    let id = rt
        .strip_prefix("com.apple.CoreSimulator.SimRuntime.")
        .unwrap_or(rt);
    let mut parts = id.split('-');
    match (parts.next(), parts.next()) {
        (Some(os), Some(first)) if first.bytes().all(|b| b.is_ascii_digit()) => {
            let ver: Vec<&str> = std::iter::once(first).chain(parts).collect();
            format!("{os} {}", ver.join("."))
        }
        _ => id.to_string(),
    }
}

/// (name, runtime) from a CoreSimulator `device.plist`. Reads one regular file of at most
/// 256 KiB, opened non-blocking; any failure is `None`.
pub fn sim_device_names(plist_path: &Path) -> Option<(String, Option<String>)> {
    let raw = read_head_bytes(plist_path, 256 * 1024)?;
    let plist::Value::Dictionary(d) = plist::Value::from_reader(std::io::Cursor::new(raw)).ok()?
    else {
        return None;
    };
    let name = d.get("name")?.as_string()?.to_string();
    let rt = d
        .get("runtime")
        .and_then(|v| v.as_string())
        .map(pretty_runtime);
    Some((name, rt))
}

/// Parses `lsof -Fpn` output into (pid, name) pairs.
pub fn parse_lsof_pn(out: &str) -> Vec<(i32, String)> {
    let mut cur = 0;
    let mut v = Vec::new();
    for line in out.lines() {
        if let Some(r) = line.strip_prefix('p') {
            cur = r.parse().unwrap_or(0);
        } else if let Some(r) = line.strip_prefix('n') {
            v.push((cur, r.to_string()));
        }
    }
    v
}

fn lsof_result(out: sys::HelperOutput) -> Option<String> {
    if (out.code == Some(0) && out.stderr.trim().is_empty())
        || (out.code == Some(1) && out.stdout.trim().is_empty() && out.stderr.trim().is_empty())
    {
        Some(out.stdout)
    } else {
        None
    }
}

fn lsof_output(cmd: &[&str], timeout_s: u64) -> Option<String> {
    lsof_result(sys::run_output(cmd, timeout_s, 8 * 1024 * 1024).ok()?)
}

fn tmux_result(out: sys::HelperOutput) -> (Option<String>, Option<String>) {
    if out.code == Some(0) {
        return (Some(out.stdout), None);
    }
    let error = out.stderr.trim();
    if out.code == Some(1)
        && out.stdout.trim().is_empty()
        && !error.contains('\n')
        && (error.starts_with("no server running on ")
            || (error.starts_with("error connecting to ")
                && error.ends_with("(No such file or directory)")))
    {
        (
            Some(String::new()),
            Some("tmux sessions: no server running".into()),
        )
    } else {
        (None, None)
    }
}

/// (pid, port) for every listening TCP socket. None if lsof failed.
pub fn listening_ports() -> Option<Vec<(i32, String)>> {
    let out = lsof_output(
        &["/usr/sbin/lsof", "-nP", "-iTCP", "-sTCP:LISTEN", "-Fpn"],
        2,
    )?;
    Some(
        parse_lsof_pn(&out)
            .into_iter()
            .map(|(p, n)| (p, n.rsplit(':').next().unwrap_or("").to_string()))
            .collect(),
    )
}

/// Established TCP sockets per pid (one lsof call).
pub fn established_counts() -> Option<HashMap<i32, usize>> {
    let mut m = HashMap::new();
    let out = lsof_output(
        &[
            "/usr/sbin/lsof",
            "-nP",
            "-iTCP",
            "-sTCP:ESTABLISHED",
            "-Fpn",
        ],
        2,
    )?;
    for (p, _) in parse_lsof_pn(&out) {
        *m.entry(p).or_insert(0) += 1;
    }
    Some(m)
}

/// `tmux list-panes -a`. An absent optional tool is informational; no server is an empty set.
pub fn tmux_panes() -> (Option<Vec<Pane>>, Option<String>) {
    let result = sys::run_output(
        &[
            "tmux",
            "list-panes",
            "-a",
            "-F",
            "#{session_name}\t#{window_index}.#{pane_index}\t#{pane_pid}\t#{pane_tty}",
        ],
        2,
        8 * 1024 * 1024,
    );
    let (out, note) = match result {
        Ok(out) => tmux_result(out),
        Err(std::io::ErrorKind::NotFound) => (
            None,
            Some("tmux sessions: optional tool not installed".into()),
        ),
        Err(_) => (None, None),
    };
    let Some(out) = out else {
        return (None, note);
    };
    let mut v = Vec::new();
    for line in out.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() == 4 && !f[2].is_empty() && f[2].bytes().all(|c| c.is_ascii_digit()) {
            v.push(Pane {
                session: f[0].to_string(),
                pane: f[1].to_string(),
                pid: f[2].parse().unwrap_or(0),
                tty: f[3].replace("/dev/", ""),
            });
        }
    }
    (Some(v), note)
}

/// pid -> watched repo for git fsmonitor daemons (their open directory fd 4).
pub fn fsmonitor_repos() -> Option<HashMap<i32, String>> {
    let out = lsof_output(
        &[
            "/usr/sbin/lsof",
            "-nP",
            "-a",
            "-c",
            "git",
            "-d",
            "4",
            "-Fpn",
        ],
        2,
    )?;
    Some(parse_lsof_pn(&out).into_iter().collect())
}

// ------------------------------------------------------------------ Codex rollouts

#[derive(Debug, Clone)]
pub struct Rollout {
    pub start: f64,
    pub thread: String,
    pub path: PathBuf,
}

/// Parses `rollout-YYYY-MM-DDTHH-MM-SS-<36 char id>.jsonl` into (local start epoch, thread id).
pub fn parse_rollout_name(n: &str) -> Option<(f64, String)> {
    let rest = n.strip_prefix("rollout-")?.strip_suffix(".jsonl")?;
    if rest.len() != 19 + 1 + 36 || rest.as_bytes()[19] != b'-' {
        return None;
    }
    let (ts, id) = (&rest[..19], &rest[20..]);
    let b = ts.as_bytes();
    let shape = b"dddd-dd-ddTdd-dd-dd";
    for (c, s) in b.iter().zip(shape.iter()) {
        let ok = if *s == b'd' {
            c.is_ascii_digit()
        } else {
            c == s
        };
        if !ok {
            return None;
        }
    }
    if !id
        .bytes()
        .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c) || c == b'-')
    {
        return None;
    }
    let n = |r: std::ops::Range<usize>| ts[r].parse::<i32>().unwrap_or(0);
    let t = sys::mktime_local(n(0..4), n(5..7), n(8..10), n(11..13), n(14..16), n(17..19));
    Some((t, id.to_string()))
}

/// Rollout FILE NAMES from the last `days` local days. No file is opened here.
pub fn codex_rollouts(cfg: &Config, days: u32) -> Vec<Rollout> {
    let mut out = Vec::new();
    let now = sys::now();
    for d in 0..days {
        let sub = sys::strftime("%Y/%m/%d", now - d as f64 * 86400.0);
        let dir = Path::new(&cfg.home).join(".codex/sessions").join(sub);
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if let Some((start, thread)) = parse_rollout_name(&name) {
                out.push(Rollout {
                    start,
                    thread,
                    path: dir.join(&name),
                });
            }
        }
    }
    out
}

/// Metadata read from the first 3 KB of a rollout. No messages are read.
#[derive(Debug, Clone, Default)]
pub struct RolloutMeta {
    pub cwd: Option<String>,
    pub originator: Option<String>,
}

pub fn rollout_meta(path: &Path) -> RolloutMeta {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r#""(cwd|originator)":"([^"]{0,200})""#).unwrap());
    let head = read_head(path, 3000);
    let mut m = RolloutMeta::default();
    for c in re.captures_iter(&head) {
        let v = c[2].to_string();
        if &c[1] == "cwd" {
            m.cwd = Some(v);
        } else {
            m.originator = Some(v);
        }
    }
    m
}

// ------------------------------------------------------------------ git project

/// Largest `.git` pointer file that is read; a real one is a single short line.
const GIT_FILE_MAX: u64 = 4096;

/// First `max` bytes of a regular file, or `None` for anything else. Opened with `O_NONBLOCK`
/// so a FIFO or device planted at the path cannot block the read.
pub(crate) fn read_head_bytes(path: &Path, max: u64) -> Option<Vec<u8>> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    if !f.metadata().is_ok_and(|m| m.is_file()) {
        return None;
    }
    let mut buf = Vec::new();
    f.take(max).read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// [`read_head_bytes`] as lossy text, or "" for anything unreadable.
fn read_head(path: &Path, max: u64) -> String {
    read_head_bytes(path, max)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

/// (project name, root) from the nearest `.git`; follows worktree pointers. None if not in a repo.
/// Uncached; [`crate::Snapshot::project_of`] caches per snapshot.
pub fn project_of(cwd: &str) -> Option<Project> {
    let home = std::env::var("HOME").unwrap_or_default();
    if cwd.is_empty() || cwd == "/" || cwd == home {
        return None;
    }
    static WT: OnceLock<Regex> = OnceLock::new();
    let wt = WT.get_or_init(|| Regex::new(r"^gitdir:\s*(.+?)/\.git/worktrees/").unwrap());
    let mut res = None;
    let p = Path::new(cwd);
    for cand in p.ancestors() {
        let cs = cand.to_string_lossy();
        if cs == "/" || cs == home || cs.is_empty() {
            break;
        }
        let g = cand.join(".git");
        let name_of = |p: &Path| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        match std::fs::metadata(&g) {
            Ok(m) if m.is_dir() => {
                res = Some(Project {
                    name: name_of(cand),
                    root: cs.into_owned(),
                });
                break;
            }
            Ok(m) if m.is_file() => {
                let txt = read_head(&g, GIT_FILE_MAX);
                let name = match wt.captures(&txt) {
                    Some(c) => name_of(Path::new(&c[1])),
                    None => name_of(cand),
                };
                res = Some(Project {
                    name: format!("{name} (worktree)"),
                    root: cs.into_owned(),
                });
                break;
            }
            _ => {}
        }
    }
    res
}

// ------------------------------------------------------------------ registry

/// One owner declaration. All of its regex fields must match.
#[derive(Debug)]
pub struct Rule {
    pub name: String,
    pub kind: String,
    pub owner: Option<String>,
    pub impact: Option<String>,
    pub notes: Option<String>,
    /// (field, regex) in argv, exe, cwd, label order.
    pub res: Vec<(&'static str, Regex)>,
}

/// Loads `registry.json`. A missing file means no rules; a broken one is ignored with a message.
pub fn load_registry(cfg: &Config) -> Vec<Rule> {
    let Ok(txt) = std::fs::read_to_string(&cfg.registry) else {
        return Vec::new();
    };
    match parse_registry(&txt) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("procwho: registry ignored ({e})");
            Vec::new()
        }
    }
}

pub fn parse_registry(txt: &str) -> Result<Vec<Rule>, String> {
    let v: serde_json::Value = serde_json::from_str(txt).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for r in v
        .get("rules")
        .and_then(|r| r.as_array())
        .cloned()
        .unwrap_or_default()
    {
        let s = |k: &str| {
            r.get(k)
                .and_then(|x| x.as_str())
                .filter(|x| !x.is_empty())
                .map(str::to_string)
        };
        let name = s("name").ok_or("rule without a name")?;
        let mut res = Vec::new();
        for k in ["argv", "exe", "cwd", "label"] {
            if let Some(pat) = s(k) {
                res.push((k, Regex::new(&pat).map_err(|e| e.to_string())?));
            }
        }
        out.push(Rule {
            name,
            kind: s("kind").unwrap_or_else(|| "declared".into()),
            owner: s("owner"),
            impact: s("impact"),
            notes: s("notes"),
            res,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn no_match_helper_results_are_empty_but_errors_stay_unavailable() {
        let out = |code, stdout: &str, stderr: &str| sys::HelperOutput {
            code: Some(code),
            stdout: stdout.into(),
            stderr: stderr.into(),
        };
        assert_eq!(lsof_result(out(1, "", "")), Some(String::new()));
        assert_eq!(lsof_result(out(1, " \n", " \n")), Some(" \n".into()));
        assert!(lsof_result(out(1, "", "lsof: permission denied")).is_none());
        assert!(lsof_result(out(0, "p42\nn*:8080", "permission denied")).is_none());
        assert!(lsof_result(out(1, "p42", "")).is_none());
        assert!(lsof_result(out(2, "", "")).is_none());
        for message in [
            "no server running on /tmp/example/default",
            "error connecting to /tmp/example/default (No such file or directory)",
        ] {
            let (rows, note) = tmux_result(out(1, "", message));
            assert_eq!(rows, Some(String::new()));
            assert_eq!(note.as_deref(), Some("tmux sessions: no server running"));
        }
        assert!(tmux_result(out(
            1,
            "",
            "error connecting to /tmp/example/default (Permission denied)"
        ))
        .0
        .is_none());
        assert!(tmux_result(out(
            1,
            "",
            "no server running on /tmp/example\npermission denied"
        ))
        .0
        .is_none());
        assert!(sys::run_output(&["/nonexistent/procwho-helper"], 1, 100).is_err());
        assert!(sys::run_output(&["/bin/sleep", "2"], 0, 100).is_err());
    }

    use super::*;

    #[test]
    fn rollout_names() {
        let (t, id) = parse_rollout_name(
            "rollout-2026-11-13T16-58-02-12345678-1234-1234-1234-123456789abc.jsonl",
        )
        .unwrap();
        assert_eq!(id, "12345678-1234-1234-1234-123456789abc");
        assert!(t > 1.7e9);
        assert!(parse_rollout_name("rollout-2026-11-13T16-58-02-short.jsonl").is_none());
        assert!(parse_rollout_name("notes.jsonl").is_none());
    }

    #[test]
    fn registry_rules() {
        let r =
            parse_registry(r#"{"rules":[{"name":"X","argv":"foo\\.js","owner":"me"}]}"#).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].kind, "declared");
        assert!(parse_registry(r#"{"rules":[{"name":"X","argv":"("}]}"#).is_err());
        assert!(parse_registry("not json").is_err());
        assert!(parse_registry("{}").unwrap().is_empty());
    }

    #[test]
    fn head_reads_are_bounded_and_skip_non_files() {
        let dir = std::env::temp_dir().join(format!("procwho-head-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let big = dir.join("big");
        std::fs::write(&big, "x".repeat(100_000)).unwrap();
        assert_eq!(read_head(&big, GIT_FILE_MAX).len(), 4096);
        let fifo = dir.join("fifo");
        let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        // SAFETY: c is a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let t = std::time::Instant::now();
        assert_eq!(read_head(&fifo, 3000), "");
        assert_eq!(rollout_meta(&fifo).cwd, None);
        assert!(t.elapsed().as_secs() < 2, "a FIFO must not block");
        assert_eq!(read_head(&dir, 10), "", "a directory is not read");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn plist_lookup_rejects_path_like_labels() {
        let dir = std::env::temp_dir().join(format!("procwho-plist-{}", std::process::id()));
        let agents = dir.join("Library/LaunchAgents");
        std::fs::create_dir_all(&agents).unwrap();
        let pl = r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict><key>KeepAlive</key><true/></dict></plist>"#;
        std::fs::write(dir.join("Library/evil.plist"), pl).unwrap();
        std::fs::write(agents.join(".hidden.plist"), pl).unwrap();
        std::fs::write(agents.join("com.ok.job.plist"), pl).unwrap();
        let cfg = Config {
            home: dir.to_string_lossy().into_owned(),
            state: dir.join("s"),
            registry: dir.join("r"),
        };
        assert!(plist_meta(&cfg, "../evil").plist.is_none());
        assert!(plist_meta(&cfg, ".hidden").plist.is_none());
        assert!(plist_meta(&cfg, "com.ok.job").keepalive);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn pid_reuse_is_detected_by_unique_id() {
        assert!(same_instance(7, Some((7, 1))));
        assert!(
            !same_instance(7, Some((8, 1))),
            "PID reused by another instance"
        );
        assert!(!same_instance(7, None), "exited during the reads");
        assert!(same_instance(0, None));
    }

    #[test]
    fn argv0_is_redacted_for_display() {
        let a = display_argv0("/tmp/sk-abcdefghijklmnopqrstu/tool\x1b[2J");
        assert!(!a.contains("sk-abc") && !a.contains('\x1b'), "{a}");
        assert_eq!(display_argv0("/usr/local/bin/node"), "/usr/local/bin/node");
    }

    #[test]
    fn lsof_parsing() {
        let v = parse_lsof_pn("p12\nn*:8080\nn[::1]:9000\np7\nn127.0.0.1:3000\n");
        assert_eq!(
            v,
            vec![
                (12, "*:8080".into()),
                (12, "[::1]:9000".into()),
                (7, "127.0.0.1:3000".into())
            ]
        );
    }
}
