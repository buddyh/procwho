//! Associations supplement exclusive ownership; they never move or duplicate processes.
use crate::model::{Proc, Workload};
use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentLink {
    pub relation: &'static str,
    pub via: String,
    pub confidence: &'static str,
    pub agent_product: Option<String>,
}

fn product(name: &str) -> String {
    match name {
        "claude" => "Claude Code",
        "codex" => "Codex",
        other => other,
    }
    .into()
}

/// Resolve direct agents, table-listed host apps, then proven launching chains.
/// Unknown/dead original parents do not support a link. Services and simulators never do.
pub fn associate(procs: &[Proc], workloads: &mut [Workload]) {
    let table = crate::agents::builtins();
    let mut links: Vec<Option<AgentLink>> = workloads
        .iter()
        .map(|w| {
            w.kind.starts_with("agent-").then(|| AgentLink {
                relation: "agent",
                via: "agent workload".into(),
                confidence: "high",
                agent_product: w.agent.as_deref().map(product),
            })
        })
        .collect();
    for (i, w) in workloads
        .iter()
        .enumerate()
        .filter(|(_, w)| w.kind == "app")
    {
        let app = w
            .bundle
            .as_deref()
            .and_then(|b| b.rsplit('/').next())
            .and_then(|b| b.strip_suffix(".app"))
            .unwrap_or(&w.name);
        let Some(rule) = table
            .iter()
            .find(|r| r.app_bundles.iter().any(|a| a == app))
        else {
            continue;
        };
        let nested = workloads.iter().any(|child| {
            if !child.kind.starts_with("agent-") {
                return false;
            }
            let mut parent = child.parent;
            let mut seen = HashSet::new();
            while let Some(p) = parent.filter(|&p| p < workloads.len() && seen.insert(p)) {
                if p == i {
                    return true;
                }
                parent = workloads[p].parent;
            }
            false
        });
        if nested || rule.app_is_agent_product {
            links[i] = Some(AgentLink {
                relation: "agent_host_app",
                via: if nested {
                    "nested agent session"
                } else {
                    "app is agent product"
                }
                .into(),
                confidence: if nested { "high" } else { "medium" },
                agent_product: Some(rule.name.clone()),
            });
        }
    }
    let by_pid: HashMap<_, _> = procs.iter().map(|p| (p.pid, p)).collect();
    let by_uniq: HashMap<_, _> = procs
        .iter()
        .filter(|p| p.uniq > 1)
        .map(|p| (p.uniq, p))
        .collect();
    for (i, w) in workloads.iter().enumerate() {
        if !matches!(w.kind.as_str(), "detached" | "unknown" | "other") {
            continue;
        }
        let roots = if w.roots.is_empty() {
            &w.pids
        } else {
            &w.roots
        };
        let evidence: Option<Vec<AgentLink>> = roots
            .iter()
            .map(|pid| {
                let p = *by_pid.get(pid)?;
                if !p.label.is_empty() {
                    return None;
                }
                let mut current = p;
                let mut seen = HashSet::new();
                while current.puniq > 1 && seen.insert(current.uniq) {
                    let Some(parent) = by_uniq.get(&current.puniq) else {
                        break;
                    };
                    if parent.start > current.start && current.start > 0.0 {
                        break;
                    }
                    if parent.wl != i {
                        if let Some(link) = links
                            .get(parent.wl)
                            .and_then(Option::as_ref)
                            .filter(|l| l.relation != "launched_by_agent")
                        {
                            return Some(AgentLink {
                                relation: "launched_by_agent",
                                via: format!(
                                    "original parent: {}",
                                    crate::config::safe(&workloads[parent.wl].name)
                                ),
                                confidence: link.confidence,
                                agent_product: link.agent_product.clone(),
                            });
                        }
                    }
                    current = parent;
                }
                let responsible = by_pid.get(&p.resp)?;
                let link = links.get(responsible.wl)?.as_ref()?;
                if responsible.wl == i || link.relation == "launched_by_agent" {
                    return None;
                }
                Some(AgentLink {
                    relation: "launched_by_agent",
                    via: format!(
                        "responsible app: {}",
                        crate::config::safe(&workloads[responsible.wl].name)
                    ),
                    confidence: "medium",
                    agent_product: link.agent_product.clone(),
                })
            })
            .collect();
        if let Some(evidence) = evidence.filter(|e| !e.is_empty()) {
            // Every root must have evidence. Preserve the weakest link in mixed groups.
            links[i] = evidence
                .iter()
                .find(|l| l.confidence == "medium")
                .or(evidence.first())
                .cloned();
        }
    }
    for (w, link) in workloads.iter_mut().zip(links) {
        w.agent_link = link;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn proc(pid: i32, wl: usize) -> Proc {
        Proc {
            pid,
            ppid: 1,
            uid: 501,
            pgid: pid,
            uniq: pid as u64,
            puniq: 1,
            comm: "tool".into(),
            exe: "/usr/bin/tool".into(),
            exe_gone: false,
            readable: true,
            argv: vec![],
            raw: String::new(),
            env: Default::default(),
            cmd: "tool".into(),
            cwd: String::new(),
            tty: String::new(),
            fp: 10,
            rss: 10,
            usage_available: true,
            fp_max: 10,
            cpu_ns: 0,
            start: 1.0,
            zombie: false,
            ports: vec![],
            label: String::new(),
            resp: -1,
            wl,
        }
    }
    fn workload(kind: &str, pid: i32) -> Workload {
        Workload {
            kind: kind.into(),
            name: format!("work {pid}"),
            pids: vec![pid],
            roots: vec![pid],
            fp: 10,
            ..Default::default()
        }
    }
    #[test]
    fn direct_nested_original_responsible_and_exclusions() {
        let mut ws = vec![
            workload("app", 10),
            workload("agent-session", 20),
            workload("detached", 30),
            workload("unknown", 40),
            workload("simulator", 50),
            workload("service", 60),
            workload("unknown", 70),
        ];
        ws[0].bundle = Some("/Applications/Zed.app".into());
        ws[1].parent = Some(0);
        ws[1].agent = Some("codex".into());
        let mut ps: Vec<_> = (0..7).map(|i| proc((i as i32 + 1) * 10, i)).collect();
        ps[2].puniq = 20;
        ps[3].resp = 10;
        ps[4].puniq = 20;
        ps[4].resp = 20;
        ps[5].puniq = 20;
        ps[5].resp = 20;
        ps[6].resp = 1;
        associate(&ps, &mut ws);
        assert_eq!(
            ws[0].agent_link.as_ref().unwrap().relation,
            "agent_host_app"
        );
        assert_eq!(ws[0].agent_link.as_ref().unwrap().confidence, "high");
        assert_eq!(
            ws[1].agent_link.as_ref().unwrap().agent_product.as_deref(),
            Some("Codex")
        );
        assert_eq!(ws[2].agent_link.as_ref().unwrap().confidence, "high");
        assert_eq!(ws[3].agent_link.as_ref().unwrap().confidence, "medium");
        for w in &ws[4..] {
            assert!(w.agent_link.is_none());
        }
        ws[1].parent = None;
        associate(&ps, &mut ws);
        assert!(ws[0].agent_link.is_none());
        ws[0].bundle = Some("/Applications/Claude.app".into());
        associate(&ps, &mut ws);
        assert_eq!(ws[0].agent_link.as_ref().unwrap().confidence, "medium");
    }
    #[test]
    fn table_products_and_generic_apps_have_distinct_policy() {
        for app in [
            "Cursor",
            "Windsurf",
            "Devin",
            "Antigravity",
            "Claude",
            "ChatGPT",
            "Kiro",
            "T3 Code",
            "Conductor",
        ] {
            let mut w = workload("app", 10);
            w.bundle = Some(format!("/Applications/{app}.app"));
            let mut ws = vec![w];
            associate(&[proc(10, 0)], &mut ws);
            let link = ws[0]
                .agent_link
                .as_ref()
                .unwrap_or_else(|| panic!("missing {app}"));
            assert_eq!(link.relation, "agent_host_app");
            assert_eq!(link.confidence, "medium");
        }
        let mut app = workload("app", 10);
        app.bundle = Some("/Applications/Terminal.app".into());
        let mut agent = workload("agent-session", 20);
        agent.parent = Some(0);
        let mut ws = vec![app, agent];
        associate(&[proc(10, 0), proc(20, 1)], &mut ws);
        assert!(
            ws[0].agent_link.is_none(),
            "generic apps are not agent products"
        );
        let mut detached = workload("unknown", 30);
        detached.roots = vec![30];
        ws.push(detached);
        let mut p = proc(30, 2);
        p.puniq = 20;
        p.label = "com.example.service".into();
        associate(&[proc(10, 0), proc(20, 1), p], &mut ws);
        assert!(
            ws[2].agent_link.is_none(),
            "launchd label excludes launching links"
        );
    }

    #[test]
    fn missing_parent_cycles_and_mixed_roots_do_not_invent_links() {
        let mut ws = vec![workload("agent-session", 10), workload("detached", 20)];
        let mut ps = vec![proc(10, 0), proc(20, 1), proc(30, 1)];
        ps[1].puniq = 10;
        ws[1].roots.push(30);
        ws[1].pids.push(30);
        associate(&ps, &mut ws);
        assert!(ws[1].agent_link.is_none());
        ps[2].puniq = 20;
        ps[1].puniq = 30;
        associate(&ps, &mut ws);
        assert!(ws[1].agent_link.is_none());
        ps[1].puniq = 999;
        associate(&ps, &mut ws);
        assert!(ws[1].agent_link.is_none());
    }
}
