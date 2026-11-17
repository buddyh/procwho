//! Optional, bounded JSON configuration. Loading configuration never executes commands.

use crate::agents::AgentRule;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

pub(crate) const MAX_CONFIG_BYTES: u64 = 256 * 1024;

/// A user-selected, read-only source of launchd display labels.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LabelProvider {
    pub name: String,
    pub command: Vec<String>,
    #[serde(default = "default_ttl")]
    pub ttl_seconds: u64,
}

fn default_ttl() -> u64 {
    600
}

/// File keys are optional; an explicit empty roots list disables default discovery.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Settings {
    pub worktree_roots: Option<Vec<PathBuf>>,
    pub stale_days: Option<u32>,
    #[serde(default)]
    pub label_providers: Vec<LabelProvider>,
    #[serde(default)]
    pub extra_agents: Vec<AgentRule>,
}

/// Resolved file values, their origin, and nonfatal configuration diagnostics.
#[derive(Debug, Clone, Serialize)]
pub struct LoadedConfig {
    pub path: PathBuf,
    pub values: Settings,
    pub sources: BTreeMap<String, String>,
    pub notes: Vec<String>,
}

/// Expand a leading `~/` without shell parsing, preserving spaces and other literal bytes.
pub fn expand_home(path: &Path, home: &Path) -> PathBuf {
    match path.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => path.to_path_buf(),
    }
}

/// Read a regular file without following a FIFO or accepting an unbounded allocation.
pub(crate) fn read_bounded(path: &Path, max: u64) -> std::io::Result<Vec<u8>> {
    use std::os::unix::fs::OpenOptionsExt;
    let f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    if !f.metadata()?.is_file() {
        return Err(std::io::Error::other("expected a regular file"));
    }
    let mut bytes = Vec::new();
    f.take(max + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > max {
        return Err(std::io::Error::other("file exceeds size limit"));
    }
    Ok(bytes)
}

impl LoadedConfig {
    /// `PROCWHO_CONFIG` overrides `~/.config/procwho/config.json`.
    pub fn from_env(home: &Path) -> Self {
        let path = std::env::var_os("PROCWHO_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config/procwho/config.json"));
        Self::load(&expand_home(&path, home), home)
    }

    /// Resolve a file. Invalid fields fall back independently; diagnostics contain no values.
    pub fn load(path: &Path, home: &Path) -> Self {
        let mut out = Self {
            path: path.to_path_buf(),
            values: Settings::default(),
            sources: BTreeMap::new(),
            notes: vec![],
        };
        for key in [
            "worktree_roots",
            "stale_days",
            "label_providers",
            "extra_agents",
        ] {
            out.sources.insert(key.into(), "default".into());
        }
        let bytes = match read_bounded(path, MAX_CONFIG_BYTES) {
            Ok(b) => b,
            Err(e) => {
                if e.kind() != std::io::ErrorKind::NotFound {
                    out.notes.push(format!("config unavailable ({})", e.kind()));
                }
                return out;
            }
        };
        let Some(obj) = serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .and_then(|v| v.as_object().cloned())
        else {
            out.notes
                .push("config must be a JSON object; using defaults".into());
            return out;
        };
        for (key, value) in obj {
            if !out.sources.contains_key(&key) {
                out.notes
                    .push(format!("config: unknown key '{}' ignored", safe(&key)));
                continue;
            }
            let allowed: &[&str] = match key.as_str() {
                "label_providers" => &["name", "command", "ttl_seconds"],
                "extra_agents" => &[
                    "name",
                    "executables",
                    "executable_patterns",
                    "excluded_executable_patterns",
                    "argv_patterns",
                    "session_identity",
                    "shared_host_subcommands",
                    "role",
                    "required_argv",
                    "ancestor_patterns",
                    "require_agent_descendant",
                    "host_commands",
                    "host_flags",
                    "headless_flags",
                    "headless_commands",
                    "excluded_commands",
                    "excluded_flags",
                    "value_flags",
                    "session_flags",
                    "session_commands",
                    "session_id_pattern",
                ],
                _ => &[],
            };
            if let Some(rows) = value.as_array().filter(|_| !allowed.is_empty()) {
                for (i, row) in rows.iter().enumerate() {
                    if let Some(obj) = row.as_object() {
                        for unknown in obj.keys().filter(|name| !allowed.contains(&name.as_str())) {
                            out.notes.push(format!(
                                "config: unknown key '{key}[{i}].{}' ignored",
                                safe(unknown)
                            ));
                        }
                    }
                }
            }
            // Deserialize one field at a time so one bad value cannot discard good values.
            let single = serde_json::json!({key.clone(): value});
            let Ok(mut settings) = serde_json::from_value::<Settings>(single) else {
                out.notes
                    .push(format!("config: invalid {key}; using default"));
                continue;
            };
            let valid = match key.as_str() {
                "worktree_roots" => settings
                    .worktree_roots
                    .as_ref()
                    .is_some_and(|roots| roots.iter().all(|p| !p.as_os_str().is_empty())),
                "stale_days" => settings.stale_days.is_some(),
                "label_providers" => settings.label_providers.iter().all(|p| {
                    !p.name.trim().is_empty()
                        && !p.command.is_empty()
                        && !p.command[0].is_empty()
                        && p.command.iter().all(|s| !s.contains('\0'))
                }),
                "extra_agents" => settings.extra_agents.iter().all(AgentRule::valid_extra),
                _ => unreachable!(),
            };
            if !valid {
                out.notes
                    .push(format!("config: invalid {key}; using default"));
                continue;
            }
            match key.as_str() {
                "worktree_roots" => {
                    let roots = settings.worktree_roots.take().unwrap_or_default();
                    out.values.worktree_roots =
                        Some(roots.iter().map(|p| expand_home(p, home)).collect());
                }
                "stale_days" => out.values.stale_days = settings.stale_days,
                "label_providers" => out.values.label_providers = settings.label_providers,
                "extra_agents" => out.values.extra_agents = settings.extra_agents,
                _ => unreachable!(),
            }
            out.sources.insert(
                key,
                format!("config file {}", safe(&path.display().to_string())),
            );
        }
        out
    }
}

