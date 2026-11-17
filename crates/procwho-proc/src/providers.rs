//! Opt-in, bounded launchd label providers. Commands run directly in a read-only sandbox.

use crate::config::{read_bounded, safe, LabelProvider};
use crate::model::Config;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

const MAX_BYTES: usize = 256 * 1024;
const TIMEOUT_SECONDS: u64 = 2;
const SANDBOX: &str = "(version 1)(allow default)(deny file-write*)(deny network*)";

/// The provider contract. Unknown row keys are discarded before caching.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ServiceLabel {
    pub label: String,
    pub name: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default, skip_deserializing)]
    pub provider: String,
}

fn parse(bytes: &[u8], provider: &str) -> Option<Vec<ServiceLabel>> {
    let mut rows: Vec<ServiceLabel> = serde_json::from_slice(bytes).ok()?;
    if rows.len() > 1024 {
        return None;
    }
    for row in &mut rows {
        row.label = safe(&row.label);
        row.name = safe(&row.name);
        row.purpose = safe(&row.purpose);
        row.provider = safe(provider);
    }
    rows.retain(|r| !r.label.is_empty() && !r.name.is_empty());
    Some(rows)
}

// Stable FNV-1a fingerprint of the full configuration. No command arguments are stored in
// the filename or cache, and a changed command cannot reuse an old command's labels.
fn cache_name(p: &LabelProvider) -> String {
    let encoded = serde_json::to_vec(p).unwrap_or_default();
    let hash = encoded.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    });
    format!("label-provider-{hash:016x}.json")
}

fn save(path: &Path, rows: &[ServiceLabel]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("missing state directory"))?;
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;
    let tmp = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        crate::sys::now().to_bits()
    ));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    let result = (|| {
        let bytes = serde_json::to_vec(rows)?;
        if bytes.len() > MAX_BYTES {
            return Err(std::io::Error::other("cache exceeds size limit"));
        }
        f.write_all(&bytes)?;
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn load_one(cfg: &Config, p: &LabelProvider) -> (Vec<ServiceLabel>, String) {
    let cache = cfg.state.join(cache_name(p));
    let age = std::fs::metadata(&cache)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .map(|d| d.as_secs());
    let cached = read_bounded(&cache, MAX_BYTES as u64)
        .ok()
        .and_then(|b| parse(&b, &p.name));
    let prefix = format!("label provider '{}'", safe(&p.name));
    if let (Some(age), Some(rows)) = (age, cached.as_ref()) {
        if age < p.ttl_seconds {
            return (rows.clone(), format!("{prefix}: cached {age}s old"));
        }
    }
    let mut command = vec!["/usr/bin/sandbox-exec", "-p", SANDBOX];
    command.extend(p.command.iter().map(String::as_str));
    let fresh = crate::sys::run_bounded(&command, TIMEOUT_SECONDS, MAX_BYTES)
        .and_then(|s| parse(s.as_bytes(), &p.name));
    match fresh {
        Some(rows) => {
            let note = if save(&cache, &rows).is_err() {
                "; cache not saved"
            } else {
                ""
            };
            (rows, format!("{prefix}: fresh{note}"))
        }
        None => match (cached, age) {
            (Some(rows), Some(age)) => (
                rows,
                format!("{prefix}: unavailable or invalid output; STALE cache {age}s old"),
            ),
            _ => (
                vec![],
                format!(
                    "{prefix}: unavailable, timed out, oversized or invalid JSON; labels unknown"
                ),
            ),
        },
    }
}

/// Run only configured providers, in parallel. First configured provider wins duplicate labels.
pub fn collect(
    cfg: &Config,
    providers: &[LabelProvider],
) -> (HashMap<String, ServiceLabel>, String) {
    if providers.is_empty() {
        return (HashMap::new(), "no label providers configured".into());
    }
    // A bounded worker pool prevents a large configuration from creating unbounded threads.
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(vec![None; providers.len()]);
    std::thread::scope(|scope| {
        let next = &next;
        let results = &results;
        let workers: Vec<_> = (0..providers.len().min(4))
            .map(|_| {
                scope.spawn(move || loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let Some(p) = providers.get(i) else { break };
                    results.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(load_one(cfg, p));
                })
            })
            .collect();
        for w in workers {
            let _ = w.join();
        }
    });
    let results = results.into_inner().unwrap_or_else(|e| e.into_inner());
    let mut labels = HashMap::new();
    let mut notes = Vec::new();
    for result in results.into_iter().flatten() {
        notes.push(result.1);
        for row in result.0 {
            labels.entry(row.label.clone()).or_insert(row);
        }
    }
    (labels, notes.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_fields_are_redacted_filtered_and_allowlisted() {
        let rows = parse(br#"[{"label":"com.example.job","name":"Demo\u001b\n","purpose":"--api-\u0000key hunter2","unknown":"private"}]"#, "labels\n").unwrap();
        assert_eq!(rows[0].name, "Demo");
        assert!(!rows[0].purpose.contains("hunter2"));
        assert_eq!(rows[0].provider, "labels");
        assert!(!serde_json::to_string(&rows).unwrap().contains("private"));
        assert!(parse(b"{}", "labels").is_none());
    }

    #[test]
    fn caches_with_ttl_and_reports_stale_failure() {
        let dir = std::env::temp_dir().join(format!("procwho-provider-{}", std::process::id()));
        let cfg = Config {
            home: dir.display().to_string(),
            state: dir.clone(),
            registry: dir.join("none"),
        };
        let p = LabelProvider {
            name: "test".into(),
            command: vec![
                "/bin/echo".into(),
                r#"[{"label":"com.example.job","name":"Demo","purpose":"test"}]"#.into(),
            ],
            ttl_seconds: 600,
        };
        let (labels, note) = collect(&cfg, std::slice::from_ref(&p));
        assert_eq!(labels["com.example.job"].name, "Demo");
        assert!(note.contains("fresh"), "{note}");
        let (_, note) = collect(&cfg, std::slice::from_ref(&p));
        assert!(note.contains("cached"), "{note}");
        let bad = LabelProvider {
            command: vec!["/missing/procwho-provider".into()],
            ttl_seconds: 0,
            ..p
        };
        let cache = cfg.state.join(cache_name(&bad));
        save(
            &cache,
            &[ServiceLabel {
                label: "com.example.old".into(),
                name: "Old".into(),
                ..Default::default()
            }],
        )
        .unwrap();
        let (labels, note) = collect(&cfg, &[bad]);
        assert!(note.contains("STALE"), "{note}");
        assert_eq!(labels.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn empty_config_has_no_side_effects_and_changed_commands_have_distinct_cache_keys() {
        let cfg = Config {
            home: "/".into(),
            state: Path::new("/nonexistent/procwho-test-state").into(),
            registry: Path::new("/none").into(),
        };
        assert!(collect(&cfg, &[]).0.is_empty());
        assert!(!cfg.state.exists());
        let mut p = LabelProvider {
            name: "a".into(),
            command: vec!["/bin/echo".into()],
            ttl_seconds: 600,
        };
        let name = cache_name(&p);
        p.command.push("[]".into());
        assert_ne!(cache_name(&p), name);
    }
}
