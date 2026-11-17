//! Read-only "which work owns this process?" for macOS.
//!
//! Maps every process to a named workload (app, launchd service, agent session, shared agent
//! host, tool set, detached leftover, unknown) with the evidence for that label, grouped
//! totals, bounded history and reviewable findings. This crate never signals, stops, restarts
//! or modifies any process, service or file outside its own state directory.
//!
//! Units: CPU is percent of ONE core averaged over the stated interval; memory is
//! `phys_footprint` (what Activity Monitor calls "Memory", including compressed pages).
//!
//! ```no_run
//! let snap = procwho_proc::Snapshot::collect();
//! for u in snap.live_uses().iter().take(3) {
//!     println!("{} {} {}", u.pid, u.cwd, u.workload);
//! }
//! ```

#[cfg(not(target_os = "macos"))]
compile_error!("procwho-proc supports macOS only");

pub mod agent_link;
pub mod agents;
pub mod apps;
pub mod attribute;
pub mod collect;
pub mod config;
pub mod findings;
pub mod fmt;
pub mod history;
pub mod model;
pub mod naming_rule;
pub mod providers;
pub mod redact;
pub mod render;
pub mod snapshot;
pub mod status;
pub mod sys;

pub use findings::{findings, Finding};
pub use history::{History, RecordInfo, TrendRow};
pub use model::{Config, Proc, Workload, MIB, RETENTION_H};
pub use render::{
    export, findings_cmd, history, sample, top, tree, unknown, watch, why, RevealAction, SortKey,
    TopOpts, WorkloadView,
};
pub use snapshot::{CpuRates, Inclusive, LiveUse, Snapshot};
pub use status::{status, StatusOpts};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Every process belongs to exactly one workload and totals add up.
    #[test]
    fn partition_invariant_on_live_snapshot() {
        let dir = std::env::temp_dir().join(format!(
            "procwho-partition-{}-{}",
            std::process::id(),
            crate::sys::now().to_bits()
        ));
        let cfg = Config {
            // Do not inherit optional providers from the real user configuration: they
            // intentionally cache into state, which is outside this test's contract.
            home: dir.join("home").display().to_string(),
            state: dir.clone(),
            registry: dir.join("none.json"),
        };
        let snap = Snapshot::collect_with(cfg);
        assert!(
            !dir.exists(),
            "a read-only collection must not create the state dir"
        );
        assert!(snap.procs.len() > 10, "expected a live process table");
        let mut seen: HashMap<i32, usize> = HashMap::new();
        for w in &snap.workloads {
            assert!(!w.pids.is_empty(), "empty workload {}", w.key);
            assert_eq!(w.procs, w.pids.len());
            for &pid in &w.pids {
                *seen.entry(pid).or_default() += 1;
            }
        }
        for p in &snap.procs {
            assert_eq!(
                seen.get(&p.pid).copied(),
                Some(1),
                "pid {} not in exactly one workload",
                p.pid
            );
            assert!(snap.workloads[p.wl].pids.contains(&p.pid));
        }
        assert_eq!(seen.len(), snap.procs.len());
        let fp: u64 = snap.workloads.iter().map(|w| w.fp).sum();
        let pf: u64 = snap.procs.iter().map(|p| p.fp).sum();
        assert_eq!(fp, pf);
        // this test process is readable and has a cwd
        let me = std::process::id() as i32;
        assert!(snap
            .live_uses()
            .iter()
            .any(|u| u.pid == me && !u.cwd.is_empty()));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A process whose argv carries a terminal escape is shown without it.
    #[test]
    fn escape_in_argv_is_not_displayed() {
        let mut child = std::process::Command::new("/bin/cat")
            .args(["-", "\x1b[2J"])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let dir = std::env::temp_dir().join(format!("procwho-esc-{}", std::process::id()));
        let cfg = Config {
            home: std::env::var("HOME").unwrap_or_default(),
            state: dir.clone(),
            registry: dir.join("none.json"),
        };
        let snap = Snapshot::collect_with(cfg);
        let p = snap.proc_by_pid(child.id() as i32).cloned();
        let _ = child.kill(); // our own test child only
        let _ = child.wait();
        let p = p.expect("test child visible");
        assert!(!p.cmd.contains('\x1b'), "{:?}", p.cmd);
        assert!(p.cmd.ends_with("?[2J"), "{:?}", p.cmd);
        assert!(snap.workloads.iter().all(|w| !w.name.contains('\x1b')));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The project cache lives and dies with one snapshot.
    #[test]
    fn project_cache_is_per_snapshot() {
        let dir = std::env::temp_dir().join(format!("procwho-proj-{}", std::process::id()));
        let repo = dir.join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let cwd = repo.join("sub");
        let cwd = cwd.to_str().unwrap();
        let cfg = Config {
            home: std::env::var("HOME").unwrap_or_default(),
            state: dir.join("state"),
            registry: dir.join("none.json"),
        };
        let a = Snapshot::collect_with(cfg.clone());
        assert_eq!(a.project_of(cwd).map(|p| p.name).as_deref(), Some("repo"));
        std::fs::remove_dir(repo.join(".git")).unwrap();
        assert!(
            a.project_of(cwd).is_some(),
            "same snapshot answers from its cache"
        );
        let b = Snapshot::collect_with(cfg);
        assert!(
            b.project_of(cwd).is_none(),
            "a new snapshot must not see a stale entry"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Booted simulators (if any) become one workload each; host-side helpers stay out.
    /// Skips quietly when no simulator is booted. Never boots or stops one.
    #[test]
    fn booted_simulators_are_their_own_workloads() {
        let dir = std::env::temp_dir().join(format!("procwho-simlive-{}", std::process::id()));
        let cfg = Config {
            home: std::env::var("HOME").unwrap_or_default(),
            state: dir.clone(),
            registry: dir.join("none.json"),
        };
        let snap = Snapshot::collect_with(cfg);
        let roots: Vec<&crate::model::Proc> = snap
            .procs
            .iter()
            .filter(|p| p.comm == "launchd_sim")
            .collect();
        if roots.is_empty() {
            eprintln!("skipped: no simulator is booted");
            return;
        }
        if roots.len() >= 2 {
            let fs = crate::findings::findings(&snap);
            let f = fs
                .iter()
                .find(|f| f.title.ends_with("iOS simulators are booted"))
                .expect("simulator finding");
            assert_eq!(f.severity, "info");
            assert!(f.not_proven[0].contains("may be driving each"));
        }
        for r in roots {
            let Some(dev) = crate::collect::sim_device_of(&r.argv) else {
                continue;
            };
            let w = snap.workload_of(r);
            assert_eq!(w.kind, "simulator");
            assert_eq!(w.key, format!("simulator:{}", dev.udid));
            assert_eq!(w.confidence, "exact");
            assert!(w.name.starts_with("iOS Simulator"), "{}", w.name);
            let mut stack = vec![r.pid];
            while let Some(pid) = stack.pop() {
                for k in snap.kids.get(&pid).into_iter().flatten() {
                    assert_eq!(
                        snap.workload_of(snap.proc_by_pid(*k).unwrap()).key,
                        w.key,
                        "descendant {k}"
                    );
                    stack.push(*k);
                }
            }
        }
        for p in snap
            .procs
            .iter()
            .filter(|p| p.comm.starts_with("CoreSimulatorSe") || p.comm == "Simulator")
        {
            assert_ne!(
                snap.workload_of(p).kind,
                "simulator",
                "host helper {} claimed",
                p.comm
            );
        }
    }

    #[test]
    fn history_roundtrip() {
        let dir = std::env::temp_dir().join(format!("procwho-hist-{}", std::process::id()));
        let cfg = Config {
            home: std::env::var("HOME").unwrap_or_default(),
            state: dir.clone(),
            registry: dir.join("none.json"),
        };
        let mut h = History::open(&dir).unwrap();
        let a = Snapshot::collect_with(cfg.clone());
        let r1 = h.record(&a).unwrap();
        assert_eq!(r1.samples, 1);
        assert!(r1.interval_s.is_none());
        let b = Snapshot::collect_with(cfg);
        let r2 = h.record(&b).unwrap();
        assert_eq!(r2.samples, 2);
        let k = &b.workloads[0].key;
        assert!(!h.trend(k, 1.0).is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
