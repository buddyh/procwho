//! One observation of the machine: processes, workloads and the sources they came from.

use crate::collect::*;
use crate::model::*;
use crate::sys::{self, Reader};
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// A process that is running somewhere, for callers that mark paths as in use.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LiveUse {
    pub pid: i32,
    pub cwd: String,
    /// Display name of the owning workload.
    pub workload: String,
    pub kind: String,
    pub agent_related: bool,
}

/// Totals including workloads nested inside one.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Inclusive {
    pub fp: u64,
    pub procs: usize,
    pub children: usize,
}

/// CPU rates from a second read: percent of one core per workload and per process.
#[derive(Debug, Clone)]
pub struct CpuRates {
    pub per_workload: Vec<f64>,
    pub per_proc: Vec<f64>,
    pub interval: f64,
}

/// Accepted range of the CPU read interval, in seconds.
pub const CPU_INTERVAL: (f64, f64) = (0.1, 3600.0);
/// Accepted range of the `watch` sampling interval, in seconds.
pub const WATCH_INTERVAL: (f64, f64) = (1.0, 86400.0);

fn check_range(what: &str, v: f64, (lo, hi): (f64, f64)) -> Result<f64, String> {
    if v.is_finite() && (lo..=hi).contains(&v) {
        Ok(v)
    } else {
        Err(format!(
            "{what} must be a number of seconds from {lo} to {hi} (got {v})"
        ))
    }
}

/// Validates a CPU read interval (`top --interval`).
///
/// ```
/// use procwho_proc::snapshot::check_cpu_interval;
/// assert!(check_cpu_interval(1.0).is_ok());
/// assert!(check_cpu_interval(f64::NAN).is_err());
/// assert!(check_cpu_interval(1e300).is_err());
/// ```
pub fn check_cpu_interval(v: f64) -> Result<f64, String> {
    check_range("--interval", v, CPU_INTERVAL)
}

/// Validates a `watch` sampling interval.
pub fn check_watch_interval(v: f64) -> Result<f64, String> {
    check_range("--interval", v, WATCH_INTERVAL)
}

pub struct Snapshot {
    pub cfg: Config,
    /// Wall-clock start of the collection (epoch seconds).
    pub ts: f64,
    pub procs: Vec<Proc>,
    pub workloads: Vec<Workload>,
    /// Seconds spent in the libproc pass.
    pub t_libproc: f64,
    /// Seconds for the whole collection and attribution.
    pub t_total: f64,
    pub sources: SourceStatus,
    pub provider_note: String,
    pub source_notes: Vec<String>,
    /// pid -> repo watched by a git fsmonitor daemon.
    pub fsmon: HashMap<i32, String>,
    /// ppid -> child pids, in process-list order.
    pub kids: HashMap<i32, Vec<i32>>,
    /// pid -> index in `procs`.
    pub index: HashMap<i32, usize>,
    pub(crate) by_uniq: HashMap<u64, i32>,
    pub(crate) pane_pid: HashMap<i32, Pane>,
    pub(crate) pane_tty: HashMap<String, Pane>,
    pub(crate) service_labels: HashMap<String, crate::providers::ServiceLabel>,
    pub(crate) rules: Vec<Rule>,
    pub(crate) extra_agents: Vec<crate::agents::AgentMatcher>,
    /// Mount points and whether each is local; see [`sys::mounts`].
    pub(crate) mounts: Vec<(String, bool)>,
    /// cwd -> project, for this snapshot only (a long `watch` must not grow or go stale).
    proj_cache: Mutex<HashMap<String, Option<Project>>>,
    established: OnceLock<Option<HashMap<i32, usize>>>,
}

impl Snapshot {
    /// Collects and attributes using `PROCWHO_STATE` / `PROCWHO_REGISTRY` or the defaults.
    pub fn collect() -> Snapshot {
        Snapshot::collect_with(Config::from_env())
    }

