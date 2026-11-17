//! Process -> workload attribution. Rules run in the order documented in ARCHITECTURE.md.

use crate::collect::plist_meta;
use crate::model::*;
use crate::snapshot::Snapshot;
use crate::sys;
use regex::Regex;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::OnceLock;

struct Res {
    app: Regex,
    signoise: Regex,
}

fn res() -> &'static Res {
    static R: OnceLock<Res> = OnceLock::new();
    R.get_or_init(|| Res {
        app: Regex::new(r"^(.*?/([^/]+)\.app)/").unwrap(),
        signoise: Regex::new(r"[\d.]{3,}|[A-Za-z0-9_-]*\d[A-Za-z0-9_-]{5,}").unwrap(),
    })
}

/// App bundles that only host an interpreter. A session started by a script is not "via Python".
const INTERPRETER_APPS: [&str; 2] = ["python", "python launcher"];

pub fn is_claude(p: &Proc) -> bool {
    crate::agents::is_family(p, "claude")
}

pub fn is_codex(p: &Proc) -> bool {
    crate::agents::is_family(p, "codex")
}

/// (app name, bundle path) when the executable lives inside an `.app`.
pub fn app_of(p: &Proc) -> Option<(String, String)> {
    res()
        .app
        .captures(&p.exe)
        .map(|c| (c[2].to_string(), c[1].to_string()))
}

fn base(s: &str) -> &str {
    s.rsplit('/').next().unwrap_or(s)
}

/// Stable "same kind of worker" key for duplicate detection.
///
/// The signature lands in workload names, finding titles and history, so it is built only
/// from data that is safe by construction: the executable basename plus the first argument
/// that looks like a script, module or subcommand. A candidate must not start with `-`,
/// must not contain `=`, must not be the value of a preceding flag (`tail -f FILE`,
/// `sshpass -p PASS`) and must survive the redactor unchanged. Python's `-m MODULE` is the
/// one flag whose value is used.
pub fn signature(p: &Proc) -> String {
    let exe_base = base(&p.exe);
    let b = if !exe_base.is_empty() {
        exe_base
    } else if !p.comm.is_empty() {
        &p.comm
    } else {
        "?"
    };
    let b = if crate::redact::redact(b) == b {
        b
    } else {
        "?"
    };
    let argv0 = p.argv.first().map(|a| base(a)).unwrap_or("");
    let python = argv0.starts_with("python") || argv0.starts_with("Python");
    let mut extra = String::new();
    let mut after_flag = false;
    for x in p.argv.iter().skip(1).take(6) {
        if x.starts_with('-') {
            // `--flag=value` carries its value; a bare flag may take the next token.
            let module_flag = python && x == "-m";
            after_flag = !(x.contains('=') || module_flag);
            continue;
        }
        if std::mem::take(&mut after_flag) || x.contains('=') || x.is_empty() {
            continue;
        }
        if crate::redact::redact(x) != *x {
            continue;
        }
        let parts: Vec<&str> = x.split('/').collect();
        let tail = parts[parts.len().saturating_sub(2)..].join("/");
        let s = res().signoise.replace_all(&tail, "#");
        extra = s.into_owned();
        break;
    }
    crate::fmt::sanitize(format!("{b} {extra}").trim())
}

/// What the kernel still says about a launchd-parented process's original parent.
///
/// After reparenting the kernel sometimes keeps the original parent's unique id and
/// sometimes reports launchd's (1), so 0 or 1 proves nothing about the original parent.
///
/// ```
/// use procwho_proc::attribute::original_parent_note as note;
/// use std::collections::HashMap;
/// let live = HashMap::from([(77u64, 4242i32)]);
/// assert!(note(1, &live).contains("unknown"));
/// assert!(note(77, &live).contains("PID 4242"));
/// assert!(note(78, &live).contains("no longer running"));
/// ```
pub fn original_parent_note(puniq: u64, by_uniq: &HashMap<u64, i32>) -> String {
    match puniq {
        0 | 1 => {
            "The original parent is unknown because macOS reports launchd's instance ID.".into()
        }
        u => match by_uniq.get(&u) {
            Some(&pid) if pid != 1 => {
                format!("The original parent, instance {u}, is still running as PID {pid}.")
            }
            _ => format!("The original parent, instance {u}, is no longer running."),
        },
    }
}

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
enum Family {
    T3,
    Desktop,
}

fn host_family(host: &str) -> Option<Family> {
    let h = host.to_lowercase();
    if h.contains("t3 code") {
        Some(Family::T3)
    } else if h.contains("chatgpt") || h.contains("codex") {
        Some(Family::Desktop)
    } else {
        None
    }
}

fn originator_family(o: &str) -> Option<Family> {
    let l = o.to_lowercase();
    if l.contains("t3") {
        Some(Family::T3)
    } else if l.contains("chatgpt")
        || l.contains("codex desktop")
        || l.contains("codex_desktop")
        || l.contains("codex app")
    {
        Some(Family::Desktop)
    } else {
        None
    }
}

/// Whether a rollout's `originator` can belong to the given host app. Unknown on either
/// side is compatible; a known host with a known but different originator is not
/// (for example originator "codex_exec" cannot be a thread of the T3 Code app).
///
/// ```
/// use procwho_proc::attribute::originator_compatible as ok;
/// assert!(ok(Some("T3 Code (Nightly)"), Some("T3 Code")));
/// assert!(ok(Some("ChatGPT"), Some("Codex Desktop")));
/// assert!(!ok(Some("T3 Code"), Some("Codex Desktop")));
/// assert!(!ok(Some("ChatGPT"), Some("codex_exec")));
/// assert!(ok(None, Some("codex_exec")));
/// assert!(ok(Some("T3 Code"), None));
/// ```
pub fn originator_compatible(host: Option<&str>, originator: Option<&str>) -> bool {
    match (
        host.and_then(host_family),
        originator.filter(|o| !o.is_empty()),
    ) {
        (Some(h), Some(o)) => originator_family(o) == Some(h),
        _ => true,
    }
}

// ------------------------------------------------------------------ table

#[derive(Default)]
struct Table {
    wl: Vec<Workload>,
    idx: HashMap<String, usize>,
}

impl Table {
    fn mk(
        &mut self,
        key: String,
        kind: &str,
        name: String,
        conf: &str,
        evidence: Vec<String>,
        init: impl FnOnce(&mut Workload),
    ) -> usize {
        let i = match self.idx.get(&key) {
            Some(&i) => i,
            None => {
                let mut w = Workload {
                    key: key.clone(),
                    kind: kind.to_string(),
                    name,
                    confidence: conf.to_string(),
                    ..Default::default()
                };
                init(&mut w);
                self.wl.push(w);
                self.idx.insert(key, self.wl.len() - 1);
                self.wl.len() - 1
            }
        };
        for e in evidence {
            if !self.wl[i].evidence.contains(&e) {
                self.wl[i].evidence.push(e);
            }
        }
        i
    }
}

enum Claim {
    None,
    Key(usize),
    /// Direct child of a shared agent host; resolved after clustering.
    HostChild(i32),
}

struct Attr<'a> {
    s: &'a Snapshot,
    t: Table,
    chains: Vec<Vec<usize>>,
    claimed: HashMap<i32, usize>,
    agents: Vec<Option<&'a crate::agents::AgentMatcher>>,
    clone_apps: Vec<Option<crate::apps::CloneApp>>,
}

