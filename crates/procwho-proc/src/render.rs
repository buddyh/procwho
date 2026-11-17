//! Text and JSON rendering of snapshots, plus the explanation helpers (`impact`, `reveal`).

use crate::fmt::{clean_text, fmt_age, fmt_mem, sanitize, shell_quote};
use crate::history::TrendRow;
use crate::model::*;
use crate::snapshot::{CpuRates, Snapshot};
use crate::sys;
use serde::Serialize;
use std::fmt::Write;

pub const FOOTPRINT_NOTE: &str =
    "The sum of process footprints can count shared memory more than once.";
pub const NOT_IN_RAM_NOTE: &str = "This estimate subtracts resident memory from footprint, with a minimum of zero; it is not measured swap.";

// ------------------------------------------------------------------ views

/// A process, redacted, for JSON output.
#[derive(Debug, Clone, Serialize)]
pub struct ProcView {
    pub pid: i32,
    pub instance_id: u64,
    pub started: Option<String>,
    pub ppid: i32,
    pub command: String,
    pub cwd: String,
    pub footprint_bytes: u64,
    pub watches: Option<String>,
}

/// One exclusive workload member, safe for display and JSON output.
#[derive(Debug, Clone, Serialize)]
pub struct Member {
    pub pid: i32,
    pub ppid: i32,
    pub name: String,
    pub command: String,
    pub footprint_bytes: Option<u64>,
    pub cpu_pct: Option<f64>,
    pub age_seconds: Option<i64>,
}

/// Build every exclusive member, largest footprint first, with unavailable memory last.
pub fn members(snap: &Snapshot, wi: usize, rates: Option<&CpuRates>) -> Vec<Member> {
    let mut rows: Vec<_> = snap.workloads[wi]
        .pids
        .iter()
        .map(|pid| {
            let index = snap.index[pid];
            let p = &snap.procs[index];
            let name = member_name(&p.exe, &p.cmd, &p.comm);
            Member {
                pid: p.pid,
                ppid: p.ppid,
                name: crate::redact::display(name),
                // Redact the complete command before cutting it, including Unicode characters.
                command: crate::redact::display(&p.cmd).chars().take(300).collect(),
                footprint_bytes: p.usage_available.then_some(p.fp),
                cpu_pct: rates
                    .filter(|_| p.usage_available)
                    .and_then(|r| r.per_proc.get(index))
                    .copied()
                    .map(round1),
                age_seconds: (p.start != 0.0).then(|| (snap.ts - p.start).max(0.0) as i64),
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.footprint_bytes
            .cmp(&a.footprint_bytes)
            .then(a.pid.cmp(&b.pid))
    });
    rows
}

/// The executable's base name. Some tools install the binary under a bare version number
/// (`.../versions/2.1.326`); the name the command was started with is more useful then.
fn member_name<'a>(exe: &'a str, cmd: &'a str, comm: &'a str) -> &'a str {
    let base = |path: &'a str| path.rsplit('/').next().unwrap_or(path);
    let is_version = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit() || c == '.');
    let exe_name = base(exe);
    if !exe_name.is_empty() && !is_version(exe_name) {
        return exe_name;
    }
    let arg0 = base(cmd.split_whitespace().next().unwrap_or(""));
    if !arg0.is_empty() && !is_version(arg0) {
        return arg0;
    }
    if comm.is_empty() {
        exe_name
    } else {
        comm
    }
}

/// Render at most 20 members for the text explanation.
fn member_table(rows: &[Member]) -> String {
    let mut out = String::from("\nProcesses\n");
    let _ = writeln!(out, "{:>7} {:>9} {:>7}  Name", "PID", "Memory", "CPU%");
    for row in rows.iter().take(20) {
        let _ = writeln!(
            out,
            "{:>7} {:>9} {:>7}  {}",
            row.pid,
            row.footprint_bytes
                .map_or("?".into(), |b| fmt_mem(b as f64)),
            row.cpu_pct.map_or("?".into(), |v| format!("{v:.1}")),
            row.name
        );
    }
    if rows.len() > 20 {
        let _ = writeln!(out, "{} more: use --json", rows.len() - 20);
    }
    out
}