    pub fn collect_with(cfg: Config) -> Snapshot {
        let t0 = Instant::now();
        let settings = crate::config::LoadedConfig::from_env(std::path::Path::new(&cfg.home));
        let extra_agents = crate::agents::compile_extra(&settings.values.extra_agents);
        let ts = sys::now();
        let mut procs = read_procs();
        let t_libproc = t0.elapsed().as_secs_f64();
        let need_fsmon = procs.iter().any(|p| p.raw.contains("fsmonitor--daemon"));
        // The helper commands are independent: run them side by side.
        let (labels, ports, panes, service_labels, fsmon, rules) = std::thread::scope(|sc| {
            let h_l = sc.spawn(launchd_labels);
            let h_p = sc.spawn(listening_ports);
            let h_t = sc.spawn(tmux_panes);
            let h_o =
                sc.spawn(|| crate::providers::collect(&cfg, &settings.values.label_providers));
            let h_f = sc.spawn(move || {
                if need_fsmon {
                    fsmonitor_repos()
                } else {
                    Some(HashMap::new())
                }
            });
            let rules = load_registry(&cfg);
            (
                h_l.join().unwrap_or((HashMap::new(), false)),
                h_p.join().unwrap_or(None),
                h_t.join().unwrap_or((None, None)),
                h_o.join()
                    .unwrap_or((HashMap::new(), "label provider lookup failed".into())),
                h_f.join().unwrap_or_default(),
                rules,
            )
        });
        let (panes, tmux_note) = panes;
        let (labels, launchd_ok) = labels;
        let (service_labels, provider_note) = service_labels;
        let mut source_notes = settings.notes;
        source_notes.extend(tmux_note);
        if fsmon.is_none() {
            source_notes.push(
                "git monitor directories unavailable or timed out; watched repos unknown".into(),
            );
        }
        let index: HashMap<i32, usize> =
            procs.iter().enumerate().map(|(i, p)| (p.pid, i)).collect();
        if let Some(list) = &ports {
            for (pid, port) in list {
                if let Some(&i) = index.get(pid) {
                    if !procs[i].ports.contains(port) {
                        procs[i].ports.push(port.clone());
                    }
                }
            }
        }
        let mut kids: HashMap<i32, Vec<i32>> = HashMap::new();
        for p in &mut procs {
            kids.entry(p.ppid).or_default().push(p.pid);
            if let Some(l) = labels.get(&p.pid) {
                p.label = l.clone();
            }
        }
        let by_uniq = procs
            .iter()
            .filter(|p| p.uniq != 0)
            .map(|p| (p.uniq, p.pid))
            .collect();
        let mut pane_pid = HashMap::new();
        let mut pane_tty = HashMap::new();
        for d in panes.iter().flatten() {
            pane_pid.insert(d.pid, d.clone());
            pane_tty.insert(d.tty.clone(), d.clone());
        }
        let mut s = Snapshot {
            cfg,
            ts,
            procs,
            workloads: Vec::new(),
            t_libproc,
            t_total: 0.0,
            sources: SourceStatus {
                launchctl: launchd_ok,
                lsof_ports: ports.is_some(),
                tmux: panes.is_some(),
            },
            provider_note,
            source_notes,
            fsmon: fsmon.unwrap_or_default(),
            kids,
            index,
            by_uniq,
            pane_pid,
            pane_tty,
            service_labels,
            rules,
            extra_agents,
            mounts: sys::mounts(),
            proj_cache: Mutex::new(HashMap::new()),
            established: OnceLock::new(),
        };
        let (mut wl, assign) = s.attribute();
        for (p, a) in s.procs.iter_mut().zip(assign) {
            p.wl = a;
        }
        wl.iter_mut().for_each(Workload::sanitize_display);
        crate::agent_link::associate(&s.procs, &mut wl);
        s.workloads = wl;
        s.t_total = t0.elapsed().as_secs_f64();
        s
    }