pub(crate) fn safe(s: &str) -> String {
    let clean: String = s.chars().filter(|c| !c.is_control()).take(4096).collect();
    crate::redact::redact(&clean)
}

/// A complete example printed by `procwho config --example`; never written automatically.
pub fn example() -> serde_json::Value {
    serde_json::json!({
        "worktree_roots": ["~/code", "~/Projects"],
        "stale_days": 14,
        "label_providers": [{"name": "local labels", "command": ["/Users/user/bin/service-labels"], "ttl_seconds": 600}],
        "extra_agents": [{"name": "Example Agent", "executable_patterns": ["/example-agent/bin/agent$"], "required_argv": ["--acp"], "role": "host", "host_commands": [], "session_flags": ["--session-id"], "session_id_pattern": "^session-[0-9]+$"}]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(contents: &str) -> LoadedConfig {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let sequence = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("procwho-config-{}-{sequence}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.json");
        std::fs::write(&path, contents).unwrap();
        let loaded = LoadedConfig::load(&path, Path::new("/Users/user with spaces"));
        std::fs::remove_dir_all(dir).unwrap();
        loaded
    }

    #[test]
    fn optional_unknown_and_invalid_keys() {
        let c = parse(r#"{"stale_days":14,"future":true,"worktree_roots":"bad"}"#);
        assert_eq!(c.values.stale_days, Some(14));
        assert!(c.values.worktree_roots.is_none());
        assert_eq!(c.notes.len(), 2);
        assert_eq!(parse("{}").values.label_providers.len(), 0);
        assert_eq!(parse("[]").notes.len(), 1);
        assert!(parse(r#"{"stale_days":null}"#).values.stale_days.is_none());
    }

    #[test]
    fn literal_roots_and_provider_defaults() {
        let c = parse(
            r#"{"worktree_roots":["~/code with spaces","/tmp/literal;$x"],"label_providers":[{"name":"labels","command":["/bin/echo","[]"]}]}"#,
        );
        assert_eq!(
            c.values.worktree_roots.unwrap(),
            vec![
                PathBuf::from("/Users/user with spaces/code with spaces"),
                PathBuf::from("/tmp/literal;$x")
            ]
        );
        assert_eq!(c.values.label_providers[0].ttl_seconds, 600);
        assert!(parse(r#"{"worktree_roots":[]}"#)
            .values
            .worktree_roots
            .unwrap()
            .is_empty());
    }

    #[test]
    fn unknown_nested_rule_keys_are_not_silently_accepted() {
        let c = parse(
            r#"{"label_providers":[{"name":"labels","command":["/bin/echo","[]"],"ttl_seonds":5}],"extra_agents":[{"name":"X","executables":["x"],"future":true}]}"#,
        );
        assert_eq!(c.values.label_providers[0].ttl_seconds, 600);
        assert_eq!(c.values.extra_agents.len(), 1);
        assert_eq!(c.notes.len(), 2);
        assert!(c.notes.iter().any(|s| s.contains("ttl_seonds")));
    }

    #[test]
    fn example_is_valid_and_extra_identity_cannot_claim_verified_parsing() {
        assert!(parse(&example().to_string()).notes.is_empty());
        let c = parse(
            r#"{"extra_agents":[{"name":"X","executables":["x"],"session_identity":"claude_codex"}]}"#,
        );
        assert!(c.values.extra_agents.is_empty());
        assert_eq!(c.notes.len(), 1);
    }
}
