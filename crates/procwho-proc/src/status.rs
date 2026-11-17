//! `status`: everything a GUI main screen needs from one snapshot and one CPU-rate read.
//!
//! JSON schema v3 is documented in docs/JSON.md. Strings that come from processes pass through
//! the redactor and the control-character filter before they leave this module.

use crate::findings::{findings, Finding};
use crate::fmt::{fmt_mem, sanitize};
use crate::redact::redact;
use crate::render::{sources_view, SourcesView, FOOTPRINT_NOTE, NOT_IN_RAM_NOTE};
use crate::snapshot::{check_cpu_interval, CpuRates, Snapshot};
use crate::sys;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write;

/// Default number of workloads in the `workloads` array.
pub const DEFAULT_LIMIT: usize = 80;

#[derive(Debug, Clone)]
pub struct StatusOpts {
    /// Workloads to list, largest footprint first; 0 lists all.
    pub limit: usize,
    pub interval: f64,
    pub json: bool,
    /// Include every exclusive process in each returned workload.
    pub members: bool,
}

impl Default for StatusOpts {
    fn default() -> Self {
        StatusOpts {
            limit: DEFAULT_LIMIT,
            interval: 1.0,
            json: false,
            members: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Totals {
    pub accounting: &'static str,
    pub processes: usize,
    pub workloads: usize,
    pub footprint_bytes: u64,
    pub agent_bytes: u64,
    pub agent_associated_bytes: u64,
    pub agent_associated_medium_bytes: u64,
    pub unreadable_processes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct KindRow {
    pub accounting: &'static str,
    pub kind: String,
    pub workloads: usize,
    pub processes: usize,
    pub footprint_bytes: u64,
    pub cpu_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkloadRow {
    pub agent_link: Option<crate::agent_link::AgentLink>,
    pub bundle_path: Option<String>,
    pub accounting: &'static str,
    pub key: String,
    pub name: String,
    pub kind: String,
    pub confidence: String,
    pub processes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<crate::render::Member>>,
    pub footprint_bytes: u64,
    pub rss_bytes: u64,
    pub not_in_ram_bytes: u64,
    pub not_in_ram_estimated: bool,
    pub cpu_pct: f64,
    pub age_seconds: Option<i64>,
    pub ports: Vec<String>,
    pub inside: Option<String>,
    pub session_id: Option<String>,
    pub project_path: Option<String>,
    pub root_pid: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FindingRow {
    pub severity: String,
    pub title: String,
    pub workload_key: Option<String>,
    pub reasons: Vec<String>,
    pub not_proven: Vec<String>,
    pub basis: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SystemView {
    pub ncpu: u32,
    pub mem_total_bytes: u64,
    pub memory_pressure: Option<&'static str>,
    pub mem_used_bytes: Option<u64>,
    pub mem_available_bytes: Option<u64>,
    pub mem_compressed_bytes: Option<u64>,
    pub mem_wired_bytes: Option<u64>,
    pub swap_used_bytes: u64,
    /// Bytes of swap files allocated so far; macOS grows them as needed.
    pub swap_total_bytes: u64,
    pub swap_disk_free_bytes: u64,
    pub swap_disk_total_bytes: u64,
    pub load1: f64,
    pub disk_free_bytes: u64,
    pub disk_total_bytes: u64,
}

/// The `status --json` document (schema version 3).
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub schema_version: u32,
    pub footprint_note: &'static str,
    pub not_in_ram_note: &'static str,
    pub generated_unix: i64,
    pub observed_at_unix: i64,
    pub collect_ms: u64,
    pub cpu_interval_s: f64,
    pub system: SystemView,
    pub totals: Totals,
    pub by_kind: Vec<KindRow>,
    pub workloads: Vec<WorkloadRow>,
    pub findings: Vec<FindingRow>,
    pub sources: SourcesView,
}

/// Sum exclusive workload footprints once, regardless of nesting or link path.
pub fn agent_totals(workloads: &[crate::model::Workload]) -> (u64, u64, u64) {
    workloads
        .iter()
        .fold((0, 0, 0), |(agent, all, medium), w| match &w.agent_link {
            Some(link) => (
                agent + if link.relation == "agent" { w.fp } else { 0 },
                all + w.fp,
                medium + if link.confidence == "medium" { w.fp } else { 0 },
            ),
            None => (agent, all, medium),
        })
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Redacted, control-character-free form of a process-derived string.
fn safe(s: &str) -> String {
    sanitize(&redact(s))
}

fn finding_row(f: &Finding) -> FindingRow {
    FindingRow {
        severity: safe(&f.severity),
        title: safe(&f.title),
        workload_key: f.workload.as_deref().map(safe),
        reasons: f.reasons.iter().map(|s| safe(s)).collect(),
        not_proven: f.not_proven.iter().map(|s| safe(s)).collect(),
        basis: safe(&f.basis),
    }
}

/// Builds the document from a snapshot, its CPU rates and its findings. `limit` 0 = all.
pub fn build(snap: &Snapshot, rates: &CpuRates, fs: &[Finding], limit: usize) -> Status {
    build_with_members(snap, rates, fs, limit, false)
}

/// Build a status report, optionally including every exclusive member.
pub fn build_with_members(
    snap: &Snapshot,
    rates: &CpuRates,
    fs: &[Finding],
    limit: usize,
    include_members: bool,
) -> Status {
    let sys_info = sys::system_info(&snap.cfg.home);
    let mut order: Vec<usize> = (0..snap.workloads.len()).collect();
    order.sort_by(|&a, &b| {
        snap.workloads[b]
            .fp
            .cmp(&snap.workloads[a].fp)
            .then(a.cmp(&b))
    });

    let mut kinds: BTreeMap<&str, KindRow> = BTreeMap::new();
    for (i, w) in snap.workloads.iter().enumerate() {
        let k = kinds.entry(w.kind.as_str()).or_insert_with(|| KindRow {
            accounting: "exclusive",
            kind: safe(&w.kind),
            workloads: 0,
            processes: 0,
            footprint_bytes: 0,
            cpu_pct: 0.0,
        });
        k.workloads += 1;
        k.processes += w.procs;
        k.footprint_bytes += w.fp;
        k.cpu_pct += rates.per_workload[i];
    }
    let mut by_kind: Vec<KindRow> = kinds.into_values().collect();
    by_kind
        .iter_mut()
        .for_each(|k| k.cpu_pct = round1(k.cpu_pct));
    by_kind.sort_by(|a, b| {
        b.footprint_bytes
            .cmp(&a.footprint_bytes)
            .then_with(|| a.kind.cmp(&b.kind))
    });

    let take = if limit == 0 {
        order.len()
    } else {
        limit.min(order.len())
    };
    let workloads = order[..take]
        .iter()
        .map(|&i| {
            let w = &snap.workloads[i];
            let root_pid = w
                .roots
                .iter()
                .filter_map(|pid| snap.proc_by_pid(*pid))
                .max_by_key(|p| p.fp)
                .map(|p| p.pid);
            WorkloadRow {
                accounting: "exclusive",
                agent_link: w.agent_link.clone(),
                bundle_path: crate::apps::workload_bundle(snap, i),
                key: safe(&w.key),
                name: safe(&w.name),
                kind: safe(&w.kind),
                confidence: safe(&w.confidence),
                processes: w.procs,
                members: include_members.then(|| crate::render::members(snap, i, Some(rates))),
                footprint_bytes: w.fp,
                rss_bytes: w.rss,
                not_in_ram_bytes: w.fp.saturating_sub(w.rss),
                not_in_ram_estimated: true,
                cpu_pct: round1(rates.per_workload[i]),
                age_seconds: (w.oldest != 0.0).then(|| (snap.ts - w.oldest).max(0.0) as i64),
                ports: w.ports.iter().map(|p| safe(p)).collect(),
                inside: w.parent.map(|p| safe(&snap.workloads[p].name)),
                session_id: w.session_id.as_deref().map(safe),
                project_path: w.project.as_ref().map(|p| safe(&p.root)),
                root_pid,
            }
        })
        .collect();

    Status {
        footprint_note: FOOTPRINT_NOTE,
        not_in_ram_note: NOT_IN_RAM_NOTE,
        schema_version: 3,
        generated_unix: snap.ts as i64,
        observed_at_unix: snap.ts as i64,
        collect_ms: (snap.t_total * 1000.0).round() as u64,
        cpu_interval_s: round1(rates.interval),
        system: SystemView {
            ncpu: sys_info.ncpu,
            mem_total_bytes: sys_info.mem_total_bytes,
            memory_pressure: sys_info.memory_pressure,
            mem_used_bytes: sys_info.mem_used_bytes,
            mem_available_bytes: sys_info.mem_available_bytes,
            mem_compressed_bytes: sys_info.mem_compressed_bytes,
            mem_wired_bytes: sys_info.mem_wired_bytes,
            swap_used_bytes: sys_info.swap_used_bytes,
            swap_total_bytes: sys_info.swap_total_bytes,
            swap_disk_free_bytes: sys_info.swap_disk_free_bytes,
            swap_disk_total_bytes: sys_info.swap_disk_total_bytes,
            load1: (sys_info.load1 * 100.0).round() / 100.0,
            disk_free_bytes: sys_info.disk_free_bytes,
            disk_total_bytes: sys_info.disk_total_bytes,
        },
        totals: Totals {
            accounting: "exclusive",
            processes: snap.procs.len(),
            workloads: snap.workloads.len(),
            footprint_bytes: snap.footprint_sum(),
            agent_bytes: agent_totals(&snap.workloads).0,
            agent_associated_bytes: agent_totals(&snap.workloads).1,
            agent_associated_medium_bytes: agent_totals(&snap.workloads).2,
            unreadable_processes: snap.unreadable_count(),
        },
        by_kind,
        workloads,
        findings: fs.iter().map(finding_row).collect(),
        sources: sources_view(snap),
    }
}

/// Short human summary: totals, by-kind table, findings count.
pub fn render_text(st: &Status) -> String {
    let mut s = String::new();
    let _ = writeln!(s, "{FOOTPRINT_NOTE}");
    let _ = writeln!(
        s,
        "Memory pressure: {}.",
        st.system.memory_pressure.unwrap_or("unknown")
    );
    let _ = writeln!(
        s,
        "Procwho status: {} processes in {} workloads use {} footprint; {} require root permission to read.",
        st.totals.processes,
        st.totals.workloads,
        fmt_mem(st.totals.footprint_bytes as f64),
        st.totals.unreadable_processes
    );
    let _ = writeln!(
        s,
        "System: {} CPUs. Load: {:.2}. Installed RAM: {}. Swap: {} used, {} allocated so far. Disk free: {} of {}.",
        st.system.ncpu,
        st.system.load1,
        fmt_mem(st.system.mem_total_bytes as f64),
        fmt_mem(st.system.swap_used_bytes as f64),
        fmt_mem(st.system.swap_total_bytes as f64),
        fmt_mem(st.system.disk_free_bytes as f64),
        fmt_mem(st.system.disk_total_bytes as f64)
    );
    let _ = writeln!(
        s,
        "Memory used: {}. Available: {}. Compressed: {}. Wired: {}.",
        st.system
            .mem_used_bytes
            .map_or("unknown".into(), |v| fmt_mem(v as f64)),
        st.system
            .mem_available_bytes
            .map_or("unknown".into(), |v| fmt_mem(v as f64)),
        st.system
            .mem_compressed_bytes
            .map_or("unknown".into(), |v| fmt_mem(v as f64)),
        st.system
            .mem_wired_bytes
            .map_or("unknown".into(), |v| fmt_mem(v as f64))
    );
    let _ = writeln!(
        s,
        "CPU is the share of one core over {:.1}s. Collection took {} ms.\n",
        st.cpu_interval_s, st.collect_ms
    );
    let _ = writeln!(
        s,
        "{:>7} {:>9} {:>6} {:>5}  Kind",
        "Memory", "Workloads", "CPU%", "Procs"
    );
    for k in &st.by_kind {
        let _ = writeln!(
            s,
            "{:>7} {:>9} {:>6.1} {:>5}  {}",
            fmt_mem(k.footprint_bytes as f64),
            k.workloads,
            k.cpu_pct,
            k.processes,
            k.kind
        );
    }
    let review = st
        .findings
        .iter()
        .filter(|f| f.severity == "review")
        .count();
    let _ = writeln!(
        s,
        "\nThere are {} findings ({} to review). See `procwho findings`.",
        st.findings.len(),
        review
    );
    s
}

/// Runs one collection and returns JSON or the text summary.
pub fn status(o: &StatusOpts) -> Result<String, String> {
    check_cpu_interval(o.interval)?;
    let snap = Snapshot::collect();
    // Findings only need the snapshot, so they run while the CPU read waits out its interval.
    let (rates, fs) = std::thread::scope(|sc| {
        let h = sc.spawn(|| findings(&snap));
        let rates = snap.cpu_rates(o.interval);
        (rates, h.join().unwrap_or_default())
    });
    let st = build_with_members(&snap, &rates, &fs, o.limit, o.members);
    if o.json {
        crate::redact::to_json(&st).map_err(|e| e.to_string())
    } else {
        Ok(crate::fmt::clean_text(&render_text(&st)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Config;

    fn live() -> (Snapshot, CpuRates, Vec<Finding>) {
        let dir = std::env::temp_dir().join(format!("procwho-status-{}", std::process::id()));
        let cfg = Config {
            home: std::env::var("HOME").unwrap_or_default(),
            state: dir.clone(),
            registry: dir.join("none.json"),
        };
        let snap = Snapshot::collect_with(cfg);
        let rates = snap.cpu_rates(0.2);
        let fs = findings(&snap);
        (snap, rates, fs)
    }

    #[test]
    fn association_totals_are_exclusive_even_inside_host_apps() {
        use crate::agent_link::AgentLink;
        use crate::model::Workload;
        let mut ws: Vec<_> = [
            ("agent", "high", 10),
            ("agent_host_app", "high", 20),
            ("launched_by_agent", "medium", 30),
        ]
        .into_iter()
        .map(|(relation, confidence, fp)| Workload {
            fp,
            agent_link: Some(AgentLink {
                relation,
                confidence,
                via: "test".into(),
                agent_product: None,
            }),
            ..Default::default()
        })
        .collect();
        ws[0].parent = Some(1);
        ws.push(Workload {
            fp: 99,
            ..Default::default()
        });
        assert_eq!(agent_totals(&ws), (10, 60, 30));
    }

    #[test]
    fn default_sources_have_only_public_generic_names() {
        let mut snap = Snapshot::fixture(vec![]);
        snap.provider_note = "no label providers configured".into();
        let value = serde_json::to_value(sources_view(&snap)).unwrap();
        let keys: std::collections::BTreeSet<_> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(
            keys,
            [
                "launchctl",
                "lsof_ports",
                "tmux",
                "label_providers",
                "notes"
            ]
            .into_iter()
            .collect()
        );
        let rates = CpuRates {
            per_workload: vec![],
            per_proc: vec![],
            interval: 0.1,
        };
        let default_output = serde_json::to_value(build(&snap, &rates, &[], 0)).unwrap();
        let sources = default_output["sources"].as_object().unwrap();
        for key in sources.keys() {
            assert!(
                [
                    "launchctl",
                    "lsof_ports",
                    "tmux",
                    "label_providers",
                    "notes"
                ]
                .contains(&key.as_str()),
                "unexpected sources key {key}"
            );
        }
    }

    #[test]
    fn schema_shape_on_live_snapshot() {
        let (snap, rates, fs) = live();
        let st = build(&snap, &rates, &fs, 0);
        let v = serde_json::to_value(&st).unwrap();
        let top: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        for k in [
            "schema_version",
            "generated_unix",
            "collect_ms",
            "cpu_interval_s",
            "system",
            "totals",
            "by_kind",
            "workloads",
            "findings",
            "sources",
        ] {
            assert!(top.contains(&k), "missing {k}");
        }
        assert_eq!(top.len(), 13);
        assert_eq!(v["footprint_note"], FOOTPRINT_NOTE);
        assert_eq!(v["not_in_ram_note"], NOT_IN_RAM_NOTE);
        assert_eq!(v["schema_version"], 3);
        assert!(
            v["generated_unix"].is_i64()
                && v["collect_ms"].is_u64()
                && v["cpu_interval_s"].is_f64()
        );
        for k in [
            "ncpu",
            "mem_total_bytes",
            "swap_used_bytes",
            "swap_total_bytes",
            "disk_free_bytes",
            "disk_total_bytes",
        ] {
            assert!(v["system"][k].is_u64(), "system.{k}");
        }
        assert!(v["system"]["load1"].is_f64());
        assert!(
            v["system"]["ncpu"].as_u64().unwrap() > 0
                && v["system"]["mem_total_bytes"].as_u64().unwrap() > 0
        );
        assert!(
            v["system"]["disk_total_bytes"].as_u64().unwrap()
                >= v["system"]["disk_free_bytes"].as_u64().unwrap()
        );
        for k in [
            "processes",
            "workloads",
            "footprint_bytes",
            "unreadable_processes",
        ] {
            assert!(v["totals"][k].is_u64(), "totals.{k}");
        }
        for r in v["by_kind"].as_array().unwrap() {
            assert!(r["kind"].is_string() && r["cpu_pct"].is_f64());
            for k in ["workloads", "processes", "footprint_bytes"] {
                assert!(r[k].is_u64());
            }
        }
        let ws = v["workloads"].as_array().unwrap();
        assert_eq!(ws.len(), snap.workloads.len());
        for w in ws {
            assert_eq!(w.as_object().unwrap().len(), 19);
            assert_eq!(w["not_in_ram_estimated"], true);
            assert_eq!(
                w["not_in_ram_bytes"].as_u64(),
                Some(
                    w["footprint_bytes"]
                        .as_u64()
                        .unwrap()
                        .saturating_sub(w["rss_bytes"].as_u64().unwrap())
                )
            );
            for k in ["key", "name", "kind", "confidence"] {
                assert!(w[k].is_string(), "workload.{k}");
            }
            for k in ["processes", "footprint_bytes", "rss_bytes"] {
                assert!(w[k].is_u64());
            }
            assert!(w["cpu_pct"].is_f64() && w["ports"].is_array());
            for k in ["age_seconds", "root_pid"] {
                assert!(w[k].is_null() || w[k].is_i64(), "workload.{k}");
            }
            for k in ["inside", "session_id", "project_path"] {
                assert!(w[k].is_null() || w[k].is_string(), "workload.{k}");
            }
        }
        for f in v["findings"].as_array().unwrap() {
            assert_eq!(f.as_object().unwrap().len(), 6);
            assert!(f["severity"].is_string() && f["title"].is_string() && f["basis"].is_string());
            assert!(f["workload_key"].is_null() || f["workload_key"].is_string());
            assert!(f["reasons"].is_array() && f["not_proven"].is_array());
        }
        for k in ["launchctl", "lsof_ports", "tmux"] {
            assert!(v["sources"][k].is_boolean());
        }
        assert!(v["sources"]["label_providers"].is_string());
        assert_eq!(v["sources"].as_object().unwrap().len(), 5);
        assert!(v["sources"]["notes"].is_array());
    }

    #[test]
    fn sorted_limited_and_sums_match() {
        let (snap, rates, fs) = live();
        let all = build(&snap, &rates, &fs, 0);
        assert!(all
            .workloads
            .windows(2)
            .all(|w| w[0].footprint_bytes >= w[1].footprint_bytes));
        assert!(all
            .by_kind
            .windows(2)
            .all(|w| w[0].footprint_bytes >= w[1].footprint_bytes));
        assert_eq!(
            all.by_kind.iter().map(|k| k.workloads).sum::<usize>(),
            all.totals.workloads
        );
        assert_eq!(
            all.by_kind.iter().map(|k| k.processes).sum::<usize>(),
            all.totals.processes
        );
        assert_eq!(
            all.by_kind.iter().map(|k| k.footprint_bytes).sum::<u64>(),
            all.totals.footprint_bytes
        );
        let few = build(&snap, &rates, &fs, 3);
        assert_eq!(few.workloads.len(), 3.min(snap.workloads.len()));
        assert_eq!(few.workloads[0].key, all.workloads[0].key);
        // by_kind still covers everything when the workload list is cut
        assert_eq!(
            few.by_kind.iter().map(|k| k.processes).sum::<usize>(),
            few.totals.processes
        );
    }

    #[test]
    fn root_pid_resolves_to_its_workload() {
        let (snap, rates, fs) = live();
        let st = build(&snap, &rates, &fs, 0);
        let mut checked = 0;
        for w in st.workloads.iter().filter(|w| w.root_pid.is_some()) {
            let pid = w.root_pid.unwrap();
            let p = snap.proc_by_pid(pid).expect("root pid is live");
            assert_eq!(safe(&snap.workload_of(p).key), w.key);
            checked += 1;
        }
        assert!(checked > 0);
    }

    /// A process whose argv carries a secret must not leak it into names or keys.
    #[test]
    fn no_secret_in_workload_names_or_keys() {
        let mut child = std::process::Command::new("/bin/cat")
            .args([
                "-",
                "--api-key",
                "sk-abcdefghijklmnopqrstu",
                "hunter2-status-secret",
            ])
            .stdin(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let (snap, rates, fs) = live();
        let _ = child.kill(); // our own test child only
        let _ = child.wait();
        let st = build(&snap, &rates, &fs, 0);
        let text = serde_json::to_string(&st).unwrap();
        assert!(!text.contains("sk-abcdefghijkl"), "token leaked");
        assert!(!text.contains("hunter2-status-secret"), "secret leaked");
        for w in &st.workloads {
            assert!(!w.name.contains("sk-abc") && !w.key.contains("sk-abc"));
        }
    }

    #[test]
    fn text_summary_has_totals_and_findings_count() {
        let (snap, rates, fs) = live();
        let t = render_text(&build(&snap, &rates, &fs, 5));
        assert!(
            t.contains("processes in") && t.contains("Kind") && t.contains("findings"),
            "{t}"
        );
    }
}
