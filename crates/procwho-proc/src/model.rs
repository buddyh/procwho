//! Plain data types shared by collection, attribution, history and rendering.

use serde::Serialize;
use std::path::PathBuf;

pub const MIB: u64 = 1024 * 1024;
pub const RETENTION_H: f64 = 72.0;

/// Where state and configuration live.
#[derive(Debug, Clone)]
pub struct Config {
    pub home: String,
    pub state: PathBuf,
    pub registry: PathBuf,
}

impl Config {
    /// `PROCWHO_STATE` / `PROCWHO_REGISTRY` override the defaults
    /// `~/.local/state/procwho` and `~/.config/procwho/registry.json`.
    pub fn from_env() -> Config {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let state = std::env::var_os("PROCWHO_STATE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&home).join(".local/state/procwho"));
        let registry = std::env::var_os("PROCWHO_REGISTRY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&home).join(".config/procwho/registry.json"));
        Config {
            home,
            state,
            registry,
        }
    }
}

/// The only environment keys that are ever copied out of a process.
#[derive(Debug, Clone, Default)]
pub struct Env {
    pub t3_thread: Option<String>,
    pub entrypoint: Option<String>,
    pub tmux_pane: Option<String>,
    pub owner: Option<String>,
}

/// One process. `argv` and `raw` stay in memory for matching only and are never serialized.
#[derive(Debug, Clone, Serialize)]
pub struct Proc {
    pub pid: i32,
    pub ppid: i32,
    pub uid: u32,
    pub pgid: i32,
    /// Kernel unique id: never reused during a boot.
    pub uniq: u64,
    /// Parent unique id as the kernel reports it. After reparenting to launchd this is
    /// sometimes still the original parent's id and sometimes launchd's (1); 1 says nothing
    /// about the original parent.
    pub puniq: u64,
    pub comm: String,
    pub exe: String,
    pub exe_gone: bool,
    pub readable: bool,
    #[serde(skip)]
    pub argv: Vec<String>,
    #[serde(skip)]
    pub raw: String,
    #[serde(skip)]
    pub env: Env,
    /// Redacted command line, safe to show or store.
    pub cmd: String,
    pub cwd: String,
    pub tty: String,
    pub fp: u64,
    /// Whether the kernel supplied memory and CPU counters.
    #[serde(skip)]
    pub usage_available: bool,
    pub rss: u64,
    pub fp_max: u64,
    pub cpu_ns: u64,
    pub start: f64,
    pub zombie: bool,
    pub ports: Vec<String>,
    pub label: String,
    pub resp: i32,
    /// Index into `Snapshot::workloads`.
    pub wl: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct Pane {
    pub session: String,
    pub pane: String,
    pub pid: i32,
    pub tty: String,
}

/// (project name, root path) of the nearest git repo.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    pub root: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct PlistMeta {
    pub plist: Option<String>,
    pub error: Option<String>,
    pub workdir: Option<String>,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub keepalive: bool,
    pub schedule: Option<String>,
}

/// A named workload: the unit that owns one or more processes.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Workload {
    pub agent_link: Option<crate::agent_link::AgentLink>,
    pub key: String,
    pub kind: String,
    pub name: String,
    pub confidence: String,
    pub evidence: Vec<String>,
    pub pids: Vec<i32>,
    pub roots: Vec<i32>,
    /// Index of the enclosing workload in `Snapshot::workloads`.
    pub parent: Option<usize>,
    // Optional facts, set when the workload is created.
    pub owner: Option<String>,
    pub impact: Option<String>,
    pub notes: Option<String>,
    pub label: Option<String>,
    pub meta: Option<PlistMeta>,
    pub svc: Option<crate::providers::ServiceLabel>,
    pub host: Option<String>,
    pub agent: Option<String>,
    pub session_id: Option<String>,
    pub t3_thread: Option<String>,
    pub tmux: Option<Pane>,
    /// `None` for kinds that carry no identity; `Some(vec![])` for sessions with none resolved.
    pub ident: Option<Vec<String>>,
    pub project: Option<Project>,
    pub bundle: Option<String>,
    pub via: Option<String>,
    // Totals, filled after attribution.
    pub procs: usize,
    pub fp: u64,
    pub rss: u64,
    pub cpu_ns: u64,
    pub ports: Vec<String>,
    pub zombies: usize,
    pub oldest: f64,
}

impl Workload {
    /// Filters terminal control characters out of every display-only text field.
    /// `key` is an identity, not display text, and is left as it is.
    pub fn sanitize_display(&mut self) {
        use crate::redact::display;
        let fix = |s: &mut String| *s = display(s);
        let fix_opt = |s: &mut Option<String>| {
            if let Some(v) = s {
                *v = display(v);
            }
        };
        fix(&mut self.name);
        self.evidence.iter_mut().for_each(fix);
        let mut seen = std::collections::HashSet::new();
        self.evidence.retain(|e| seen.insert(e.clone()));
        self.ident.iter_mut().flatten().for_each(fix);
        for o in [
            &mut self.owner,
            &mut self.impact,
            &mut self.notes,
            &mut self.host,
            &mut self.via,
        ] {
            fix_opt(o);
        }
    }
}

/// Which helper sources answered in this run.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SourceStatus {
    pub launchctl: bool,
    pub lsof_ports: bool,
    pub tmux: bool,
}
