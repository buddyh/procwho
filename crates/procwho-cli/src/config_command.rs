//! Configuration inspection and worktree defaults, separate from CLI dispatch.

use procwho_proc::config::{expand_home, LoadedConfig};
use std::path::{Path, PathBuf};

pub const HELP: &str = "procwho config [--example]\n\nPrint resolved configuration and value sources as JSON. --example prints a sample file.\nPROCWHO_CONFIG overrides ~/.config/procwho/config.json. No commands run.\n";

pub fn run(rest: &[String]) -> Result<String, String> {
    let started = std::time::Instant::now();
    let observed = procwho_proc::sys::now() as i64;
    let args = super::Args::parse(rest, &[], &["--example"])?;
    args.no_positionals()?;
    let value = if args.has("--example") {
        procwho_proc::config::example()
    } else {
        let home = procwho_wt::discover::home();
        let loaded = LoadedConfig::from_env(&home);
        let defaults = worktree_defaults(&loaded);
        serde_json::json!({
            "config_file": loaded.path,
            "values": {
                "worktree_roots": defaults.roots.iter().map(|(p, _)| p).collect::<Vec<_>>(),
                "stale_days": defaults.stale_days,
                "label_providers": loaded.values.label_providers,
                "extra_agents": loaded.values.extra_agents
            },
            "sources": loaded.sources,
            "notes": loaded.notes,
            "worktree_scan": {"root_depths": defaults.roots.iter().map(|(p, d)| serde_json::json!({"path":p,"depth":d})).collect::<Vec<_>>(), "containers": defaults.containers.iter().map(|(p,d)| serde_json::json!({"path":p,"depth":d})).collect::<Vec<_>>()},
            "built_in_agents": procwho_proc::agents::builtins()
        })
    };
    // A configured command can contain credentials. JSON-encode only after redacting strings.
    let mut value = value;
    value["schema_version"] = serde_json::json!(2);
    value["observed_at_unix"] = serde_json::json!(observed);
    value["collect_ms"] = serde_json::json!(started.elapsed().as_millis() as u64);
    redact_values(&mut value);
    serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
}

fn redact_values(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => {
            *s = procwho_proc::redact::redact(s)
                .chars()
                .filter(|c| !c.is_control())
                .collect()
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(redact_values),
        serde_json::Value::Object(o) => {
            // Redact argv as a whole so `--api-key`, `VALUE` is treated as one unit.
            if let Some(serde_json::Value::Array(command)) = o.get_mut("command") {
                let joined = command
                    .iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                if procwho_proc::redact::redact(&joined) != joined {
                    for arg in command.iter_mut().skip(1) {
                        *arg = serde_json::Value::String("[redacted]".into());
                    }
                }
            }
            o.values_mut().for_each(redact_values);
        }
        _ => {}
    }
}

pub fn worktree_defaults(config: &LoadedConfig) -> procwho_wt::Options {
    let mut defaults = procwho_wt::Options::default();
    if let Some(roots) = &config.values.worktree_roots {
        defaults.roots = roots.iter().map(|p| (p.clone(), 3)).collect();
    }
    if let Some(days) = config.values.stale_days {
        defaults.stale_days = days;
    }
    defaults
}

pub fn cli_path(path: &str) -> PathBuf {
    expand_home(Path::new(path), &procwho_wt::discover::home())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_argv_redacts_flag_values_before_output() {
        let mut v = serde_json::json!({"command":["labels", "--api-key", "hunter2"]});
        redact_values(&mut v);
        assert!(!v.to_string().contains("hunter2"));
    }

    #[test]
    fn file_roots_and_stale_days_override_defaults() {
        let loaded = LoadedConfig {
            path: PathBuf::from("/config"),
            values: procwho_proc::config::Settings {
                worktree_roots: Some(vec![PathBuf::from("/Users/user/code with spaces")]),
                stale_days: Some(23),
                ..Default::default()
            },
            sources: Default::default(),
            notes: vec![],
        };
        let opts = worktree_defaults(&loaded);
        assert_eq!(
            opts.roots,
            vec![(PathBuf::from("/Users/user/code with spaces"), 3)]
        );
        assert_eq!(opts.stale_days, 23);
    }
}
