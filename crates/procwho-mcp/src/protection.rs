//! Instance-bound dependencies of this stdio connection, independent of identity certainty.
use procwho_proc::Snapshot;
use serde_json::{json, Value};
use std::collections::HashSet;

struct Node {
    pid: i32,
    ppid: i32,
    instance_id: u64,
    start: f64,
    workload_key: String,
    kind: String,
    root: bool,
}

fn protect(nodes: &[Node], current: i32) -> Vec<Value> {
    let mut ancestors = HashSet::new();
    let mut pid = current;
    while pid > 1 && ancestors.insert(pid) {
        let Some(node) = nodes.iter().find(|n| n.pid == pid) else {
            break;
        };
        pid = node.ppid;
    }
    let scopes: HashSet<_> = nodes
        .iter()
        .filter(|n| {
            ancestors.contains(&n.pid)
                && ["agent-session", "agent-toolset", "agent-command"].contains(&n.kind.as_str())
        })
        .map(|n| n.workload_key.as_str())
        .collect();
    let mut descendants = HashSet::from([current]);
    loop {
        let before = descendants.len();
        for node in nodes {
            if node.pid > 1 && descendants.contains(&node.ppid) {
                descendants.insert(node.pid);
            }
        }
        if descendants.len() == before {
            break;
        }
    }
    nodes.iter().filter(|n| n.pid > 1 && (ancestors.contains(&n.pid) || descendants.contains(&n.pid) || scopes.contains(n.workload_key.as_str())))
        .map(|n| {
            let relationship = if n.pid == current { "mcp-server" }
                else if n.kind == "agent-host" && ancestors.contains(&n.pid) { "shared-host" }
                else if n.kind == "agent-session" && n.root { "session-root" }
                else if ancestors.contains(&n.pid) { "ancestor" }
                else { "tool-helper" };
            json!({"pid":n.pid,"instance_id":n.instance_id,"start_unix":n.start,
                "workload_key":n.workload_key,"relationship":relationship,"shared_host":n.kind == "agent-host"})
        }).collect()
}

pub(crate) fn dependencies(snap: &Snapshot, current: i32) -> Vec<Value> {
    let nodes: Vec<_> = snap
        .procs
        .iter()
        .map(|p| {
            let w = snap.workload_of(p);
            Node {
                pid: p.pid,
                ppid: p.ppid,
                instance_id: p.uniq,
                start: p.start,
                workload_key: w.key.clone(),
                kind: w.kind.clone(),
                root: w.roots.contains(&p.pid),
            }
        })
        .collect();
    protect(&nodes, current)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(pid: i32, parent: i32, key: &str, kind: &str, root: bool) -> Node {
        Node {
            pid,
            ppid: parent,
            instance_id: pid as u64 + 1000,
            start: 123.0,
            workload_key: key.into(),
            kind: kind.into(),
            root,
        }
    }
    #[test]
    fn protects_connection_session_helpers_and_shared_host_without_other_sessions() {
        let nodes = vec![
            node(1, 0, "system", "system", true),
            node(2, 1, "host", "agent-host", true),
            node(3, 2, "session", "agent-session", true),
            node(4, 3, "session", "agent-session", false),
            node(5, 3, "session", "agent-session", false),
            node(6, 2, "other", "agent-session", true),
            node(7, 4, "mcp", "unknown", true),
            node(8, 7, "tool", "unknown", true),
        ];
        let protected = protect(&nodes, 7);
        assert_eq!(
            protected
                .iter()
                .map(|p| p["pid"].as_i64().unwrap())
                .collect::<Vec<_>>(),
            vec![2, 3, 4, 5, 7, 8]
        );
        assert_eq!(protected[0]["relationship"], "shared-host");
        assert_eq!(protected[0]["shared_host"], true);
        assert_eq!(protected[1]["relationship"], "session-root");
        assert_eq!(protected[4]["instance_id"], 1007);
    }
    #[test]
    fn ssh_dependencies_keep_physical_ancestors_and_correct_workload_keys() {
        let nodes = vec![
            node(10, 1, "opaque:root", "system-opaque", true),
            node(11, 10, "terminal:11:1011", "terminal", true),
            node(12, 11, "session:12:1012", "agent-session", true),
            node(13, 12, "session:12:1012", "agent-session", false),
        ];
        let protected = protect(&nodes, 13);
        assert_eq!(protected.len(), 4);
        assert_eq!(protected[0]["workload_key"], "opaque:root");
        assert_eq!(protected[1]["workload_key"], "terminal:11:1011");
        assert_eq!(protected[3]["workload_key"], "session:12:1012");
    }

    #[test]
    fn unknown_identity_still_protects_ancestors_and_survives_cycles() {
        let nodes = vec![
            node(2, 3, "a", "unknown", true),
            node(3, 2, "b", "unknown", true),
            node(4, 2, "c", "unknown", true),
        ];
        let protected = protect(&nodes, 4);
        assert_eq!(protected.len(), 3);
        assert!(protected.iter().all(|p| p["instance_id"].is_u64()));
    }
}