impl Snapshot {
    /// Returns the workloads (in creation order) and, per process index, its workload index.
    pub(crate) fn attribute(&self) -> (Vec<Workload>, Vec<usize>) {
        let chains: Vec<Vec<usize>> = (0..self.procs.len())
            .map(|i| self.ownership_chain(i))
            .collect();
        let mut agents: Vec<_> = self
            .procs
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let ancestors: Vec<_> = chains[i].iter().map(|&j| &self.procs[j]).collect();
                crate::agents::detect_with_ancestors(p, &self.extra_agents, &ancestors)
            })
            .collect();
        let mut proven_ancestors = BTreeSet::new();
        for (i, matched) in agents.iter().enumerate() {
            if matched.is_some_and(|m| !m.rule.require_agent_descendant) {
                proven_ancestors.extend(chains[i].iter().copied());
            }
        }
        for (i, matched) in agents.iter_mut().enumerate() {
            if matched.is_some_and(|m| m.rule.require_agent_descendant)
                && !proven_ancestors.contains(&i)
            {
                *matched = None;
            }
        }
        let mut a = Attr {
            s: self,
            t: Table::default(),
            chains,
            agents,
            clone_apps: self
                .procs
                .iter()
                .map(|p| crate::apps::verified_clone(self, p))
                .collect(),
            claimed: HashMap::new(),
        };
        a.run()
    }

    // Ownership cannot cross a uid change or an unreadable -> readable edge.
    // Keep chain_idx intact for physical ancestry and MCP protection reporting.
    pub(crate) fn ownership_chain(&self, i: usize) -> Vec<usize> {
        let mut child = &self.procs[i];
        self.chain_idx(i)
            .into_iter()
            .take_while(|&j| {
                let parent = &self.procs[j];
                let same = child.uid == parent.uid && (!child.readable || parent.readable);
                child = parent;
                same
            })
            .collect()
    }

    /// Ancestors as indices, nearest first, stopping below launchd (pid 1).
    pub(crate) fn chain_idx(&self, i: usize) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        let mut cur = self.procs[i].ppid;
        while cur > 1 && out.len() < 40 {
            let Some(&ci) = self.index.get(&cur) else {
                break;
            };
            if out.contains(&ci) {
                break;
            }
            out.push(ci);
            cur = self.procs[ci].ppid;
        }
        out
    }
}