/// A workload with the numbers and the evidence for its label.
#[derive(Debug, Clone, Serialize)]
pub struct WorkloadView {
    pub agent_link: Option<crate::agent_link::AgentLink>,
    pub bundle_path: Option<String>,
    pub accounting: &'static str,
    pub name: String,
    pub key: String,
    pub kind: String,
    pub confidence: String,
    pub evidence: Vec<String>,
    pub identity: Option<Vec<String>>,
    pub inside: Option<String>,
    pub processes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub members: Option<Vec<Member>>,
    pub footprint_bytes: u64,
    pub rss_bytes: u64,
    pub not_in_ram_bytes: u64,
    pub not_in_ram_estimated: bool,
    pub footprint_note: &'static str,
    pub cpu_pct: Option<f64>,
    pub cpu_sample_state: &'static str,
    pub cpu_interval_s: Option<f64>,
    pub ports: Vec<String>,
    pub zombies: usize,
    pub if_stopped: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sample_processes: Vec<ProcView>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourcesView {
    pub launchctl: bool,
    pub lsof_ports: bool,
    pub tmux: bool,
    pub label_providers: String,
    pub notes: Vec<String>,
}

pub fn proc_view(snap: &Snapshot, p: &Proc) -> ProcView {
    ProcView {
        pid: p.pid,
        instance_id: p.uniq,
        started: (p.start != 0.0).then(|| sys::strftime("%Y-%m-%dT%H:%M:%S", p.start)),
        ppid: p.ppid,
        command: crate::redact::display(&p.cmd),
        cwd: crate::redact::display(&p.cwd),
        footprint_bytes: p.fp,
        watches: snap.fsmon.get(&p.pid).map(|s| crate::redact::display(s)),
    }
}

fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// `per` = number of sample processes to include (largest footprint first); 0 for none.
pub fn workload_view(
    snap: &Snapshot,
    wi: usize,
    rates: Option<&CpuRates>,
    per: usize,
) -> WorkloadView {
    let w = &snap.workloads[wi];
    let mut sample = Vec::new();
    if per > 0 {
        let mut ps: Vec<&Proc> = w.pids.iter().map(|p| &snap.procs[snap.index[p]]).collect();
        ps.sort_by_key(|p| std::cmp::Reverse(p.fp));
        sample = ps
            .into_iter()
            .take(per)
            .map(|p| proc_view(snap, p))
            .collect();
    }
    WorkloadView {
        accounting: "exclusive",
        agent_link: w.agent_link.clone(),
        bundle_path: crate::apps::workload_bundle(snap, wi),
        name: crate::redact::display(&w.name),
        key: crate::redact::display(&w.key),
        kind: crate::redact::display(&w.kind),
        confidence: crate::redact::display(&w.confidence),
        evidence: w
            .evidence
            .iter()
            .map(|s| crate::redact::display(s))
            .collect(),
        identity: w
            .ident
            .as_ref()
            .map(|rows| rows.iter().map(|s| crate::redact::display(s)).collect()),
        inside: w
            .parent
            .map(|p| crate::redact::display(&snap.workloads[p].name)),
        processes: w.procs,
        members: None,
        footprint_bytes: w.fp,
        rss_bytes: w.rss,
        not_in_ram_bytes: w.fp.saturating_sub(w.rss),
        not_in_ram_estimated: true,
        footprint_note: FOOTPRINT_NOTE,
        cpu_pct: rates.map(|r| round1(r.per_workload[wi])),
        cpu_sample_state: if rates.is_some() {
            "sampled"
        } else {
            "not_sampled"
        },
        cpu_interval_s: rates.map(|r| r.interval),
        ports: w.ports.clone(),
        zombies: w.zombies,
        if_stopped: impact(snap, wi),
        sample_processes: sample,
    }
}

/// Named inclusive totals. Never add these to the nested rows again.
#[derive(Debug, Clone, Serialize)]
pub struct InclusiveTotals {
    pub accounting: &'static str,
    pub nested_workloads: usize,
    pub processes: usize,
    pub footprint_bytes: u64,
}

pub fn inclusive_totals(snap: &Snapshot, wi: usize) -> InclusiveTotals {
    let total = snap.inclusive(wi);
    InclusiveTotals {
        accounting: "inclusive",
        nested_workloads: total.children,
        processes: total.procs,
        footprint_bytes: total.fp,
    }
}

fn tmux_optional_missing(snap: &Snapshot) -> bool {
    snap.source_notes
        .iter()
        .any(|n| n == "tmux sessions: optional tool not installed")
}

pub fn sources_view(snap: &Snapshot) -> SourcesView {
    SourcesView {
        launchctl: snap.sources.launchctl,
        lsof_ports: snap.sources.lsof_ports,
        tmux: snap.sources.tmux,
        label_providers: crate::redact::display(&snap.provider_note),
        notes: {
            let mut notes = snap.source_notes.clone();
            if snap.established_unavailable() {
                notes.push(
                    "Established connections could not be read, so their counts are unknown."
                        .into(),
                );
            }
            for (ok, source) in [
                (snap.sources.launchctl, "launchd jobs"),
                (snap.sources.lsof_ports, "listening ports"),
                (
                    snap.sources.tmux || tmux_optional_missing(snap),
                    "tmux sessions",
                ),
            ] {
                if !ok {
                    notes.push(format!(
                        "Information about {source} is unavailable for this observation."
                    ));
                }
            }
            notes
        },
    }
}

// ------------------------------------------------------------------ explanations

/// What would be affected if this workload stopped. Descriptive only; procwho never stops anything.
pub fn impact(snap: &Snapshot, wi: usize) -> Vec<String> {
    let w = &snap.workloads[wi];
    let kids = snap.children_of(wi);
    let mut out = Vec::new();
    if let Some(i) = &w.impact {
        out.push(i.clone());
    }
    match w.kind.as_str() {
        "service" => {
            let m = w.meta.clone().unwrap_or_default();
            out.push(if m.keepalive {
                "The launchd KeepAlive setting restarts this service immediately.".into()
            } else {
                format!(
                    "The launchd start policy is {}.",
                    m.schedule
                        .as_deref()
                        .unwrap_or("unknown because its settings were not found")
                )
            });
            out.push(format!(
                "The launchd job {} manages this service.",
                w.label.as_deref().unwrap_or("?")
            ));
            if let Some(svc) = &w.svc {
                if !svc.purpose.is_empty() {
                    out.push(format!(
                        "Label provider '{}' describes its purpose as: {}.",
                        svc.provider, svc.purpose
                    ));
                }
            }
        }
        "agent-session" => {
            out.push("Stopping it ends this agent session and the processes it started.".into());
            out.push(match &w.session_id {
                Some(id) => format!("Resume the session with --resume {id}."),
                None => {
                    "The session ID is unknown, so find the transcript before you resume.".into()
                }
            });
        }
        "agent-host" => {
            out.push(format!(
                "Stopping it ends every {} thread hosted by {}.",
                w.agent
                    .as_deref()
                    .map(|a| if a == "codex" { "Codex" } else { a })
                    .unwrap_or("agent"),
                w.host.as_deref().unwrap_or("this client")
            ));
            out.push(format!(
                "It currently hosts {} tool sets and commands.",
                kids.len()
            ));
        }
        "agent-toolset" => {
            out.push("The owning Codex thread loses its MCP tools.".into());
            out.push("The process data does not identify that thread.".into());
        }
        "app" => {
            out.push("Stopping it closes the app or interrupts part of it.".into());
            out.push("The app or launchd can restart its helpers.".into());
        }
        "terminal" | "terminal-pane" => {
            out.push("Stopping it closes the shells and the work running in them.".into())
        }
        "watcher" => {
            out.push(
                "The next git command in that repository restarts its file-change monitor.".into(),
            );
            out.push("Stopping it loses only cached file-change information.".into());
        }
        "unknown" => out.push("The owner and dependencies are unknown.".into()),
        "detached" => {
            out.push("No supervisor is known to restart it.".into());
            out.push("No parent is waiting for it to finish.".into());
            out.push("Stopping it closes connections to its ports.".into());
        }
        "simulator" => {
            out.push(
                "Stopping it shuts down this simulated device and every app running in it.".into(),
            );
            out.push("A test run or agent session may be using this device.".into());
        }
        "system" => {
            out.push("Stopping this macOS component can interrupt system functions.".into())
        }
        "system-opaque" => {
            out.push("Reading these processes requires additional permissions.".into());
            out.push("Their owners and dependencies beyond the user ID are unknown.".into());
        }
        _ => {}
    }
    if !w.ports.is_empty() {
        out.push(format!(
            "It listens on TCP ports: {}.",
            w.ports
                .iter()
                .take(12)
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !kids.is_empty() {
        let names: Vec<String> = kids
            .iter()
            .map(|&k| snap.workloads[k].name.clone())
            .collect();
        out.push(format!(
            "Stopping it also stops {} nested workloads: {}.",
            kids.len(),
            names.join("; ")
        ));
    }
    let mut seen = std::collections::HashSet::new();
    out.into_iter()
        .map(|s| crate::redact::display(&s))
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

/// One read-only command the user (or an agent) can run to look at the owner.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RevealAction {
    pub label: String,
    /// The command as an argument vector: run it directly, no shell parsing needed.
    pub argv: Vec<String>,
    /// The same command as a shell string; every word is quoted with [`shell_quote`].
    pub command: String,
    /// Context that is not part of the command.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl RevealAction {
    fn new(label: &str, argv: &[&str], note: Option<String>) -> RevealAction {
        let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
        let command = argv
            .iter()
            .map(|a| shell_quote(a))
            .collect::<Vec<_>>()
            .join(" ");
        RevealAction {
            label: label.to_string(),
            argv,
            command,
            note,
        }
    }

    /// `label:   command   (note)` for text output.
    pub fn line(&self) -> String {
        let mut s = format!("{:<9} {}", format!("{}:", self.label), self.command);
        if let Some(n) = &self.note {
            let _ = write!(s, "   ({n})");
        }
        s
    }
}

/// Read-only commands the user can run to look at the owner themselves.
/// Every value that comes from a process, plist or registry is quoted.
pub fn reveal(snap: &Snapshot, wi: usize, p: Option<&Proc>) -> Vec<RevealAction> {
    let w = &snap.workloads[wi];
    let m = w.meta.clone().unwrap_or_default();
    let mut out = Vec::new();
    let proj = w
        .project
        .clone()
        .or_else(|| p.and_then(|p| snap.project_of(&p.cwd)));
    if let Some(pr) = proj {
        out.push(RevealAction::new("project", &["open", &pr.root], None));
    } else if let Some(p) = p {
        if !p.cwd.is_empty() && p.cwd != "/" {
            out.push(RevealAction::new("cwd", &["open", &p.cwd], None));
        }
    }
    if let Some(pl) = &m.plist {
        let target = format!("gui/{}/{}", sys::uid(), w.label.as_deref().unwrap_or("?"));
        out.push(RevealAction::new(
            "launchd",
            &["launchctl", "print", &target],
            Some(format!("plist {pl}")),
        ));
    }
    let mut logs: Vec<String> = Vec::new();
    for l in [m.stdout.clone(), m.stderr.clone()].into_iter().flatten() {
        if !logs.contains(&l) {
            logs.push(l);
        }
    }
    for l in logs {
        out.push(RevealAction::new("log", &["tail", "-n", "50", &l], None));
    }
    if let Some(t) = &w.tmux {
        out.push(RevealAction::new(
            "terminal",
            &["tmux", "attach", "-t", &t.session],
            None,
        ));
    }
    if let Some(b) = &w.bundle {
        out.push(RevealAction::new("app", &["open", "-R", b], None));
    }
    out
}

fn avg_cpu(rows: &[&TrendRow]) -> f64 {
    let n: f64 = rows
        .iter()
        .map(|r| r.cpu_delta_ns.unwrap_or(0) as f64)
        .sum();
    let d: f64 = rows.iter().map(|r| r.interval_s.unwrap_or(0.0)).sum();
    n / d / 1e7
}

fn cpu_rows(tr: &[TrendRow]) -> Vec<&TrendRow> {
    tr.iter()
        .filter(|r| r.cpu_delta_ns.is_some() && r.interval_s.is_some_and(|i| i != 0.0))
        .collect()
}

fn now_str() -> String {
    sys::strftime("%Y-%m-%d %H:%M:%S", sys::now())
}

// ------------------------------------------------------------------ top

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Mem,
    Cpu,
    Procs,
}

#[derive(Debug, Clone)]
pub struct TopOpts {
    pub n: usize,
    pub sort: SortKey,
    pub kind: Option<String>,
    pub grep: Option<String>,
    pub interval: f64,
    pub json: bool,
}

impl Default for TopOpts {
    fn default() -> Self {
        TopOpts {
            n: 35,
            sort: SortKey::Mem,
            kind: None,
            grep: None,
            interval: 1.0,
            json: false,
        }
    }
}

/// Workload indices after filtering and sorting.
pub fn select_workloads(
    snap: &Snapshot,
    rates: &CpuRates,
    o: &TopOpts,
) -> Result<Vec<usize>, String> {
    let rx = match &o.grep {
        Some(g) => Some(
            regex::RegexBuilder::new(g)
                .case_insensitive(true)
                .build()
                .map_err(|e| format!("--grep: {e}"))?,
        ),
        None => None,
    };
    let mut ws: Vec<usize> = (0..snap.workloads.len())
        .filter(|&i| {
            o.kind
                .as_ref()
                .is_none_or(|k| snap.workloads[i].kind.contains(k.as_str()))
        })
        .filter(|&i| {
            rx.as_ref()
                .is_none_or(|r| r.is_match(&snap.workloads[i].name))
        })
        .collect();
    match o.sort {
        SortKey::Mem => ws.sort_by(|&a, &b| snap.workloads[b].fp.cmp(&snap.workloads[a].fp)),
        SortKey::Procs => {
            ws.sort_by(|&a, &b| snap.workloads[b].procs.cmp(&snap.workloads[a].procs))
        }
        SortKey::Cpu => {
            ws.sort_by(|&a, &b| rates.per_workload[b].total_cmp(&rates.per_workload[a]))
        }
    }
    Ok(ws)
}

pub fn top(o: &TopOpts) -> Result<String, String> {
    crate::snapshot::check_cpu_interval(o.interval)?;
    let snap = Snapshot::collect();
    let rates = snap.cpu_rates(o.interval);
    let ws = select_workloads(&snap, &rates, o)?;
    if o.json {
        #[derive(Serialize)]
        struct Doc {
            schema_version: u32,
            observed_at_unix: i64,
            collect_ms: u64,
            generated: String,
            processes: usize,
            workloads_total: usize,
            workloads_matched: usize,
            footprint_sum_bytes: u64,
            accounting: &'static str,
            footprint_note: &'static str,
            not_in_ram_note: &'static str,
            not_inspectable_processes: usize,
            cpu_interval_s: f64,
            sources: SourcesView,
            workloads: Vec<WorkloadView>,
        }
        let d = Doc {
            schema_version: 3,
            observed_at_unix: snap.ts as i64,
            collect_ms: (snap.t_total * 1000.0).round() as u64,
            generated: sys::strftime("%Y-%m-%dT%H:%M:%S%z", sys::now()),
            processes: snap.procs.len(),
            workloads_total: snap.workloads.len(),
            workloads_matched: ws.len(),
            footprint_sum_bytes: snap.footprint_sum(),
            accounting: "exclusive",
            footprint_note: FOOTPRINT_NOTE,
            not_in_ram_note: NOT_IN_RAM_NOTE,
            not_inspectable_processes: snap.unreadable_count(),
            cpu_interval_s: round1(rates.interval),
            sources: sources_view(&snap),
            workloads: ws
                .iter()
                .take(o.n)
                .map(|&i| workload_view(&snap, i, Some(&rates), 0))
                .collect(),
        };
        return crate::redact::to_json(&d).map_err(|e| e.to_string());
    }
    let mut s = String::new();
    let _ = writeln!(s, "{FOOTPRINT_NOTE}");
    let system = sys::system_info(&snap.cfg.home);
    let _ = writeln!(
        s,
        "Memory pressure: {}.",
        system.memory_pressure.unwrap_or("unknown")
    );
    let _ = writeln!(
        s,
        "Memory used: {} of {}. Available: {}. Compressed: {}. Wired: {}.",
        system
            .mem_used_bytes
            .map_or("unknown".into(), |v| fmt_mem(v as f64)),
        fmt_mem(system.mem_total_bytes as f64),
        system
            .mem_available_bytes
            .map_or("unknown".into(), |v| fmt_mem(v as f64)),
        system
            .mem_compressed_bytes
            .map_or("unknown".into(), |v| fmt_mem(v as f64)),
        system
            .mem_wired_bytes
            .map_or("unknown".into(), |v| fmt_mem(v as f64))
    );
    let tot = snap.footprint_sum();
    let _ = writeln!(
        s,
        "Procwho at {}: {} processes in {} workloads use {} footprint; {} require root permission to read.",
        now_str(),
        snap.procs.len(),
        snap.workloads.len(),
        fmt_mem(tot as f64),
        snap.unreadable_count()
    );
    let _ = writeln!(
        s,
        "CPU is the share of one core over {:.1}s. Memory includes compressed and nonresident pages. Collection took {:.0} ms.\n",
        rates.interval,
        snap.t_total * 1000.0
    );
    let _ = writeln!(
        s,
        "{:>7} {:>6} {:>5} {:>7}  {:<14} {:<8} Workload",
        "Memory", "CPU%", "Procs", "Age", "Kind", "Identity confidence"
    );
    for &i in ws.iter().take(o.n) {
        let w = &snap.workloads[i];
        let _ = writeln!(
            s,
            "{:>7} {:>6.1} {:>5} {:>7}  {:<14} {:<8} {}",
            fmt_mem(w.fp as f64),
            rates.per_workload[i],
            w.procs,
            fmt_age(w.oldest, snap.ts),
            w.kind,
            format!("Identity: {}", w.confidence),
            w.name
        );
    }
    if ws.len() > o.n {
        let rest = &ws[o.n..];
        let _ = writeln!(
            s,
            "{:>7} {:>6.1} {:>5} {:>7}  ... {} more workloads (use -n, --kind, --grep)",
            fmt_mem(rest.iter().map(|&i| snap.workloads[i].fp).sum::<u64>() as f64),
            rest.iter().map(|&i| rates.per_workload[i]).sum::<f64>(),
            rest.iter().map(|&i| snap.workloads[i].procs).sum::<usize>(),
            "",
            rest.len()
        );
    }
    let gaps: Vec<&str> = [
        (snap.sources.launchctl, "launchd jobs"),
        (snap.sources.lsof_ports, "listening ports"),
        (
            snap.sources.tmux || tmux_optional_missing(&snap),
            "tmux sessions",
        ),
    ]
    .iter()
    .filter(|(ok, _)| !ok)
    .map(|(_, n)| *n)
    .collect();
    let _ = write!(s, "\nSources: {}", snap.provider_note);
    for note in &snap.source_notes {
        let _ = write!(s, "; {note}");
    }
    if !gaps.is_empty() {
        let _ = write!(
            s,
            "; Information about {} is unavailable for this observation.",
            gaps.join(", ")
        );
    }
    s.push('\n');
    Ok(clean_text(&s))
}

// ------------------------------------------------------------------ why

pub fn why_rule(target: &str) -> Result<String, String> {
    let snap = Snapshot::collect();
    let (wi, _) = snap
        .find(target)
        .ok_or_else(|| "procwho: no live workload matches target".to_string())?;
    crate::redact::to_json(&crate::naming_rule::suggested_rule(
        &snap.workloads[wi],
        &snap.procs,
    ))
    .map_err(|e| e.to_string())
}

pub fn why(target: &str, json: bool) -> Result<String, String> {
    let snap = Snapshot::collect();
    let Some((wi, pi)) = snap.find(target) else {
        return Err(format!(
            "procwho: no live PID or workload matches '{target}' (it may have exited)"
        ));
    };
    let w = &snap.workloads[wi];
    let p = pi.map(|i| &snap.procs[i]);
    let inc = snap.inclusive(wi);
    let hist = crate::history::History::open_existing(&snap.cfg.state);
    let tr = hist
        .as_ref()
        .map(|h| h.trend(&w.key, 24.0))
        .unwrap_or_default();
    let rv = reveal(&snap, wi, p);

    if json {
        #[derive(Serialize)]
        struct Anc {
            pid: i32,
            command: String,
        }
        #[derive(Serialize)]
        struct ProcDetail {
            #[serde(flatten)]
            base: ProcView,
            exe: String,
            tty: String,
            cpu_time_s: f64,
            memory_bytes: u64,
            ancestry: Vec<Anc>,
            original_parent: Option<String>,
            descendants: usize,
        }
        #[derive(Serialize)]
        struct HistView {
            samples: usize,
            span_min: f64,
            footprint_first_bytes: i64,
            footprint_last_bytes: i64,
            avg_cpu_pct: Option<f64>,
        }
        #[derive(Serialize)]
        struct Doc {
            schema_version: u32,
            observed_at_unix: i64,
            collect_ms: u64,
            workload: WorkloadView,
            suggested_rule: serde_json::Value,
            declared_owner: Option<String>,
            declared_notes: Option<String>,
            totals_inclusive: InclusiveTotals,
            history: Option<HistView>,
            process: Option<ProcDetail>,
            reveal: Vec<RevealAction>,
            read_only: &'static str,
            sources: SourcesView,
        }
        let process = p.map(|p| {
            let original_parent = (p.ppid == 1 && p.label.is_empty())
                .then(|| crate::attribute::original_parent_note(p.puniq, &snap.by_uniq));
            ProcDetail {
                base: proc_view(&snap, p),
                exe: p.exe.clone(),
                tty: p.tty.clone(),
                cpu_time_s: p.cpu_ns as f64 / 1e9,
                memory_bytes: p.fp,
                ancestry: snap
                    .chain(p.pid)
                    .into_iter()
                    .map(|a| Anc {
                        pid: a,
                        command: snap.procs[snap.index[&a]].cmd.clone(),
                    })
                    .collect(),
                original_parent,
                descendants: snap.subtree_len(p.pid) - 1,
            }
        });
        let cpu = cpu_rows(&tr);
        let d = Doc {
            schema_version: 3,
            observed_at_unix: snap.ts as i64,
            collect_ms: (snap.t_total * 1000.0).round() as u64,
            workload: WorkloadView {
                members: Some(members(&snap, wi, None)),
                ..workload_view(&snap, wi, None, 0)
            },
            suggested_rule: crate::naming_rule::suggested_rule(w, &snap.procs),
            declared_owner: w.owner.clone(),
            declared_notes: w.notes.clone(),
            totals_inclusive: inclusive_totals(&snap, wi),
            history: (tr.len() >= 2).then(|| HistView {
                samples: tr.len(),
                span_min: (tr[tr.len() - 1].ts - tr[0].ts) / 60.0,
                footprint_first_bytes: tr[0].fp,
                footprint_last_bytes: tr[tr.len() - 1].fp,
                avg_cpu_pct: (!cpu.is_empty()).then(|| round1(avg_cpu(&cpu))),
            }),
            process,
            reveal: rv,
            read_only: "Procwho reads process information without changing processes.",
            sources: sources_view(&snap),
        };
        return crate::redact::to_json(&d).map_err(|e| e.to_string());
    }

    let mut s = String::new();
    let _ = writeln!(s, "Workload   {}", w.name);
    let _ = writeln!(s, "CPU        No CPU interval was sampled.");
    let _ = writeln!(
        s,
        "Kind       {}    Identity confidence: {}",
        w.kind, w.confidence
    );
    if let Some(id) = w.ident.as_ref().filter(|v| !v.is_empty()) {
        let _ = writeln!(s, "Identity   {}", id.join("; "));
    }
    if w.owner.is_some() || w.notes.is_some() {
        let _ = writeln!(
            s,
            "Declared   Owner: {}. {}",
            w.owner.as_deref().unwrap_or("None"),
            w.notes.as_deref().unwrap_or("")
        );
    }
    let _ = write!(
        s,
        "Totals     {} processes use {} footprint and {} resident memory",
        w.procs,
        fmt_mem(w.fp as f64),
        fmt_mem(w.rss as f64)
    );
    if inc.children > 0 {
        let _ = write!(
            s,
            "; with {} nested workloads: {} processes, {}",
            inc.children,
            inc.procs,
            fmt_mem(inc.fp as f64)
        );
    }
    s.push('\n');
    let _ = writeln!(
        s,
        "Not in RAM (est.) ~{}; {NOT_IN_RAM_NOTE}",
        fmt_mem(w.fp.saturating_sub(w.rss) as f64)
    );
    let _ = writeln!(s, "{FOOTPRINT_NOTE}");
    if let Some(par) = w.parent {
        let _ = writeln!(s, "Inside     {}", snap.workloads[par].name);
    }
    let _ = writeln!(s, "Why this label:");
    for e in &w.evidence {
        let _ = writeln!(s, "  - {e}");
    }
    if let Some(p) = p {
        let started = if p.start != 0.0 {
            sys::strftime("%Y-%m-%d %H:%M:%S", p.start)
        } else {
            "?".into()
        };
        let _ = writeln!(
            s,
            "\nProcess    PID {}. Instance ID: {} (unique during this boot). Started: {}.",
            p.pid, p.uniq, started
        );
        let _ = writeln!(s, "Command    {}", p.cmd);
        let _ = writeln!(s, "Executable {}", sanitize(&p.exe));
        let _ = writeln!(
            s,
            "Folder     {}. Terminal: {}. Memory: {}. Total CPU time: {:.1}s.",
            if p.cwd.is_empty() {
                "(unreadable)".to_string()
            } else {
                sanitize(&p.cwd)
            },
            if p.tty.is_empty() { "-" } else { &p.tty },
            fmt_mem(p.fp as f64),
            p.cpu_ns as f64 / 1e9
        );
        if let Some(r) = snap.fsmon.get(&p.pid) {
            let _ = writeln!(s, "Watches    {}", sanitize(r));
        }
        let _ = writeln!(s, "Ancestry:");
        for a in snap.chain(p.pid) {
            let _ = writeln!(s, "  <- {:>6} {}", a, snap.procs[snap.index[&a]].cmd);
        }
        if p.ppid == 1 && p.label.is_empty() {
            let _ = writeln!(
                s,
                "  {}",
                crate::attribute::original_parent_note(p.puniq, &snap.by_uniq)
            );
        }
        let _ = writeln!(
            s,
            "Children   {} descendant processes.",
            snap.subtree_len(p.pid) - 1
        );
    }
    s.push_str(&member_table(&members(&snap, wi, None)));
    let _ = writeln!(s, "\nIf this were stopped:");
    for i in impact(&snap, wi) {
        let _ = writeln!(s, "  - {i}");
    }
    if tr.len() >= 2 {
        let cpu = cpu_rows(&tr);
        let _ = write!(
            s,
            "\nHistory    {} samples over {:.0} min: footprint {} to {}, processes {} to {}",
            tr.len(),
            (tr[tr.len() - 1].ts - tr[0].ts) / 60.0,
            fmt_mem(tr[0].fp as f64),
            fmt_mem(tr[tr.len() - 1].fp as f64),
            tr[0].procs,
            tr[tr.len() - 1].procs
        );
        if !cpu.is_empty() {
            let _ = write!(s, ", avg CPU {:.1}% of a core", avg_cpu(&cpu));
        }
        s.push('\n');
    } else {
        let _ = writeln!(
            s,
            "\nHistory    {} stored samples are insufficient for a trend. Use `procwho watch` to sample.",
            tr.len()
        );
    }
    if !rv.is_empty() {
        let _ = writeln!(s, "\nInspect:");
        for r in rv {
            let _ = writeln!(s, "  {}", r.line());
        }
    }
    let _ = writeln!(
        s,
        "\nProcwho reads process information without changing processes."
    );
    Ok(clean_text(&s))
}

// ------------------------------------------------------------------ tree

pub fn tree(target: &str, n: usize) -> Result<String, String> {
    let snap = Snapshot::collect();
    let Some((wi, _)) = snap.find(target) else {
        return Err(format!(
            "procwho: no live PID or workload matches '{target}' (it may have exited)"
        ));
    };
    let w = &snap.workloads[wi];
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{}  [{}, {} processes, {}]",
        w.name,
        w.kind,
        w.procs,
        fmt_mem(w.fp as f64)
    );
    let mine: std::collections::HashSet<i32> = w.pids.iter().copied().collect();

    fn show(
        snap: &Snapshot,
        mine: &std::collections::HashSet<i32>,
        pid: i32,
        depth: usize,
        s: &mut String,
    ) {
        let p = &snap.procs[snap.index[&pid]];
        let extra = snap
            .fsmon
            .get(&pid)
            .map(|r| format!("  -> {}", sanitize(r)))
            .unwrap_or_default();
        let ports = if p.ports.is_empty() {
            String::new()
        } else {
            format!("  [ports {}]", p.ports.join(","))
        };
        let z = if p.zombie { "  [zombie]" } else { "" };
        let _ = writeln!(
            s,
            "{:>7} {:>7} {:>6} {}{}{}{}{}",
            fmt_mem(p.fp as f64),
            fmt_age(p.start, snap.ts),
            pid,
            "  ".repeat(depth),
            p.cmd,
            extra,
            ports,
            z
        );
        let mut kids: Vec<i32> = snap.kids.get(&pid).cloned().unwrap_or_default();
        kids.sort_by(|a, b| {
            snap.procs[snap.index[b]]
                .fp
                .cmp(&snap.procs[snap.index[a]].fp)
        });
        for k in kids {
            if mine.contains(&k) {
                show(snap, mine, k, depth + 1, s);
            } else {
                let kw = &snap.workloads[snap.procs[snap.index[&k]].wl];
                let _ = writeln!(
                    s,
                    "{:>22} {}[nested workload: {}, {}]",
                    "",
                    "  ".repeat(depth + 1),
                    kw.name,
                    fmt_mem(kw.fp as f64)
                );
            }
        }
    }

    let mut roots: Vec<i32> = w
        .pids
        .iter()
        .copied()
        .filter(|x| !mine.contains(&snap.procs[snap.index[x]].ppid))
        .collect();
    roots.sort_by(|a, b| {
        snap.procs[snap.index[b]]
            .fp
            .cmp(&snap.procs[snap.index[a]].fp)
    });
    for r in roots.iter().take(n) {
        show(&snap, &mine, *r, 0, &mut s);
    }
    if roots.len() > n {
        let _ = writeln!(
            s,
            "... {} more root processes (-n to show)",
            roots.len() - n
        );
    }
    Ok(clean_text(&s))
}

// ------------------------------------------------------------------ findings

pub fn findings_cmd(json: bool) -> Result<String, String> {
    let snap = Snapshot::collect();
    let fs = crate::findings::findings(&snap);
    if json {
        return crate::redact::to_json(&serde_json::json!({"schema_version":2,"observed_at_unix":snap.ts as i64,"collect_ms":(snap.t_total * 1000.0).round() as u64,"findings":fs,"footprint_note":FOOTPRINT_NOTE})).map_err(|e| e.to_string());
    }
    let mut s = String::new();
    let _ = writeln!(
        s,
        "Procwho findings at {}: {} items to review.\n",
        now_str(),
        fs.len()
    );
    let _ = writeln!(s, "{FOOTPRINT_NOTE}");
    for f in &fs {
        let _ = writeln!(s, "[{}] {}", f.severity, f.title);
        for r in &f.reasons {
            let _ = writeln!(s, "    + {r}");
        }
        for r in &f.not_proven {
            let _ = writeln!(s, "    ? {r}");
        }
        let _ = writeln!(s, "    Basis: {}\n", f.basis);
    }
    Ok(clean_text(&s))
}

// ------------------------------------------------------------------ unknown

pub fn unknown(n: usize, json: bool) -> Result<String, String> {
    let snap = Snapshot::collect();
    let rank = |c: &str| match c {
        "none" => Some(0),
        "low" => Some(1),
        "medium" => Some(2),
        _ => None,
    };
    let mut ws: Vec<usize> = (0..snap.workloads.len())
        .filter(|&i| rank(&snap.workloads[i].confidence).is_some())
        .collect();
    ws.sort_by(|&a, &b| {
        let (wa, wb) = (&snap.workloads[a], &snap.workloads[b]);
        (rank(&wa.confidence), std::cmp::Reverse(wa.fp))
            .cmp(&(rank(&wb.confidence), std::cmp::Reverse(wb.fp)))
    });
    if json {
        #[derive(Serialize)]
        struct Row {
            missing_evidence: String,
            #[serde(flatten)]
            w: WorkloadView,
        }
        let rows: Vec<Row> = ws
            .iter()
            .take(n)
            .map(|&i| Row {
                missing_evidence: snap.workloads[i]
                    .evidence
                    .last()
                    .cloned()
                    .unwrap_or_default(),
                w: workload_view(&snap, i, None, 3),
            })
            .collect();
        return crate::redact::to_json(&serde_json::json!({"schema_version":3,"observed_at_unix":snap.ts as i64,"collect_ms":(snap.t_total * 1000.0).round() as u64,"workloads":rows})).map_err(|e| e.to_string());
    }
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{:<7} {:>7} {:>5}  Workload and missing evidence",
        "Identity confidence", "Memory", "Procs"
    );
    for &i in ws.iter().take(n) {
        let w = &snap.workloads[i];
        let _ = writeln!(
            s,
            "{:<7} {:>7} {:>5}  {}",
            format!("Identity: {}", w.confidence),
            fmt_mem(w.fp as f64),
            w.procs,
            w.name
        );
        let _ = writeln!(
            s,
            "{:<22} {}",
            "",
            w.evidence.last().map(String::as_str).unwrap_or("")
        );
    }
    Ok(clean_text(&s))
}

// ------------------------------------------------------------------ sample / watch / history

pub fn sample() -> Result<String, String> {
    let cfg = Config::from_env();
    // Opening the history first creates the state dir, so provider metadata can be cached.
    let mut h = crate::history::History::open(&cfg.state)?;
    let snap = Snapshot::collect_with(cfg);
    let r = h.record(&snap)?;
    Ok(format!(
        "sample stored: {} processes, {} workloads, collected in {:.0} ms (libproc pass {:.0} ms); history has {} samples, {:.0} KiB\n",
        snap.procs.len(),
        snap.workloads.len(),
        snap.t_total * 1000.0,
        snap.t_libproc * 1000.0,
        r.samples,
        r.db_bytes as f64 / 1024.0
    ))
}

static INTERRUPTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn on_sigint(_: libc::c_int) {
    INTERRUPTED.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Samples in the foreground. `emit` receives each output line (without newline).
/// `count == 0` runs until Ctrl-C. The interrupt only ends this loop; nothing else is signalled.
pub fn watch(interval: f64, count: u32, emit: &mut dyn FnMut(&str)) -> Result<(), String> {
    crate::snapshot::check_watch_interval(interval)?;
    let cfg = Config::from_env();
    emit(&format!(
        "Procwho watch takes one sample every {}s{}. History is at {}/history.db and retained for {} hours.",
        interval,
        if count > 0 {
            format!(", {count} samples")
        } else {
            ", until Ctrl-C".to_string()
        },
        cfg.state.display(),
        RETENTION_H
    ));
    // SAFETY: the handler only stores to an atomic.
    unsafe { libc::signal(libc::SIGINT, on_sigint as *const () as usize) };
    let mut h = crate::history::History::open(&cfg.state)?;
    let mut n = 0u32;
    while count == 0 || n < count {
        if INTERRUPTED.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        let cpu0 = sys::own_cpu_ms();
        let snap = Snapshot::collect_with(cfg.clone());
        n += 1;
        match h.record(&snap) {
            Ok(r) => emit(&format!(
                "{} sample {}: {} procs, {:.0} ms wall, {:.0} ms own CPU, db {:.0} KiB",
                sys::strftime("%H:%M:%S", sys::now()),
                n,
                snap.procs.len(),
                snap.t_total * 1000.0,
                sys::own_cpu_ms() - cpu0,
                r.db_bytes as f64 / 1024.0
            )),
            // A locked or full database loses this sample only; the loop keeps going.
            Err(e) => eprintln!("procwho watch: sample {n} not stored: {e}"),
        }
        if count == 0 || n < count {
            let end = std::time::Instant::now() + std::time::Duration::from_secs_f64(interval);
            while std::time::Instant::now() < end
                && !INTERRUPTED.load(std::sync::atomic::Ordering::SeqCst)
            {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }
    Ok(())
}

pub fn history(target: &str, hours: f64, n: usize) -> Result<String, String> {
    let snap = Snapshot::collect();
    let Some((wi, _)) = snap.find(target) else {
        return Err(format!(
            "procwho: no live PID or workload matches '{target}' (it may have exited)"
        ));
    };
    let w = &snap.workloads[wi];
    let tr = crate::history::History::open_existing(&snap.cfg.state)
        .map(|h| h.trend(&w.key, hours))
        .unwrap_or_default();
    let mut s = String::new();
    let _ = writeln!(
        s,
        "{}\n{:<9} {:>5} {:>8} {:>7} {:>5} {:>5}",
        w.name, "Time", "Procs", "Memory", "CPU%", "Started", "Exited"
    );
    for r in &tr[tr.len().saturating_sub(n)..] {
        let cpu = match (r.cpu_delta_ns, r.interval_s) {
            (Some(c), Some(i)) if i != 0.0 => format!("{:.1}", c as f64 / (i * 1e7)),
            _ => "-".into(),
        };
        let _ = writeln!(
            s,
            "{:<9} {:>5} {:>8} {:>7} {:>5} {:>5}",
            sys::strftime("%H:%M:%S", r.ts),
            r.procs,
            fmt_mem(r.fp as f64),
            cpu,
            r.births,
            r.deaths
        );
    }
    if tr.len() < 3 {
        let _ = writeln!(
            s,
            "Fewer than three samples are insufficient to establish a trend."
        );
    }
    Ok(clean_text(&s))
}

// ------------------------------------------------------------------ export

/// Redacted JSON inventory. With `out`, writes the file and returns a one-line note.
pub fn export(out: Option<&str>, per: usize) -> Result<String, String> {
    let snap = Snapshot::collect();
    let rates = snap.cpu_rates(1.0);
    #[derive(Serialize)]
    struct Units {
        mem: &'static str,
        cpu_pct: String,
    }
    #[derive(Serialize)]
    struct Doc {
        schema_version: u32,
        observed_at_unix: i64,
        generated: String,
        host_note: &'static str,
        units: Units,
        sources: SourcesView,
        collect_ms: u64,
        process_count: usize,
        accounting: &'static str,
        workloads: Vec<WorkloadView>,
        findings: Vec<crate::findings::Finding>,
    }
    let mut order: Vec<usize> = (0..snap.workloads.len()).collect();
    order.sort_by(|&a, &b| snap.workloads[b].fp.cmp(&snap.workloads[a].fp));
    let doc = Doc {
        schema_version: 3,
        observed_at_unix: snap.ts as i64,
        generated: sys::strftime("%Y-%m-%dT%H:%M:%S%z", sys::now()),
        host_note:
            "This local report redacts process arguments and excludes environments and transcripts.",
        units: Units {
            mem: "Memory is footprint bytes, including compressed pages.",
            cpu_pct: format!(
                "CPU is the percent of one core over {:.1}s.",
                rates.interval
            ),
        },
        sources: sources_view(&snap),
        collect_ms: (snap.t_total * 1000.0).round() as u64,
        process_count: snap.procs.len(),
        accounting: "exclusive",
        workloads: order
            .iter()
            .map(|&i| workload_view(&snap, i, Some(&rates), per))
            .collect(),
        findings: crate::findings::findings(&snap),
    };
    let txt = crate::redact::to_json(&doc).map_err(|e| e.to_string())?;
    match out {
        Some(path) => {
            std::fs::write(path, &txt).map_err(|e| format!("procwho: cannot write {path}: {e}"))?;
            Ok(format!(
                "Wrote {path} ({:.0} KiB, {} workloads).\n",
                txt.len() as f64 / 1024.0,
                doc.workloads.len()
            ))
        }
        None => Ok(txt + "\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_table_shows_all_small_lists_and_top_twenty_large_lists() {
        let rows: Vec<_> = (0..21)
            .map(|pid| Member {
                pid,
                ppid: 1,
                name: format!("process-{pid}"),
                command: String::new(),
                footprint_bytes: Some(21 - pid as u64),
                cpu_pct: None,
                age_seconds: None,
            })
            .collect();
        let small = member_table(&rows[..20]);
        assert!(small.contains("process-19"));
        assert!(!small.contains("more:"));
        let large = member_table(&rows);
        assert!(large.contains("Processes"));
        assert!(!large.contains("process-20"));
        assert!(large.contains("1 more: use --json"));
    }

    #[test]
    fn workload_estimate_is_explicit_and_floored_at_zero() {
        let mut snap = Snapshot::fixture(vec![]);
        snap.workloads.push(Workload {
            fp: 100,
            rss: 40,
            ..Default::default()
        });
        let row = workload_view(&snap, 0, None, 0);
        assert_eq!(row.not_in_ram_bytes, 60);
        assert!(row.not_in_ram_estimated);
        assert_eq!(row.footprint_note, FOOTPRINT_NOTE);
        assert_eq!(row.cpu_pct, None);
        assert_eq!(row.cpu_sample_state, "not_sampled");
        assert_eq!(row.cpu_interval_s, None);
        let rates = CpuRates {
            per_workload: vec![0.0],
            per_proc: vec![],
            interval: 0.2,
        };
        let sampled = workload_view(&snap, 0, Some(&rates), 0);
        assert_eq!(sampled.cpu_pct, Some(0.0));
        assert_eq!(sampled.cpu_sample_state, "sampled");
        assert_eq!(sampled.cpu_interval_s, Some(0.2));
        snap.workloads[0].rss = 200;
        assert_eq!(workload_view(&snap, 0, None, 0).not_in_ram_bytes, 0);
    }

    #[test]
    fn impact_preserves_every_complete_nested_name() {
        let mut snap = Snapshot::fixture(vec![]);
        snap.workloads = vec![Workload::default()];
        for i in 0..6 {
            snap.workloads.push(Workload {
                name: format!("project {i} {} complete ending", "long name ".repeat(20)),
                parent: Some(0),
                ..Default::default()
            });
        }
        let impact = impact(&snap, 0).join("\n");
        for child in &snap.workloads[1..] {
            assert!(impact.contains(&child.name));
        }
    }

    #[test]
    fn bad_intervals_are_errors_not_panics() {
        for v in [f64::NAN, f64::INFINITY, -1.0, 0.0, 1e300] {
            let o = TopOpts {
                interval: v,
                ..Default::default()
            };
            assert!(top(&o).is_err(), "top {v}");
        }
        for v in [f64::NAN, f64::INFINITY, 0.5, 1e9] {
            let mut lines = 0;
            assert!(watch(v, 1, &mut |_| lines += 1).is_err(), "watch {v}");
            assert_eq!(lines, 0, "nothing runs before validation");
        }
    }

    #[test]
    fn reveal_action_quotes_hostile_values() {
        let cwd = "/tmp/a'; touch /tmp/x; '";
        let a = RevealAction::new("cwd", &["open", cwd], None);
        assert_eq!(a.argv, vec!["open".to_string(), cwd.to_string()]);
        assert_eq!(a.command, r"open '/tmp/a'\''; touch /tmp/x; '\'''");
        let j = serde_json::to_value(&a).unwrap();
        assert_eq!(j["argv"][1], cwd);
        assert!(j.get("note").is_none());
    }

    #[test]
    fn member_name_prefers_the_started_name_over_a_bare_version() {
        assert_eq!(
            member_name("/opt/x/versions/2.1.326", "claude --resume", "2.1.326"),
            "claude"
        );
        assert_eq!(
            member_name("/usr/bin/node", "node server.js", "node"),
            "node"
        );
        assert_eq!(member_name("", "", "kernel_task"), "kernel_task");
    }
}