    #[cfg(test)]
    pub(crate) fn fixture(procs: Vec<Proc>) -> Self {
        let index = procs.iter().enumerate().map(|(i, p)| (p.pid, i)).collect();
        Self {
            cfg: Config {
                home: "/nonexistent/procwho-fixture".into(),
                state: "/nonexistent/procwho-fixture/state".into(),
                registry: "/nonexistent/procwho-fixture/registry".into(),
            },
            ts: 0.0,
            procs,
            workloads: vec![],
            t_libproc: 0.0,
            t_total: 0.0,
            sources: SourceStatus {
                launchctl: false,
                lsof_ports: false,
                tmux: false,
            },
            provider_note: String::new(),
            source_notes: vec![],
            fsmon: HashMap::new(),
            kids: HashMap::new(),
            index,
            by_uniq: HashMap::new(),
            pane_pid: HashMap::new(),
            pane_tty: HashMap::new(),
            service_labels: HashMap::new(),
            rules: vec![],
            extra_agents: vec![],
            mounts: vec![],
            proj_cache: Mutex::new(HashMap::new()),
            established: OnceLock::new(),
        }
    }

    /// Optional socket evidence, collected at most once and only when findings need it.
    pub(crate) fn established_counts(&self) -> Option<&HashMap<i32, usize>> {
        self.established
            .get_or_init(crate::collect::established_counts)
            .as_ref()
    }

    #[cfg(test)]
    pub(crate) fn set_established_fixture(&self, counts: Option<HashMap<i32, usize>>) {
        let _ = self.established.set(counts);
    }

    pub(crate) fn established_unavailable(&self) -> bool {
        self.established.get().is_some_and(Option::is_none)
    }

    /// One entry per readable process that has a non-empty cwd.
    pub fn live_uses(&self) -> Vec<LiveUse> {
        self.procs
            .iter()
            .filter(|p| p.readable && !p.cwd.is_empty())
            .map(|p| {
                let w = &self.workloads[p.wl];
                LiveUse {
                    pid: p.pid,
                    cwd: p.cwd.clone(),
                    workload: w.name.clone(),
                    kind: w.kind.clone(),
                    agent_related: w.agent_link.is_some() || w.kind.starts_with("agent-"),
                }
            })
            .collect()
    }

    pub fn proc_by_pid(&self, pid: i32) -> Option<&Proc> {
        self.index.get(&pid).map(|&i| &self.procs[i])
    }

    pub fn workload_of(&self, p: &Proc) -> &Workload {
        &self.workloads[p.wl]
    }

    /// Ancestor pids inside the same uid/readability boundary, nearest first.
    pub fn owner_chain(&self, pid: i32) -> Vec<i32> {
        self.index.get(&pid).map_or_else(Vec::new, |&i| {
            self.ownership_chain(i)
                .into_iter()
                .map(|a| self.procs[a].pid)
                .collect()
        })
    }

    /// Ancestor pids, nearest first, stopping below launchd.
    pub fn chain(&self, pid: i32) -> Vec<i32> {
        match self.index.get(&pid) {
            Some(&i) => self
                .chain_idx(i)
                .into_iter()
                .map(|a| self.procs[a].pid)
                .collect(),
            None => Vec::new(),
        }
    }

    /// Number of processes in the subtree rooted at `pid`, including itself.
    pub fn subtree_len(&self, pid: i32) -> usize {
        let mut out = vec![pid];
        let mut i = 0;
        while i < out.len() {
            if let Some(k) = self.kids.get(&out[i]) {
                out.extend(k.iter().copied());
            }
            i += 1;
            if out.len() > self.procs.len() + 1 {
                break; // defensive: a cycle in corrupt data
            }
        }
        out.len()
    }