impl<'a> Attr<'a> {
    fn procs(&self) -> &'a [Proc] {
        &self.s.procs
    }

    fn run(&mut self) -> (Vec<Workload>, Vec<usize>) {
        let procs = self.procs();
        // Parents first; stable for equal depth.
        let mut order: Vec<usize> = (0..procs.len()).collect();
        order.sort_by_key(|&i| self.chains[i].len());
        let mut hostkids: Vec<(i32, Vec<usize>)> = Vec::new();
        for &i in &order {
            let boundary = self.boundary_note(i);
            let claim = self.claim(i);
            let claim = if matches!(claim, Claim::None) && boundary.is_some() {
                Claim::Key(self.t.mk(
                    format!("unknown:boundary:{}:{}", procs[i].pid, procs[i].uniq),
                    "unknown",
                    format!("UNKNOWN owner: {}", signature(&procs[i])),
                    "none",
                    vec!["No owner rule matched this process.".into()],
                    |_| {},
                ))
            } else {
                claim
            };
            match claim {
                Claim::Key(k) => {
                    if let Some(note) = boundary {
                        self.t.wl[k].evidence.push(note);
                    }
                    self.claimed.insert(procs[i].pid, k);
                }
                Claim::HostChild(h) => match hostkids.iter_mut().find(|(hp, _)| *hp == h) {
                    Some((_, v)) => v.push(i),
                    None => hostkids.push((h, vec![i])),
                },
                Claim::None => {}
            }
        }
        self.cluster_host_children(hostkids);
        // Everything else inherits its nearest claimed ancestor.
        let mut assign = vec![0usize; procs.len()];
        let mut parents: Vec<(usize, usize)> = Vec::new();
        for &i in &order {
            let p = &procs[i];
            let anc_claim = self.chains[i]
                .iter()
                .find_map(|&a| self.claimed.get(&procs[a].pid).copied());
            let key = if let Some(&k) = self.claimed.get(&p.pid) {
                self.t.wl[k].roots.push(p.pid);
                if let Some(par) = anc_claim {
                    if par != k {
                        parents.push((k, par));
                    }
                }
                k
            } else if let Some(k) = anc_claim {
                k
            } else {
                self.t.mk(
                    "unknown:unrooted".into(),
                    "unknown",
                    "UNKNOWN: no attributable ancestor".into(),
                    "none",
                    vec!["No ancestor has a known owner, and launchd is not the parent.".into()],
                    |_| {},
                )
            };
            assign[i] = key;
            self.t.wl[key].pids.push(p.pid);
        }
        for (k, par) in parents {
            if self.t.wl[k].parent.is_none() {
                self.t.wl[k].parent = Some(par);
            }
        }
        for w in &mut self.t.wl {
            let ps: Vec<&Proc> = w.pids.iter().map(|pid| &procs[self.s.index[pid]]).collect();
            w.procs = ps.len();
            w.fp = ps.iter().map(|p| p.fp).sum();
            w.rss = ps.iter().map(|p| p.rss).sum();
            w.cpu_ns = ps.iter().map(|p| p.cpu_ns).sum();
            let set: BTreeSet<&String> = ps.iter().flat_map(|p| p.ports.iter()).collect();
            let mut ports: Vec<String> = set.into_iter().cloned().collect();
            ports.sort_by_key(|x| {
                if x.bytes().all(|c| c.is_ascii_digit()) && !x.is_empty() {
                    x.parse::<u64>().unwrap_or(0)
                } else {
                    0
                }
            });
            w.ports = ports;
            w.zombies = ps.iter().filter(|p| p.zombie).count();
            w.oldest = ps
                .iter()
                .map(|p| p.start)
                .filter(|&s| s != 0.0)
                .fold(0.0, |a: f64, s| if a == 0.0 || s < a { s } else { a });
        }
        debug_assert!(self.t.wl.iter().all(|w| !w.pids.is_empty()));
        (std::mem::take(&mut self.t.wl), assign)
    }

    fn boundary_note(&self, i: usize) -> Option<String> {
        let p = &self.procs()[i];
        let parent = self.s.proc_by_pid(p.ppid)?;
        if p.uid == parent.uid && (!p.readable || parent.readable) {
            return None;
        }
        let owner = if parent.uid == 0 {
            "root".into()
        } else {
            format!("uid {}", parent.uid)
        };
        Some(format!(
            "Started by {} (PID {}), owned by {owner}; ownership is tracked separately from here.",
            crate::collect::display_argv0(base(if parent.exe.is_empty() {
                &parent.comm
            } else {
                &parent.exe
            })),
            parent.pid
        ))
    }

    /// Does this process START a workload?
    fn claim(&mut self, i: usize) -> Claim {
        let s = self.s;
        let p = &s.procs[i];
        let pid = p.pid;
        let par = self.chains[i].first().map(|&j| &s.procs[j]);
        if !p.readable {
            let who = if p.uid == 0 {
                "root".to_string()
            } else {
                format!("uid {}", p.uid)
            };
            return Claim::Key(self.t.mk(
                format!("opaque:{who}"),
                "system-opaque",
                format!("System processes owned by {who} (not inspectable without privileges)"),
                "n/a",
                vec!["Process arguments, working folders and memory require permission to read for other users.".into()],
                |_| {},
            ));
        }
        if let Some(o) = p.env.owner.as_ref().filter(|o| !o.is_empty()) {
            return Claim::Key(self.t.mk(
                format!("declared:{o}"),
                "declared",
                o.clone(),
                "declared",
                vec!["The process declares its owner with PROCWHO_OWNER.".into()],
                |_| {},
            ));
        }
        if !s.rules.is_empty() {
            let argv = p.raw.as_str();
            for r in &s.rules {
                let hay = |k: &str| match k {
                    "argv" => argv,
                    "exe" => p.exe.as_str(),
                    "cwd" => p.cwd.as_str(),
                    _ => p.label.as_str(),
                };
                if !r.res.is_empty() && r.res.iter().all(|(k, rx)| rx.is_match(hay(k))) {
                    let on: Vec<&str> = r
                        .res
                        .iter()
                        .map(|(k, _)| match *k {
                            "argv" => "arguments",
                            "exe" => "executable path",
                            "cwd" => "working folder",
                            _ => "launchd label",
                        })
                        .collect();
                    return Claim::Key(self.t.mk(
                        format!("rule:{}", r.name),
                        &r.kind,
                        r.name.clone(),
                        "declared",
                        vec![format!(
                            "A registry.json rule matches its {}.",
                            on.join(", ")
                        )],
                        |w| {
                            w.owner = r.owner.clone();
                            w.impact = r.impact.clone();
                            w.notes = r.notes.clone();
                        },
                    ));
                }
            }
        }
        // iOS Simulator: each booted device has one `launchd_sim` root; everything below it
        // belongs to that device, so descendants skip the label rules and inherit it.
        if self.chains[i].iter().any(|&a| {
            self.claimed
                .get(&s.procs[a].pid)
                .is_some_and(|&k| self.t.wl[k].kind == "simulator")
        }) {
            return Claim::None;
        }
        if let Some(dev) = crate::collect::sim_device_of(&p.argv) {
            let plist = Path::new(&dev.plist);
            let names = if s.is_local_path(&dev.plist) {
                crate::collect::sim_device_names(plist)
            } else {
                None
            };
            let short = &dev.udid[..8];
            let name = match &names {
                Some((n, Some(rt))) => format!("iOS Simulator: {} ({rt})", n),
                Some((n, None)) => format!("iOS Simulator: {}", n),
                None => format!("iOS Simulator {short}"),
            };
            let ev = vec![
                format!("The launchd_sim arguments name CoreSimulator device {}.", dev.udid),
                match &names {
                    Some(_) => format!("The device name comes from {}.", dev.plist),
                    None => format!("The device file at {} is unreadable, so its name uses the start of its device ID.", dev.plist),
                },
            ];
            return Claim::Key(self.t.mk(
                format!("simulator:{}", dev.udid),
                "simulator",
                name,
                "exact",
                ev,
                |_| {},
            ));
        }
        let lab = p.label.as_str();
        if lab.starts_with("com.apple.")
            || (!lab.is_empty()
                && ["/System/", "/usr/libexec/", "/usr/sbin/", "/usr/bin/"]
                    .iter()
                    .any(|x| p.exe.starts_with(x)))
        {
            return Claim::Key(self.t.mk(
                "system:apple".into(),
                "system",
                "macOS services (Apple launchd jobs)".into(),
                "exact",
                vec!["An Apple launchd label (com.apple.*) or system path identifies this macOS service.".into()],
                |_| {},
            ));
        }
        if !lab.is_empty() && !lab.starts_with("application.") {
            let key = format!("launchd:{lab}");
            if let Some(&k) = self.t.idx.get(&key) {
                return Claim::Key(k);
            }
            let svc = s.service_labels.get(lab).cloned();
            let meta = plist_meta(&s.cfg, lab);
            let name = match svc.as_ref().map(|v| v.name.as_str()) {
                Some(n) if n != lab => n.to_string(),
                _ => lab.to_string(),
            };
            let mut ev = vec![format!(
                "The launchd job '{lab}' lists this process ID in launchctl list."
            )];
            if let Some(svc) = &svc {
                ev.push(format!(
                    "Label provider '{}' lists this label ({}).",
                    svc.provider, s.provider_note
                ));
                if !svc.purpose.is_empty() {
                    ev.push(format!("Its recorded purpose is: {}.", svc.purpose));
                }
            }
            if let Some(pl) = &meta.plist {
                ev.push(format!("Its launchd settings are in {pl}."));
            }
            return Claim::Key(self.t.mk(key, "service", name, "exact", ev, |w| {
                w.label = Some(lab.to_string());
                w.meta = Some(meta);
                w.svc = svc;
            }));
        }
        let raw = p.raw.as_str();
        if raw.contains("--headless")
            && !raw.contains("--type=")
            && (raw.contains("--remote-debugging") || raw.contains("--user-data-dir"))
        {
            return Claim::Key(self.headless(i));
        }
        if let Some(matched) = self.agents[i] {
            let anc = &self.chains[i];
            if matched.rule.role == crate::agents::AgentRole::Toolset {
                let agent = matched.rule.name.as_str();
                let host = self.host_of(anc);
                return Claim::Key(self.t.mk(
                    format!("agent-helper:{pid}:{}", p.uniq),
                    "agent-toolset",
                    format!("{agent} helper: {}", base(&p.exe)),
                    matched.confidence(p),
                    vec![
                        format!("The {agent} helper path and required parent agent both match."),
                        "The helper has no known provider session ID.".into(),
                    ],
                    |w| {
                        w.agent = Some(agent.into());
                        w.host = host;
                    },
                ));
            }
            // Only a same-family launcher/native chain is deduplicated. An independently
            // launched agent (including a different provider) owns its own session.
            let nested = anc.iter().any(|&a| {
                self.agents[a].is_some_and(|m| {
                    m.rule.name == matched.rule.name
                        && (crate::agents::is_launcher(&s.procs[a])
                            || (m.rule.role == crate::agents::AgentRole::Host
                                && matched.rule.role == crate::agents::AgentRole::Host))
                })
            });
            if nested {
                return Claim::None;
            }
            let agent = matched.rule.name.as_str();
            let host = self.host_of(anc);
            let mode = matched.host_mode(p);
            if let Some(mode) = mode {
                let display_agent = if agent == "codex" { "Codex" } else { agent };
                let hd = host.clone().unwrap_or_else(|| "CLI".into());
                return Claim::Key(self.t.mk(
                    format!("agent-host:{pid}:{}", p.uniq),
                    "agent-host",
                    format!("{display_agent} {mode} (shared by all {hd} {display_agent} threads)"),
                    matched.confidence(p),
                    vec![
                        format!("Runs the {agent} host through {} in mode '{mode}'.", crate::redact::display(&p.exe)),
                        "The shared host does not identify individual sessions or their projects.".into(),
                        format!("The parent app is {}.", host.as_deref().unwrap_or("none")),
                    ],
                    |w| { w.host = host.clone(); w.agent = Some(agent.to_string()); },
                ));
            }
            let sid = matched.session_id(p);
            let t3 = p.env.t3_thread.clone().filter(|x| {
                !x.is_empty()
                    && x.len() <= 128
                    && x.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                    && crate::redact::redact(x) == *x
                    && anc.iter().any(|&a| {
                        self.agents[a].is_some_and(|m| {
                            m.rule.name == "T3 Code"
                                && m.rule.role == crate::agents::AgentRole::Host
                        })
                    })
            });
            let pane = std::iter::once(pid)
                .chain(anc.iter().map(|&a| s.procs[a].pid))
                .find_map(|a| s.pane_pid.get(&a))
                .cloned();
            let proj = s.project_of(&p.cwd);
            let exe_tail = crate::redact::display(&p.exe);
            let who = if exe_tail.is_empty() {
                p.argv
                    .first()
                    .map(|a| crate::collect::display_argv0(a))
                    .unwrap_or_default()
            } else {
                exe_tail
            };
            let display_agent = match agent {
                "claude" => "Claude Code",
                "codex" => "Codex",
                _ => agent,
            };
            let mut ev = vec![format!(
                "Runs {display_agent} through the executable at {who}."
            )];
            let mut ident = Vec::new();
            if let Some(sid) = &sid {
                ev.push("The process arguments contain an exact session ID.".into());
                ident.push(format!("The session ID is {sid}."));
            }
            if let Some(t3) = &t3 {
                ev.push("The T3CODE_THREAD_ID value identifies the exact T3 thread.".into());
                ident.push(format!("The T3 thread ID is {}.", t3));
            }
            if let Some(pn) = &pane {
                ev.push(format!(
                    "An ancestor belongs to tmux pane {}:{}.",
                    pn.session, pn.pane
                ));
                ident.push(format!("The tmux session is {}.", pn.session));
            }
            if sid.is_none() && t3.is_none() {
                ev.push("The session ID is not in the process arguments or environment.".into());
            }
            let headless = matched.headless(p);
            let where_ = match &proj {
                Some(pr) => pr.name.clone(),
                None => {
                    let b = base(&p.cwd);
                    if b.is_empty() {
                        "?".into()
                    } else {
                        b.to_string()
                    }
                }
            };
            let mut cap = agent.to_string();
            if let Some(first) = cap.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            let mut name = format!("{cap} session: {where_}");
            match (&pane, &host) {
                (Some(pn), _) => name.push_str(&format!(" [{}]", pn.session)),
                (None, Some(h)) => name.push_str(&format!(" [via {h}]")),
                _ => {}
            }
            if headless {
                name.push_str(" (headless)");
            }
            let conf = if sid.is_some() || t3.is_some() {
                "exact"
            } else {
                matched.confidence(p)
            };
            return Claim::Key(self.t.mk(
                format!("session:{pid}:{}", p.uniq),
                "agent-session",
                name,
                conf,
                ev,
                |w| {
                    w.agent = Some(agent.to_string());
                    w.session_id = sid;
                    w.t3_thread = t3;
                    w.tmux = pane;
                    w.host = host;
                    w.ident = Some(ident);
                    w.project = proj;
                },
            ));
        }
        if let Some(par) = par {
            if let Some(&k) = self.claimed.get(&par.pid) {
                if self.t.wl[k].kind == "agent-host" {
                    // App/backend services also run ordinary UI/editor infrastructure. Their
                    // generic children inherit the host; ancestry does not prove an agent command.
                    if s.index
                        .get(&par.pid)
                        .and_then(|&j| self.agents[j])
                        .is_some_and(|m| m.rule.role == crate::agents::AgentRole::Host)
                    {
                        return Claim::None;
                    }
                    return Claim::HostChild(par.pid);
                }
            }
        }
        if p.comm == "tmux" && p.ppid == 1 {
            return Claim::Key(self.t.mk(
                "terminal:tmux".into(),
                "terminal",
                "tmux server (all panes)".into(),
                "exact",
                vec!["Runs the tmux server.".into()],
                |_| {},
            ));
        }
        if let Some(d) = s.pane_pid.get(&pid) {
            return Claim::Key(self.pane_wl(
                d,
                "The process ID matches a pane listed by tmux list-panes.".into(),
            ));
        }
        // A login/SSH shell owns its descendants even when sshd is not inspectable.
        // Only the first shell below a login service starts a session; nested shells inherit.
        if ["sh", "bash", "zsh", "fish", "tcsh", "csh", "dash", "ksh"].contains(&base(&p.exe)) {
            let ancestry = s.chain_idx(i);
            let login = ancestry
                .iter()
                .map(|&j| &s.procs[j])
                .take_while(|q| {
                    !["sh", "bash", "zsh", "fish", "tcsh", "csh", "dash", "ksh"]
                        .contains(&base(&q.exe))
                })
                .find(|q| {
                    ["sshd", "sshd-session", "login", "su", "sudo"]
                        .contains(&base(if q.exe.is_empty() { &q.comm } else { &q.exe }))
                });
            let is_login = p.argv.first().is_some_and(|a| a.starts_with('-')) && !p.tty.is_empty();
            if login.is_some() || is_login {
                let ssh = login.is_some_and(|q| {
                    base(if q.exe.is_empty() { &q.comm } else { &q.exe }).starts_with("sshd")
                });
                let title = if ssh { "SSH session" } else { "Login shell" };
                let tty = if p.tty.is_empty() { "none" } else { &p.tty };
                return Claim::Key(self.t.mk(
                    format!("terminal:{pid}:{}", p.uniq),
                    "terminal",
                    format!("{title} (tty {tty})"),
                    "high",
                    vec![format!(
                        "Shell PID {pid} belongs to {}.",
                        login.map_or(
                            "a login session identified by its arguments and terminal".into(),
                            |q| format!("login service PID {}", q.pid)
                        )
                    )],
                    |_| {},
                ));
            }
        }
        if let Some(app) = &self.clone_apps[i] {
            let parent_same_app = self.chains[i].first().is_some_and(|&j| {
                app_of(&s.procs[j]).is_some_and(|a| a.1 == app.bundle)
                    || self.clone_apps[j]
                        .as_ref()
                        .is_some_and(|a| a.bundle == app.bundle)
            });
            if p.ppid == 1 || !parent_same_app {
                let key = self.t.mk(
                    format!("app:{}", app.name),
                    "app",
                    format!("{} (app)", app.name),
                    "medium",
                    vec![
                        format!(
                            "The executable uses a macOS code-sign clone path for {}.",
                            app.bundle_id
                        ),
                        format!("The command names the installed app at {}.", app.bundle),
                        "The installed bundle ID matches the clone path.".into(),
                        "The installed executable name matches the clone path.".into(),
                    ],
                    |w| w.bundle = Some(app.bundle.clone()),
                );
                self.t.wl[key].confidence = "medium".into();
                return Claim::Key(key);
            }
        }
        if let Some(app) = app_of(p) {
            let par_other = match par {
                None => true,
                Some(q) => app_of(q).is_none_or(|a| a.0 != app.0),
            };
            if (p.ppid == 1 || par_other) && lab.starts_with("application.") {
                return Claim::Key(self.t.mk(
                    format!("app:{}", app.0),
                    "app",
                    format!("{} (app)", app.0),
                    "exact",
                    vec![
                        format!("The launchd application label is {lab}."),
                        format!("The executable belongs to app bundle {}.", app.1),
                    ],
                    |w| w.bundle = Some(app.1.clone()),
                ));
            }
        }
        if p.ppid == 1 {
            return Claim::Key(self.orphan(i));
        }
        Claim::None
    }

    fn pane_wl(&mut self, d: &Pane, ev: String) -> usize {
        self.t.mk(
            format!("pane:{}:{}", d.session, d.pane),
            "terminal-pane",
            format!("tmux pane {}:{}", d.session, d.pane),
            "exact",
            vec![ev],
            |_| {},
        )
    }

    /// App hosting the agent: nearest ancestor inside an `.app`, ignoring interpreter bundles.
    fn host_of(&self, anc: &[usize]) -> Option<String> {
        anc.iter().find_map(|&a| {
            app_of(&self.s.procs[a])
                .map(|x| x.0)
                .filter(|n| !INTERPRETER_APPS.contains(&n.to_lowercase().as_str()))
        })
    }

    /// App name of the process macOS calls responsible, if it is an app.
    fn responsible_app(&self, resp: i32, excluded: &[i32]) -> Option<String> {
        if excluded.contains(&resp) {
            return None;
        }
        self.s
            .index
            .get(&resp)
            .and_then(|&j| app_of(&self.s.procs[j]))
            .map(|a| a.0)
    }

    fn headless(&mut self, i: usize) -> usize {
        let p = &self.s.procs[i];
        let proj = self.s.project_of(&p.cwd);
        let where_ = proj
            .as_ref()
            .map(|x| x.name.clone())
            .unwrap_or_else(|| p.cwd.clone());
        let via = self.responsible_app(p.resp, &[p.pid, -1]);
        let tool = if p.raw.contains("puppeteer") {
            "puppeteer"
        } else if p.raw.to_lowercase().contains("playwright") {
            "playwright"
        } else {
            "automation"
        };
        let mut ev = vec![
            format!(
                "The arguments request a browser without a window and a {tool} profile folder."
            ),
            format!("The working folder is {}.", p.cwd),
        ];
        if let Some(v) = &via {
            ev.push(format!(
                "The macOS responsibility API links this process to {v}."
            ));
        }
        let orphan = p.ppid == 1;
        let key = format!(
            "headless:{where_}:{}:{}",
            via.as_deref().unwrap_or("None"),
            if orphan {
                "orphan".to_string()
            } else {
                p.ppid.to_string()
            }
        );
        let name = format!(
            "Headless Chrome ({tool}) in {where_}{}",
            via.as_ref()
                .map(|v| format!(", launched via {v}"))
                .unwrap_or_default()
        );
        self.t.mk(
            key,
            if orphan {
                "detached"
            } else {
                "automation-browser"
            },
            name,
            "medium",
            ev,
            |w| {
                w.project = proj;
                w.via = via;
            },
        )
    }

    fn orphan(&mut self, i: usize) -> usize {
        let s = self.s;
        let p = &s.procs[i];
        let base_ev = vec![
            "The parent is launchd (PID 1), with no matching launchd job.".to_string(),
            "Its parent may have exited, or the process may have detached itself.".into(),
            original_parent_note(p.puniq, &s.by_uniq),
        ];
        if p.raw.contains("fsmonitor--daemon") {
            return self.t.mk(
                "watcher:git-fsmonitor".into(),
                "watcher",
                "git fsmonitor daemons (one per repo/worktree; self-detaching by design)".into(),
                "exact",
                vec![
                    "Runs 'git fsmonitor--daemon run --detach'.".into(),
                    "The daemon's open folder identifies the watched repository.".into(),
                ],
                |_| {},
            );
        }
        if !p.tty.is_empty() {
            if let Some(d) = s.pane_tty.get(&p.tty) {
                let ev = format!(
                    "This helper shares terminal {} with the pane, as prompt helpers such as gitstatus do.",
                    p.tty
                );
                return self.pane_wl(d, ev);
            }
        }
        if let Some(app) = app_of(p) {
            return self.t.mk(
                format!("app:{}", app.0),
                "app",
                format!("{} (app)", app.0),
                "high",
                vec![
                    format!("The executable runs inside app bundle {}.", app.1),
                    "macOS lists launchd as the parent of this app helper.".into(),
                ],
                |w| w.bundle = Some(app.1.clone()),
            );
        }
        if [
            "/System/",
            "/usr/libexec/",
            "/usr/sbin/",
            "/Library/Apple/",
            "/System/Volumes/Preboot/",
        ]
        .iter()
        .any(|x| p.exe.starts_with(x))
        {
            return self.t.mk(
                "system:apple".into(),
                "system",
                "macOS services (Apple launchd jobs)".into(),
                "high",
                vec!["The executable path identifies a macOS service or extension helper.".into()],
                |_| {},
            );
        }
        let proj = s.project_of(&p.cwd);
        let via = self.responsible_app(p.resp, &[p.pid, -1, 1]);
        let short = signature(p);
        if proj.is_some() || via.is_some() {
            let mut ev = base_ev;
            if let Some(pr) = &proj {
                ev.push(format!("The working folder is inside project {}.", pr.root));
            }
            if let Some(v) = &via {
                ev.push(format!(
                    "The macOS responsibility API links this process to {v}."
                ));
            }
            let key = format!(
                "detached:{short}:{}:{}",
                proj.as_ref().map(|x| x.name.as_str()).unwrap_or(""),
                via.as_deref().unwrap_or("")
            );
            let name = format!(
                "Detached '{short}'{}{}",
                proj.as_ref()
                    .map(|x| format!(" in {}", x.name))
                    .unwrap_or_default(),
                via.as_ref()
                    .map(|v| format!(", launched via {v}"))
                    .unwrap_or_default()
            );
            let conf = if proj.is_some() { "medium" } else { "low" };
            return self.t.mk(key, "detached", name, conf, ev, |w| {
                w.project = proj;
                w.via = via;
            });
        }
        let mut ev = base_ev;
        ev.push(format!(
            "The working folder {} has no project, and no app, label, session or terminal identifies an owner.",
            if p.cwd.is_empty() { "?" } else { &p.cwd }
        ));
        self.t.mk(
            format!("unknown:{short}"),
            "unknown",
            format!("UNKNOWN owner: {short}"),
            "none",
            ev,
            |_| {},
        )
    }

    /// Group direct provider-host children by start time; a burst does not prove thread identity.
    fn cluster_host_children(&mut self, hostkids: Vec<(i32, Vec<usize>)>) {
        let s = self.s;
        for (host, mut kids) in hostkids {
            let hw = self.claimed[&host];
            let host_app = self.t.wl[hw].host.clone();
            let hd = host_app.clone().unwrap_or_else(|| "CLI".into());
            let verified_codex_host = s
                .proc_by_pid(host)
                .and_then(|p| crate::agents::detect(p, &s.extra_agents))
                .is_some_and(|m| {
                    m.rule.name == "codex"
                        && m.rule.session_identity == crate::agents::SessionIdentity::ClaudeCodex
                });
            if !verified_codex_host {
                let agent = self.t.wl[hw]
                    .agent
                    .clone()
                    .unwrap_or_else(|| "agent".into());
                for &i in &kids {
                    let p = &s.procs[i];
                    let project = s.project_of(&p.cwd);
                    let key = self.t.mk(
                        format!("agent-command:{}:{}", p.pid, p.uniq),
                        "agent-command",
                        tool_name(
                            project.as_ref().map(|p| p.name.as_str()),
                            &hd,
                            &agent,
                            p.start,
                            Some(&signature(p)),
                        ),
                        "medium",
                        vec![
                            format!("The parent is a shared {agent} host that owns this command."),
                            "The owning agent session is unknown.".into(),
                        ],
                        |w| w.project = project,
                    );
                    self.claimed.insert(p.pid, key);
                }
                continue;
            }
            kids.sort_by(|&a, &b| s.procs[a].start.total_cmp(&s.procs[b].start));
            let mut clusters: Vec<Vec<usize>> = Vec::new();
            let mut cur: Vec<usize> = Vec::new();
            for &k in &kids {
                if let Some(&last) = cur.last() {
                    if s.procs[k].start - s.procs[last].start > 4.0 {
                        clusters.push(std::mem::take(&mut cur));
                    }
                }
                cur.push(k);
            }
            if !cur.is_empty() {
                clusters.push(cur);
            }
            for cl in clusters {
                let first = &s.procs[cl[0]];
                let t = first.start;
                if cl.len() >= 4 {
                    let key = self.toolset(host, &cl, t, &hd, host_app.as_deref());
                    for &k in &cl {
                        self.claimed.insert(s.procs[k].pid, key);
                    }
                } else {
                    for &k in &cl {
                        let p = &s.procs[k];
                        let proj = s.project_of(&p.cwd);
                        let name = tool_name(
                            proj.as_ref().map(|p| p.name.as_str()),
                            &hd,
                            "Codex",
                            p.start,
                            Some(&signature(p)),
                        );
                        let ev = if proj.is_some() {
                            "The owning thread is unknown; the working folder identifies the project."
                        } else {
                            "The owning thread and project are unknown."
                        };
                        let key = self.t.mk(
                            format!("hostcmd:{host}:{}:{}", p.pid, p.uniq),
                            "agent-command",
                            name,
                            if proj.is_some() { "medium" } else { "low" },
                            vec![
                                format!("The parent is shared Codex app-server PID {host}."),
                                ev.into(),
                            ],
                            |w| w.project = proj.clone(),
                        );
                        self.claimed.insert(p.pid, key);
                    }
                }
            }
        }
    }

    fn toolset(
        &mut self,
        host: i32,
        cl: &[usize],
        t: f64,
        hd: &str,
        _host_app: Option<&str>,
    ) -> usize {
        let projects: Vec<_> = cl
            .iter()
            .filter_map(|&i| self.s.project_of(&self.s.procs[i].cwd))
            .collect();
        // Only name a project when every helper resolves to the same one.
        let project = projects
            .first()
            .filter(|first| projects.len() == cl.len() && projects.iter().all(|p| p == *first))
            .cloned();
        let name = tool_name(
            project.as_ref().map(|p| p.name.as_str()),
            hd,
            "Codex",
            t,
            None,
        );
        // Timing can group helpers, but cannot bind a process to a provider thread.
        self.t.mk(
            format!("toolset:{host}:{}", self.s.procs[cl[0]].uniq),
            "agent-toolset",
            name,
            "low",
            vec![
                format!(
                    "{} helpers started within four seconds under shared host PID {host}.",
                    cl.len()
                ),
                "Start times group these helpers but do not identify their agent session.".into(),
            ],
            |w| w.project = project,
        )
    }
}

