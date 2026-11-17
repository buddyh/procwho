//! Reviewable findings: possible leftovers and runaways, each with reasons and what is NOT proven.

use crate::attribute::{is_claude, is_codex, signature};
use crate::fmt::fmt_age;
use crate::history::{History, TrendRow};
use crate::model::MIB;
use crate::snapshot::Snapshot;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Finding {
    pub severity: String,
    pub title: String,
    /// Workload key, if the finding belongs to one.
    pub workload: Option<String>,
    pub reasons: Vec<String>,
    pub not_proven: Vec<String>,
    pub basis: String,
    pub footprint_note: &'static str,
}

/// Insertion-ordered counter (Python `Counter` iteration order).
struct Counter<K> {
    order: Vec<K>,
    n: HashMap<K, usize>,
}

impl<K: std::hash::Hash + Eq + Clone> Counter<K> {
    fn new() -> Self {
        Counter {
            order: Vec::new(),
            n: HashMap::new(),
        }
    }
    fn add(&mut self, k: K) {
        let e = self.n.entry(k.clone()).or_insert_with(|| {
            self.order.push(k);
            0
        });
        *e += 1;
    }
    fn items(&self) -> impl Iterator<Item = (&K, usize)> {
        self.order.iter().map(|k| (k, self.n[k]))
    }
}

fn mib(b: u64) -> f64 {
    b as f64 / MIB as f64
}