    /// Totals for a workload and every workload nested inside it.
    pub fn inclusive(&self, wl: usize) -> Inclusive {
        let mut keys = vec![wl];
        let mut i = 0;
        while i < keys.len() {
            for (j, w) in self.workloads.iter().enumerate() {
                if w.parent == Some(keys[i]) && !keys.contains(&j) {
                    keys.push(j);
                }
            }
            i += 1;
        }
        Inclusive {
            fp: keys.iter().map(|&k| self.workloads[k].fp).sum(),
            procs: keys.iter().map(|&k| self.workloads[k].procs).sum(),
            children: keys.len() - 1,
        }
    }

    pub fn children_of(&self, wl: usize) -> Vec<usize> {
        (0..self.workloads.len())
            .filter(|&j| self.workloads[j].parent == Some(wl))
            .collect()
    }

    /// A PID (if live) or a workload name fragment / key. Returns (workload, process).
    /// Among name hits the one with the largest footprint wins.
    pub fn find(&self, q: &str) -> Option<(usize, Option<usize>)> {
        if !q.is_empty() && q.bytes().all(|c| c.is_ascii_digit()) {
            if let Some(&i) = q
                .parse::<i32>()
                .ok()
                .as_ref()
                .and_then(|p| self.index.get(p))
            {
                return Some((self.procs[i].wl, Some(i)));
            }
        }
        let ql = q.to_lowercase();
        let mut best: Option<usize> = None;
        for (i, w) in self.workloads.iter().enumerate() {
            let hit = w.name.to_lowercase().contains(&ql) || w.key == q;
            if hit && best.is_none_or(|b| w.fp > self.workloads[b].fp) {
                best = Some(i);
            }
        }
        best.map(|b| (b, None))
    }

    /// Short second read of the CPU counters: percent of one core.
    /// `wait` is clamped to 0..=3600 s (non-finite counts as 0); validate user input with
    /// [`check_cpu_interval`] first.
    pub fn cpu_rates(&self, wait: f64) -> CpuRates {
        let wait = if wait.is_finite() {
            wait.clamp(0.0, CPU_INTERVAL.1)
        } else {
            0.0
        };
        std::thread::sleep(Duration::from_secs_f64(wait));
        let t1 = sys::now();
        let rd = Reader::new();
        let span = ((t1 - self.ts) * 1e9).max(1.0);
        let mut per_proc = vec![0.0; self.procs.len()];
        let mut per_workload = vec![0.0; self.workloads.len()];
        for (i, p) in self.procs.iter().enumerate() {
            if !p.readable {
                continue;
            }
            let Some((uniq, ns)) = read_cpu(&rd, p.pid) else {
                continue;
            };
            if uniq != p.uniq {
                continue; // exited or PID reused: identity check by unique id
            }
            let pct = ns.saturating_sub(p.cpu_ns) as f64 / span * 100.0;
            per_proc[i] = pct;
            per_workload[p.wl] += pct;
        }
        CpuRates {
            per_workload,
            per_proc,
            interval: t1 - self.ts,
        }
    }

    /// Whether a path is on a local filesystem. Paths on network mounts are never touched
    /// by checks that stat them, so a hung mount cannot stall a sample.
    pub fn is_local_path(&self, path: &str) -> bool {
        sys::is_local(&self.mounts, path)
    }

    /// [`project_of`] cached for the life of this snapshot.
    pub fn project_of(&self, cwd: &str) -> Option<Project> {
        if let Some(hit) = self
            .proj_cache
            .lock()
            .ok()
            .and_then(|c| c.get(cwd).cloned())
        {
            return hit;
        }
        let res = project_of(cwd);
        if let Ok(mut c) = self.proj_cache.lock() {
            c.insert(cwd.to_string(), res.clone());
        }
        res
    }

    pub fn footprint_sum(&self) -> u64 {
        self.workloads.iter().map(|w| w.fp).sum()
    }

    pub fn unreadable_count(&self) -> usize {
        self.procs.iter().filter(|p| !p.readable).count()
    }
}