/// Recognizable context leads the display name; identity keys remain independent.
fn tool_name(
    project: Option<&str>,
    host: &str,
    agent: &str,
    started: f64,
    command: Option<&str>,
) -> String {
    let kind = if command.is_some() {
        "command"
    } else {
        "tool set"
    };
    let name = match project {
        Some(project) => format!("{project} · {agent} {kind}"),
        None => format!(
            "{host} · {agent} {kind} · started {}",
            sys::strftime("%H:%M", started)
        ),
    };
    command.map_or(name.clone(), |command| format!("{name} · {command}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc_with(exe: &str, argv: &[&str]) -> Proc {
        Proc {
            pid: 1,
            ppid: 1,
            uid: 501,
            pgid: 1,
            uniq: 1,
            puniq: 0,
            comm: "x".into(),
            exe: exe.into(),
            exe_gone: false,
            readable: true,
            argv: argv.iter().map(|s| s.to_string()).collect(),
            raw: argv.join(" "),
            env: Env::default(),
            cmd: String::new(),
            cwd: String::new(),
            tty: String::new(),
            fp: 0,
            rss: 0,
            usage_available: true,
            fp_max: 0,
            cpu_ns: 0,
            start: 0.0,
            zombie: false,
            ports: vec![],
            label: String::new(),
            resp: -1,
            wl: 0,
        }
    }

    #[test]
    fn code_sign_clone_requires_matching_installed_bundle_metadata() {
        let dir = std::env::temp_dir().join(format!("procwho-clone-{}", std::process::id()));
        let app = dir.join("Google Chrome.app");
        let binary = app.join("Contents/MacOS/Google Chrome");
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::write(&binary, "fixture").unwrap();
        let plist = app.join("Contents/Info.plist");
        let write_info = |id: &str, exe: &str| {
            let d = plist::Dictionary::from_iter([
                ("CFBundleIdentifier", plist::Value::String(id.into())),
                ("CFBundleExecutable", plist::Value::String(exe.into())),
            ]);
            plist::Value::Dictionary(d).to_file_xml(&plist).unwrap();
        };
        write_info("com.google.Chrome", "Google Chrome");
        let clone = "/private/var/folders/aa/bb/X/com.google.Chrome.code_sign_clone/code_sign_clone.xyz/Google Chrome.app.bundle/Contents/MacOS/Google Chrome";
        let mut root = proc_with(clone, &[binary.to_str().unwrap()]);
        root.pid = 20;
        let mut helper = proc_with(
            &format!(
                "{}/Contents/Helpers/Renderer.app/Contents/MacOS/Renderer",
                app.display()
            ),
            &["Renderer"],
        );
        helper.pid = 21;
        helper.ppid = 20;
        let mut detached_helper = helper.clone();
        detached_helper.pid = 22;
        detached_helper.ppid = 1;
        // A direct-path helper must not raise the clone root's identity confidence.
        let mut snap = Snapshot::fixture(vec![detached_helper, root, helper]);
        snap.mounts = vec![("/".into(), true)];
        let verified = crate::apps::verified_clone(&snap, &snap.procs[1]).unwrap();
        assert_eq!(verified.bundle_id, "com.google.Chrome");
        let (ws, assignment) = snap.attribute();
        assert_eq!(ws.len(), 1);
        assert_eq!(ws[0].kind, "app");
        assert_eq!(ws[0].confidence, "medium");
        assert_eq!(ws[0].procs, 3);
        assert_eq!(assignment, [0, 0, 0]);
        snap.workloads = ws;
        assert_eq!(
            crate::apps::workload_bundle(&snap, 0).as_deref(),
            app.to_str()
        );
        write_info("com.other.App", "Google Chrome");
        assert!(crate::apps::verified_clone(&snap, &snap.procs[1]).is_none());
        write_info("com.google.Chrome", "Other");
        assert!(crate::apps::verified_clone(&snap, &snap.procs[1]).is_none());
        write_info("com.google.Chrome", "Google Chrome");
        for invalid in ["/tmp/com.google.Chrome.code_sign_clone/code_sign_clone.xyz/Google Chrome.app.bundle/Contents/MacOS/Google Chrome", "/usr/bin/node"] {
            snap.procs[1].exe = invalid.into();
            assert!(crate::apps::verified_clone(&snap, &snap.procs[1]).is_none());
        }
        snap.procs[1].exe = clone.into();
        snap.procs[1].argv = vec!["Google Chrome".into(), binary.to_string_lossy().into()];
        assert!(crate::apps::verified_clone(&snap, &snap.procs[1]).is_none());
        snap.procs[1].argv = vec![dir
            .join("Missing.app/Contents/MacOS/Google Chrome")
            .to_string_lossy()
            .into()];
        assert!(crate::apps::verified_clone(&snap, &snap.procs[1]).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn workload_bundle_requires_an_existing_outer_app_of_its_root() {
        let dir = std::env::temp_dir().join(format!("procwho-bundle-{}", std::process::id()));
        let app = dir.join("Example.app");
        let exe = app.join("Contents/Helpers/Helper.app/Contents/MacOS/Helper");
        let mut p = proc_with(exe.to_str().unwrap(), &["Helper"]);
        p.pid = 20;
        let mut snap = Snapshot::fixture(vec![p]);
        snap.mounts = vec![("/".into(), true)];
        snap.workloads.push(Workload {
            roots: vec![20],
            ..Default::default()
        });
        assert!(crate::apps::workload_bundle(&snap, 0).is_none());
        std::fs::create_dir_all(&app).unwrap();
        assert_eq!(
            crate::apps::workload_bundle(&snap, 0).as_deref(),
            app.to_str()
        );
        snap.procs[0].exe = "/usr/bin/node".into();
        assert!(crate::apps::workload_bundle(&snap, 0).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn members_are_exclusive_sorted_redacted_and_nullable() {
        let mut root = proc_with(
            "/Applications/Example.app/Contents/MacOS/Example",
            &["Example"],
        );
        root.pid = 10;
        root.start = 50.0;
        root.cmd = "Example --password hidden-value".into();
        root.fp = 20;
        let mut child = root.clone();
        child.pid = 12;
        child.ppid = 10;
        child.exe = "/usr/bin/node".into();
        child.cmd = "λ".repeat(400);
        let mut missing = child.clone();
        missing.pid = 11;
        missing.usage_available = false;
        missing.start = 0.0;
        let mut nested = child.clone();
        nested.pid = 13;
        nested.exe = "/usr/local/bin/claude".into();
        nested.argv = vec!["claude".into()];
        nested.raw = "claude".into();
        nested.env.owner = Some("Nested session".into());
        let mut snap = Snapshot::fixture(vec![root, child, missing, nested]);
        snap.ts = 100.0;
        let (ws, assignment) = snap.attribute();
        snap.workloads = ws;
        for (p, wi) in snap.procs.iter_mut().zip(assignment) {
            p.wl = wi;
        }
        let rates = crate::snapshot::CpuRates {
            per_proc: vec![0.0, 1.25, 0.0, 0.0],
            per_workload: vec![0.0; snap.workloads.len()],
            interval: 0.1,
        };
        for wi in 0..snap.workloads.len() {
            let members = crate::render::members(&snap, wi, None);
            assert_eq!(members.len(), snap.workloads[wi].procs);
            assert!(members.iter().all(|m| m.cpu_pct.is_none()));
        }
        let wi = snap.procs[0].wl;
        assert_ne!(
            wi, snap.procs[3].wl,
            "nested workloads own their own processes"
        );
        let members = crate::render::members(&snap, wi, Some(&rates));
        assert_eq!(
            members.iter().map(|m| m.pid).collect::<Vec<_>>(),
            [10, 12, 11]
        );
        assert_eq!(members[0].name, "Example");
        assert!(!members[0].command.contains("hidden-value"));
        assert_eq!(members[0].age_seconds, Some(50));
        assert_eq!(members[0].cpu_pct, Some(0.0));
        assert_eq!(members[1].command.chars().count(), 300);
        assert_eq!(members[1].cpu_pct, Some(1.3));
        assert_eq!(members[2].footprint_bytes, None);
        assert_eq!(members[2].cpu_pct, None);
        assert_eq!(members[2].age_seconds, None);
        let status = crate::status::build_with_members(&snap, &rates, &[], 0, true);
        for row in status.workloads {
            assert_eq!(row.members.unwrap().len(), row.processes);
        }
        let plain = serde_json::to_value(crate::status::build(&snap, &rates, &[], 0)).unwrap();
        assert!(plain["workloads"]
            .as_array()
            .unwrap()
            .iter()
            .all(|w| w.get("members").is_none()));
    }

    fn process(pid: i32, ppid: i32, uid: u32, readable: bool, exe: &str) -> Proc {
        let mut p = proc_with(exe, &[base(exe)]);
        p.pid = pid;
        p.ppid = ppid;
        p.uid = uid;
        p.readable = readable;
        p.uniq = pid as u64;
        p
    }

    #[test]
    fn ssh_boundary_owns_shell_children_and_nests_agent_under_session() {
        let mut shell = process(12, 11, 501, true, "/bin/zsh");
        shell.tty = "ttys001".into();
        let mut s = Snapshot::fixture(vec![
            process(10, 1, 0, false, "/usr/sbin/sshd"),
            process(11, 10, 0, false, "/usr/libexec/sshd-session"),
            shell,
            process(13, 12, 501, true, "/bin/worker"),
            process(
                14,
                13,
                501,
                true,
                "/Users/user/.local/share/claude/versions/2.1.0",
            ),
        ]);
        let (workloads, assign) = s.attribute();
        for (p, &wi) in s.procs.iter_mut().zip(&assign) {
            p.wl = wi;
        }
        s.workloads = workloads;
        let shell = &s.workloads[assign[2]];
        assert_eq!(shell.kind, "terminal");
        assert_eq!(shell.name, "SSH session (tty ttys001)");
        assert_eq!(assign[2], assign[3]);
        assert_eq!(shell.parent, None);
        assert!(shell.evidence.iter().any(|e| e == "Started by sshd-session (PID 11), owned by root; ownership is tracked separately from here."));
        assert_eq!(s.workloads[assign[4]].kind, "agent-session");
        assert_eq!(s.workloads[assign[4]].parent, Some(assign[2]));
        assert_eq!(
            crate::render::workload_view(&s, assign[4], None, 0)
                .inside
                .as_deref(),
            Some("SSH session (tty ttys001)")
        );
        assert_eq!(s.owner_chain(14), vec![13, 12]);
        assert_eq!(s.chain(14), vec![13, 12, 11, 10]);
        assert!(!crate::render::impact(&s, assign[2])
            .iter()
            .any(|e| e.contains("part of macOS")));
        assert!(s
            .procs
            .iter()
            .filter(|p| p.readable)
            .all(|p| s.workload_of(p).kind != "system-opaque"));
    }

    #[test]
    fn helpers_never_inherit_across_readability_or_uid_boundaries() {
        for (uid, readable, label) in [
            (0, false, ""),
            (0, true, "com.apple.example"),
            (502, false, ""),
            (502, true, ""),
            (501, false, ""),
        ] {
            let mut parent = process(10, 1, uid, readable, "/usr/libexec/example");
            parent.label = label.into();
            let s = Snapshot::fixture(vec![
                parent,
                process(11, 10, 501, true, "/opt/example/helper"),
                process(12, 11, 501, true, "/opt/example/child"),
            ]);
            let (wl, assign) = s.attribute();
            let w = &wl[assign[1]];
            assert_eq!(w.kind, "unknown", "uid {uid}, readable {readable}");
            assert_eq!(assign[1], assign[2]);
            assert_ne!(assign[0], assign[1]);
            assert_eq!(w.parent, None);
            assert!(w
                .evidence
                .iter()
                .any(|e| e.contains("ownership is tracked separately from here")));
        }
    }

    #[test]
    fn login_sessions_are_separate_and_nested_shells_inherit() {
        let s = Snapshot::fixture(vec![
            process(10, 1, 0, false, "/usr/bin/login"),
            process(11, 10, 501, true, "/bin/zsh"),
            process(12, 11, 501, true, "/bin/bash"),
            process(13, 10, 501, true, "/bin/zsh"),
        ]);
        let (wl, assign) = s.attribute();
        assert_eq!(wl[assign[1]].name, "Login shell (tty none)");
        assert_eq!(assign[1], assign[2]);
        assert_ne!(assign[1], assign[3]);
    }

    #[test]
    fn table_preserves_native_agents_shims_and_shared_hosts() {
        for (exe, argv, kind, agent) in [
            ("/Users/user/.local/share/claude/versions/2.1.0", vec!["claude"], "agent-session", "claude"),
            ("/Users/user/.local/share/claude/versions/2.1.0", vec!["claude"], "agent-session", "claude"),
            ("/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex", vec!["codex", "app-server"], "agent-host", "codex"),
            ("/usr/local/bin/node", vec!["node", "/opt/node_modules/@openai/codex/bin/codex.js"], "agent-session", "codex"),
        ] {
            let snapshot = Snapshot::fixture(vec![proc_with(exe, &argv)]);
            let (workloads, _) = snapshot.attribute();
            assert_eq!(workloads[0].kind, kind);
            assert_eq!(workloads[0].agent.as_deref(), Some(agent));
        }
        assert!(!is_codex(&proc_with(
            "/opt/codex-code-mode-host/codex",
            &["host"]
        )));
        assert!(crate::agents::detect(
            &proc_with("/opt/codex-code-mode-host/codex", &["host"]),
            &[]
        )
        .is_none());
    }

    #[test]
    fn nested_agents_are_independent_and_extra_hosts_do_not_use_codex_inference() {
        let mut root = proc_with(
            "/Users/user/.local/share/claude/versions/2.1.0",
            &["claude"],
        );
        root.pid = 2;
        let mut child = proc_with("/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex", &["codex"]);
        child.pid = 3;
        child.ppid = 2;
        let (workloads, assign) = Snapshot::fixture(vec![root, child]).attribute();
        assert_eq!(workloads.len(), 2);
        assert_eq!(assign, [0, 1]);

        let mut root = proc_with("/opt/example-agent", &["example-agent", "serve"]);
        root.pid = 2;
        let mut child = proc_with("/bin/tool", &["tool"]);
        child.pid = 3;
        child.ppid = 2;
        let mut snapshot = Snapshot::fixture(vec![root, child]);
        let extra = crate::agents::AgentRule {
            name: "Example".into(),
            executables: vec!["example-agent".into()],
            shared_host_subcommands: vec!["serve".into()],
            ..Default::default()
        };
        snapshot.extra_agents = crate::agents::compile_extra(&[extra]);
        let (workloads, _) = snapshot.attribute();
        assert_eq!(workloads[0].kind, "agent-host");
        assert_eq!(workloads[0].confidence, "medium");
        assert_eq!(workloads[1].kind, "agent-command");
        assert!(workloads[1].session_id.is_none());
        assert!(!workloads[1].name.contains("Codex"));
        // A configured display name cannot opt into the built-in timing/identity policy.
        let extra = crate::agents::AgentRule {
            name: "codex".into(),
            executables: vec!["example-agent".into()],
            shared_host_subcommands: vec!["serve".into()],
            ..Default::default()
        };
        snapshot.extra_agents = crate::agents::compile_extra(&[extra]);
        let (workloads, _) = snapshot.attribute();
        assert_eq!(workloads[1].confidence, "medium");
        assert!(workloads[1]
            .evidence
            .iter()
            .any(|s| s.contains("owning agent session is unknown")));
    }

    #[test]
    fn tool_names_lead_with_project_or_host_without_claiming_thread_identity() {
        assert_eq!(
            tool_name(Some("Garden"), "Client", "Codex", 0.0, None),
            "Garden · Codex tool set"
        );
        let unknown = tool_name(None, "Client", "Codex", 0.0, None);
        assert!(unknown.starts_with("Client · Codex tool set · started "));
        assert!(!unknown.contains("unknown"));
        assert_eq!(
            tool_name(Some("Garden"), "Client", "Codex", 0.0, Some("build")),
            "Garden · Codex command · build"
        );
    }

    #[test]
    fn signature_keeps_the_complete_selected_argument() {
        let script = format!("{}-complete-ending.js", "long.project.name.".repeat(10));
        let p = proc_with("/usr/bin/node", &["node", &script]);
        assert!(signature(&p).ends_with(&script));
    }

    #[test]
    fn signature_skips_key_value_args() {
        // Prototype quirk (a): it picked the KEY=value token as the distinguishing word.
        let p = proc_with(
            "/usr/local/bin/node",
            &[
                "node",
                "NoDefaultCurrentDirectoryInExePath=1",
                "scripts/preview.js",
            ],
        );
        assert_eq!(signature(&p), "node scripts/preview.js");
        let q = proc_with("/usr/bin/codex", &["codex", "features.code_mode_host=true"]);
        assert_eq!(signature(&q), "codex");
        let r = proc_with(
            "/usr/bin/python3.11",
            &["python3.11", "-m", "example.serve"],
        );
        assert_eq!(signature(&r), "python3.11 example.serve");
    }

    #[test]
    fn signature_never_uses_flag_values() {
        let p = proc_with("/usr/bin/tail", &["tail", "-f", "sk-abcdefghijklmnopqrst"]);
        assert_eq!(signature(&p), "tail");
        let p = proc_with(
            "/opt/homebrew/bin/sshpass",
            &["sshpass", "-p", "hunter2", "ssh", "host"],
        );
        assert_eq!(signature(&p), "sshpass ssh");
        // A positional token the redactor would change is dropped, not shown.
        let p = proc_with("/bin/tool", &["tool", "ghp_abcdefghijklmnopqrstuvwxyz"]);
        assert_eq!(signature(&p), "tool");
    }

    #[test]
    fn signature_normal_shapes() {
        let p = proc_with("/usr/local/bin/node", &["node", "scripts/rea.mjs", "mcp"]);
        assert_eq!(signature(&p), "node scripts/rea.mjs");
        let p = proc_with("/usr/bin/python3", &["python3", "-m", "example.serve"]);
        assert_eq!(signature(&p), "python3 example.serve");
        let p = proc_with("/usr/bin/git", &["git", "fsmonitor--daemon", "run"]);
        assert_eq!(signature(&p), "git fsmonitor--daemon");
        let p = proc_with(
            "/usr/local/bin/node",
            &["node", "--max-old-space-size=4096", "server.js"],
        );
        assert_eq!(signature(&p), "node server.js");
    }

    #[test]
    fn signature_has_no_terminal_escapes() {
        let p = proc_with("/bin/x", &["x", "a\x1b[2Jb.js"]);
        assert_eq!(signature(&p), "x a?[2Jb.js");
    }

    #[test]
    fn workload_display_fields_are_sanitized() {
        let mut w = Workload {
            key: "declared:\x1b[2J".into(),
            name: "owner\x1b[2J\nx".into(),
            evidence: vec!["cwd /a\x1b]0;t\x07".into()],
            ident: Some(vec!["tmux \x1b[31m".into()]),
            ..Default::default()
        };
        w.sanitize_display();
        assert_eq!(w.name, "owner?[2J?x");
        assert_eq!(w.evidence[0], "cwd /a?]0;t?");
        assert_eq!(w.ident.as_ref().unwrap()[0], "tmux ?[31m");
        assert_eq!(w.key, "declared:\x1b[2J", "key is identity, not display");
    }

    #[test]
    fn signature_masks_noise() {
        let p = proc_with("/bin/x", &["x", "/a/b/t3-a4a9b1fb-b04ccd2a838460a9/run.js"]);
        assert_eq!(signature(&p), "x #/run.js");
    }

    #[test]
    fn interpreter_bundles_are_not_hosts() {
        let py = proc_with("/Library/Frameworks/Python.framework/Versions/3.11/Resources/Python.app/Contents/MacOS/Python", &["Python"]);
        assert_eq!(app_of(&py).map(|a| a.0).as_deref(), Some("Python"));
        assert!(INTERPRETER_APPS.contains(&"python"));
    }

    #[test]
    fn originator_rules() {
        assert!(originator_compatible(
            Some("T3 Code (Nightly)"),
            Some("T3 Code")
        ));
        assert!(!originator_compatible(
            Some("T3 Code (Nightly)"),
            Some("Codex Desktop")
        ));
        assert!(originator_compatible(
            Some("ChatGPT"),
            Some("Codex Desktop")
        ));
        assert!(!originator_compatible(Some("ChatGPT"), Some("T3 Code")));
        assert!(!originator_compatible(Some("T3 Code"), Some("codex_exec")));
        assert!(originator_compatible(None, Some("codex_exec")));
        assert!(originator_compatible(Some("Cursor"), Some("T3 Code")));
    }

    #[test]
    fn simulator_udid_comes_from_argv() {
        let argv = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let d = crate::collect::sim_device_of(&argv(&[
            "launchd_sim",
            "/Users/user/Library/Developer/CoreSimulator/Devices/aaaaaaaa-1111-2222-3333-bbbbbbbbbbbb/data/var/run/launchd_bootstrap.plist",
        ]))
        .unwrap();
        assert_eq!(d.udid, "AAAAAAAA-1111-2222-3333-BBBBBBBBBBBB");
        // not launchd_sim, relative, dot-dot, bad udid, missing path
        let bad = [
            argv(&[
                "launchd",
                "/x/CoreSimulator/Devices/AAAAAAAA-1111-2222-3333-BBBBBBBBBBBB/a",
            ]),
            argv(&[
                "launchd_sim",
                "x/CoreSimulator/Devices/AAAAAAAA-1111-2222-3333-BBBBBBBBBBBB/a",
            ]),
            argv(&[
                "launchd_sim",
                "/x/../CoreSimulator/Devices/AAAAAAAA-1111-2222-3333-BBBBBBBBBBBB/a",
            ]),
            argv(&["launchd_sim", "/x/CoreSimulator/Devices/not-a-udid/a"]),
            argv(&["launchd_sim"]),
        ];
        for b in &bad {
            assert!(crate::collect::sim_device_of(b).is_none(), "{b:?}");
        }
    }

    #[test]
    fn simulator_device_plist_fixture() {
        let dir = std::env::temp_dir().join(format!("procwho-sim-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("device.plist");
        std::fs::write(
            &f,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>name</key><string>iPhone 17 Pro</string>
<key>runtime</key><string>com.apple.CoreSimulator.SimRuntime.iOS-27-0</string>
<key>UDID</key><string>AAAAAAAA-1111-2222-3333-BBBBBBBBBBBB</string>
</dict></plist>"#,
        )
        .unwrap();
        assert_eq!(
            crate::collect::sim_device_names(&f),
            Some(("iPhone 17 Pro".to_string(), Some("iOS 27.0".to_string())))
        );
        assert!(crate::collect::sim_device_names(&dir.join("missing.plist")).is_none());
        assert!(
            crate::collect::sim_device_names(&dir).is_none(),
            "a directory is not read"
        );
        assert_eq!(
            crate::collect::pretty_runtime("com.apple.CoreSimulator.SimRuntime.watchOS-11-2"),
            "watchOS 11.2"
        );
        assert_eq!(crate::collect::pretty_runtime("custom"), "custom");
        let _ = std::fs::remove_dir_all(dir);
    }
}