/// All findings for a snapshot, `review` first, then by title.
pub fn findings(snap: &Snapshot) -> Vec<Finding> {
    let mut out: Vec<Finding> = Vec::new();
    let now = snap.ts;
    let mut add = |sev: &str,
                   title: String,
                   wl: Option<&str>,
                   reasons: Vec<String>,
                   unproven: Vec<&str>,
                   basis: String| {
        out.push(Finding {
            footprint_note: crate::render::FOOTPRINT_NOTE,
            severity: sev.into(),
            title: crate::redact::display(&title),
            workload: wl.map(crate::redact::display),
            reasons: reasons.iter().map(|s| crate::redact::display(s)).collect(),
            not_proven: unproven.into_iter().map(crate::redact::display).collect(),
            basis: crate::redact::display(&basis),
        });
    };

    let est = snap.established_counts();
    let hist = History::open_existing(&snap.cfg.state);
    let trends = hist
        .as_ref()
        .map(|h| h.trends_all(24.0))
        .unwrap_or_default();
    let no_trend: Vec<TrendRow> = Vec::new();

    // Agent helpers duplicated across many sessions.
    let mut helper_order: Vec<String> = Vec::new();
    let mut helpers: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, p) in snap.procs.iter().enumerate() {
        if p.readable
            && matches!(
                snap.workloads[p.wl].kind.as_str(),
                "agent-toolset" | "agent-session"
            )
        {
            let sig = signature(p);
            if !helpers.contains_key(&sig) {
                helper_order.push(sig.clone());
            }
            helpers.entry(sig).or_default().push(i);
        }
    }
    for sig in &helper_order {
        let ps = &helpers[sig];
        let mut owners: Vec<usize> = ps.iter().map(|&i| snap.procs[i].wl).collect();
        owners.sort_unstable();
        owners.dedup();
        if owners.len() >= 8
            && !ps
                .iter()
                .any(|&i| is_claude(&snap.procs[i]) || is_codex(&snap.procs[i]))
        {
            let fp: u64 = ps.iter().map(|&i| snap.procs[i].fp).sum();
            add(
                "info",
                format!("There are {} copies of agent helper '{sig}' across {} sessions and tool sets.", ps.len(), owners.len()),
                None,
                vec![
                    format!("Their total footprint is {:.0} MiB, about {:.0} MiB per copy.", mib(fp), mib(fp) / ps.len() as f64),
                    "Each agent thread starts a copy of every configured MCP server.".into(),
                ],
                vec!["Unused copies are not identified; removing the server from agent settings reduces the count."],
                "The count comes from one observation.".into(),
            );
        }
    }

    // Booted iOS simulators: many of them at once is a common cause of load.
    let sims: Vec<&crate::model::Workload> = snap
        .workloads
        .iter()
        .filter(|w| w.kind == "simulator")
        .collect();
    if sims.len() >= 2 {
        let procs: usize = sims.iter().map(|w| w.procs).sum();
        let fp: u64 = sims.iter().map(|w| w.fp).sum();
        let names: Vec<String> = sims
            .iter()
            .map(|w| w.name.trim_start_matches("iOS Simulator: ").to_string())
            .collect();
        add(
            "info",
            format!("There are {} booted iOS simulators.", sims.len()),
            None,
            vec![
                format!(
                    "Their {procs} processes use {:.0} MiB of footprint.",
                    mib(fp)
                ),
                format!("The devices are: {}.", names.join("; ")),
            ],
            vec!["Usage is unknown because tests or agent sessions may be using these devices."],
            "This finding comes from one observation.".into(),
        );
    }

    for w in &snap.workloads {
        let ps: Vec<&crate::model::Proc> = w
            .pids
            .iter()
            .map(|pid| &snap.procs[snap.index[pid]])
            .collect();
        let tr = trends.get(&w.key).unwrap_or(&no_trend);
        let with_cpu: Vec<&TrendRow> = tr
            .iter()
            .filter(|r| r.cpu_delta_ns.is_some() && r.interval_s.is_some_and(|i| i != 0.0))
            .collect();
        let span = if tr.len() > 1 {
            (tr[tr.len() - 1].ts - tr[0].ts) / 60.0
        } else {
            0.0
        };
        let key = Some(w.key.as_str());

        if matches!(w.kind.as_str(), "detached" | "unknown") && w.fp > 50 * MIB {
            let age_h = if w.oldest != 0.0 {
                (now - w.oldest) / 3600.0
            } else {
                0.0
            };
            let mut reasons = vec![
                format!(
                    "Its {} processes use {:.0} MiB of footprint; the oldest started {:.1} hours ago.",
                    w.procs,
                    mib(w.fp),
                    age_h
                ),
                "No launchd job, terminal or live parent identifies their owner."
                    .into(),
            ];
            if !w.ports.is_empty() {
                reasons.push(match est {
                    Some(est) => {
                        let n_est: usize = w.pids.iter().map(|p| est.get(p).copied().unwrap_or(0)).sum();
                        format!("It listens on {} local ports with {} established TCP connections{}.", w.ports.len(), n_est,
                            if n_est == 0 { " (none are attached at this moment)" } else { "" })
                    }
                    None => format!("It listens on {} local ports; established TCP connections are unknown because lsof could not read them.", w.ports.len()),
                });
            }
            let (basis, sev);
            if with_cpu.len() >= 3 {
                let cpu_s: f64 = with_cpu
                    .iter()
                    .map(|r| r.cpu_delta_ns.unwrap_or(0) as f64)
                    .sum::<f64>()
                    / 1e9;
                reasons.push(format!(
                    "It used {:.2} CPU-seconds over {} observations spanning {:.0} minutes.",
                    cpu_s,
                    with_cpu.len(),
                    span
                ));
                basis = format!(
                    "The trend covers {} intervals over {:.0} minutes.",
                    with_cpu.len(),
                    span
                );
                sev = if cpu_s < 1.0 { "review" } else { "info" };
            } else {
                basis = format!(
                    "Only one observation was assessed; {} stored samples do not establish activity over time.",
                    tr.len()
                );
                sev = "info";
            }
            add(
                sev,
                format!("The detached workload {} may have finished.", w.name),
                key,
                reasons,
                vec![
                    "Completion is unknown because detached work can remain active or have a waiting client.",
                    "The effect of stopping it on other work is unknown.",
                ],
                basis,
            );
        }

        let mut sigs: Counter<String> = Counter::new();
        for p in &ps {
            sigs.add(signature(p));
        }
        for (s, n) in sigs.items() {
            if n >= 8
                && !matches!(
                    w.kind.as_str(),
                    "system" | "system-opaque" | "app" | "agent-toolset" | "agent-session"
                )
                && !s.contains("Helper")
            {
                let fp: u64 = ps.iter().filter(|p| signature(p) == *s).map(|p| p.fp).sum();
                add(
                    "info",
                    format!("There are {n} copies of '{s}' in {}.", w.name),
                    key,
                    vec![format!(
                        "The {n} processes share a command pattern and use {:.0} MiB of footprint.",
                        mib(fp)
                    )],
                    vec!["Unused copies are not identified because each may serve a separate thread or repository."],
                    "The count comes from one observation.".into(),
                );
            }
        }

        if with_cpu.len() >= 3 && span >= 2.0 {
            let last3 = &with_cpu[with_cpu.len() - 3..];
            if last3.iter().all(|r| {
                r.cpu_delta_ns.unwrap_or(0) as f64 / (r.interval_s.unwrap_or(1.0) * 1e9) > 0.8
            }) {
                let num: f64 = last3
                    .iter()
                    .map(|r| r.cpu_delta_ns.unwrap_or(0) as f64)
                    .sum();
                let den: f64 = last3.iter().map(|r| r.interval_s.unwrap_or(0.0)).sum();
                add(
                    "review",
                    format!("The workload {} has sustained CPU use.", w.name),
                    key,
                    vec![format!("It averaged {:.0}% of one core across the last three intervals ({:.0} minutes of history).", num / den / 1e7, span)],
                    vec!["The cause is unknown; builds, tests and renders can also sustain CPU use."],
                    format!("The trend covers {} intervals.", with_cpu.len()),
                );
            }
        }

        let n = tr.len();
        if n >= 4
            && (n - 4..n - 1).all(|i| tr[i].fp < tr[i + 1].fp)
            && tr[n - 1].fp - tr[n - 4].fp > (200 * MIB) as i64
            && tr[n - 1].fp as f64 > tr[n - 4].fp as f64 * 1.2
        {
            add(
                "review",
                format!("Memory use is growing for {}.", w.name),
                key,
                vec![format!(
                    "Footprint rose in each of the last three intervals, from {:.0} to {:.0} MiB.",
                    tr[n - 4].fp as f64 / MIB as f64,
                    tr[n - 1].fp as f64 / MIB as f64
                )],
                vec!["A leak is not established because caches and loading data can also grow."],
                format!("The trend covers {} samples over {:.0} minutes.", n, span),
            );
        }

        let recent: Vec<&TrendRow> = tr[n.saturating_sub(3)..]
            .iter()
            .filter(|r| r.interval_s.is_some_and(|i| i != 0.0))
            .collect();
        if recent.len() == 3
            && !matches!(w.kind.as_str(), "system" | "system-opaque" | "app")
            && recent
                .iter()
                .all(|r| r.births as f64 / r.interval_s.unwrap_or(1.0) * 60.0 > 30.0)
        {
            let l = recent[2];
            add(
                "review",
                format!("The workload {} starts processes frequently.", w.name),
                key,
                vec![format!(
                    "It started over 30 processes per minute in the last three intervals (latest: {} in {:.0}s).",
                    l.births,
                    l.interval_s.unwrap_or(0.0)
                )],
                vec!["A process creation fault is not established because compilers and tests also start processes quickly."],
                "The trend covers three intervals.".into(),
            );
        }

        let mut gone: Vec<&str> = ps
            .iter()
            .map(|p| p.cwd.as_str())
            .filter(|c| {
                !c.is_empty()
                    && *c != "/"
                    && snap.is_local_path(c)
                    && !std::path::Path::new(c).is_dir()
            })
            .collect();
        gone.sort_unstable();
        gone.dedup();
        if let Some(first) = gone.first() {
            add(
                "review",
                format!("A working folder is missing for {}.", w.name),
                key,
                vec![format!(
                    "There are {} missing working folders, including {first}.",
                    gone.len()
                )],
                vec!["A missing working folder does not establish that the process is unused."],
                "This finding comes from one observation.".into(),
            );
        }
    }

    // Binaries replaced on disk while still running.
    let gone: Vec<&crate::model::Proc> = snap.procs.iter().filter(|p| p.exe_gone).collect();
    let mut gc: Counter<String> = Counter::new();
    for p in &gone {
        gc.add(signature(p));
    }
    for (sig, n) in gc.items() {
        let ex = gone
            .iter()
            .find(|p| signature(p) == *sig)
            .copied()
            .expect("counted above");
        let exe_tail: String = crate::fmt::tail_chars(&ex.exe, 70);
        add(
            "info",
            format!("There are {n} processes running a replaced executable: {sig}."),
            Some(snap.workloads[ex.wl].key.as_str()),
            vec![
                format!(
                    "PID {} started {} ago; macOS cannot find its original executable at {}.",
                    ex.pid,
                    fmt_age(ex.start, now),
                    exe_tail
                ),
                "An upgrade through npm or Homebrew can replace the file while the old code keeps running.".into(),
            ],
            vec!["A fault is not established because the old code can keep working until restart."],
            "This finding comes from one observation.".into(),
        );
    }

    // Zombies.
    let mut zp: Counter<i32> = Counter::new();
    for p in snap.procs.iter().filter(|p| p.zombie) {
        zp.add(p.ppid);
    }
    for (ppid, n) in zp.items() {
        if let (true, Some(par)) = (n >= 3, snap.proc_by_pid(*ppid)) {
            add(
                "info",
                format!("PID {ppid} has not collected the exit results of {n} finished children."),
                Some(snap.workloads[par.wl].key.as_str()),
                vec![
                    format!("The parent command is {}.", &par.cmd),
                    "These finished processes hold process-list slots but use no memory or CPU."
                        .into(),
                ],
                vec!["No memory or CPU cost is established."],
                "This finding comes from one observation.".into(),
            );
        }
    }
    let nz = snap.procs.iter().filter(|p| p.zombie).count();
    if nz > 0 {
        add(
            "info",
            format!("There are {nz} finished processes waiting for their parents to collect exit results."),
            None,
            vec![
                "They leave the process list when their parent collects their exit results or exits.".into(),
                "They have already finished, so they cannot be stopped.".into(),
            ],
            vec![],
            "This finding comes from one observation.".into(),
        );
    }
    out.sort_by(|a, b| {
        ((a.severity != "review"), &a.title).cmp(&((b.severity != "review"), &b.title))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_socket_source_is_unknown_not_zero_connections() {
        let mut snapshot = Snapshot::fixture(vec![]);
        snapshot.set_established_fixture(None);
        snapshot.workloads.push(crate::model::Workload {
            key: "detached:fixture".into(),
            kind: "detached".into(),
            name: "Fixture".into(),
            fp: 60 * MIB,
            ports: vec!["8080".into()],
            ..Default::default()
        });
        let found = findings(&snapshot);
        let reason = found
            .iter()
            .flat_map(|f| &f.reasons)
            .find(|s| s.contains("established TCP"))
            .unwrap();
        assert!(reason.contains("unknown"));
        assert!(!reason.contains("nothing is attached"));
        let sources = crate::render::sources_view(&snapshot);
        assert!(sources
            .notes
            .iter()
            .any(|s| s.contains("Established connections could not be read")));
        assert!(sources.notes.iter().any(|s| s.contains("tmux")));
    }
}
