//! Generate a copyable owner rule without writing configuration or copying prompts.
use crate::model::{Proc, Workload};
use regex::Regex;
use serde_json::{json, Value};

fn safe_fragment(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 4096
        && !s.chars().any(char::is_control)
        && crate::redact::redact(s) == s
        && ![
            "redacted",
            "secret",
            "token",
            "password",
            "api_key",
            "api-key",
            "bearer",
            "credential",
        ]
        .iter()
        .any(|word| s.to_ascii_lowercase().contains(word))
        && !s.split(|c: char| !c.is_ascii_alphanumeric()).any(|word| {
            word.len() >= 32 || (word.len() >= 24 && word.chars().any(|c| c.is_ascii_digit()))
        })
}

fn matcher(p: &Proc, project: Option<&str>) -> Option<(&'static str, String)> {
    // These registrations contain launch-specific identifiers, not stable service names.
    if safe_fragment(&p.label)
        && ![
            "application.",
            "UIKitApplication:",
            "com.apple.xpc.launchd.unmanaged.",
        ]
        .iter()
        .any(|prefix| p.label.starts_with(prefix))
    {
        return Some(("label", format!("^{}$", regex::escape(&p.label))));
    }
    if safe_fragment(&p.exe)
        && (p.exe.contains(".app/Contents/")
            || p.exe.contains("/node_modules/")
            || p.exe.contains("/site-packages/"))
    {
        return Some(("exe", format!("^{}$", regex::escape(&p.exe))));
    }
    let cwd = project.filter(|s| safe_fragment(s)).unwrap_or(&p.cwd);
    if cwd.starts_with('/')
        && cwd != "/"
        && safe_fragment(cwd)
        && !matches!(
            cwd,
            "/tmp" | "/private/tmp" | "/var" | "/usr" | "/Applications"
        )
        && !cwd.starts_with("/System/")
        && !cwd.starts_with("/private/tmp/")
        && !cwd.starts_with("/var/folders/")
        && !(cwd.starts_with("/Users/") && cwd.split('/').filter(|s| !s.is_empty()).count() <= 2)
        && !cwd.ends_with("/Contents/MacOS")
    {
        return Some((
            "cwd",
            format!("^{}(?:/|$)", regex::escape(cwd.trim_end_matches('/'))),
        ));
    }
    // Only argv[0]. Never infer subcommands from free text, prompts, flag values or redactions.
    let first = p.argv.first()?;
    if !safe_fragment(first)
        || first.starts_with('-')
        || first.contains('=')
        || first.chars().any(char::is_whitespace)
    {
        return None;
    }
    Some(("argv", format!("^{}(?: |$)", regex::escape(first))))
}

/// Match using exactly the same raw fields as the registry loader.
pub fn matches_rule(field: &str, pattern: &str, p: &Proc) -> bool {
    let value = match field {
        "label" => &p.label,
        "exe" => &p.exe,
        "cwd" => &p.cwd,
        "argv" => &p.raw,
        _ => return false,
    };
    Regex::new(pattern).is_ok_and(|rx| rx.is_match(value))
}

/// Generate a valid registry rule object, including ignored, removable count metadata.
/// The count covers current processes, not workloads, and includes this rule's descendants
/// only when their own fields match. Registry precedence can change actual attribution.
pub fn suggested_rule(workload: &Workload, procs: &[Proc]) -> Value {
    let mut candidates: Vec<_> = procs
        .iter()
        .filter(|p| workload.pids.contains(&p.pid))
        .filter_map(|p| matcher(p, workload.project.as_ref().map(|p| p.root.as_str())))
        .collect();
    let priority = |field: &str| match field {
        "label" => 0,
        "exe" => 1,
        "cwd" => 2,
        _ => 3,
    };
    candidates.sort_by(|a, b| {
        priority(a.0)
            .cmp(&priority(b.0))
            .then_with(|| b.1.len().cmp(&a.1.len()))
            .then_with(|| a.1.cmp(&b.1))
    });
    let (field, pattern) = candidates
        .into_iter()
        .next()
        .unwrap_or(("argv", r"^\b\B$".into()));
    let rx = Regex::new(&pattern).expect("generated literal matcher");
    let count = procs
        .iter()
        .filter(|p| {
            let value = match field {
                "label" => &p.label,
                "exe" => &p.exe,
                "cwd" => &p.cwd,
                _ => &p.raw,
            };
            rx.is_match(value)
        })
        .count();
    let mut rule = json!({"name": "...", field: pattern, "_matches_now": count});
    if count == 0 {
        rule["_note"] = json!("No stable readable matcher was found. This rule matches nothing; edit the matcher before use.");
    }
    for (key, value) in [("owner", &workload.owner), ("impact", &workload.impact)] {
        if let Some(value) = value.as_deref().filter(|v| safe_fragment(v)) {
            rule[key] = json!(value);
        }
    }
    rule
}

#[cfg(test)]
mod tests {
    use super::*;
    fn proc() -> Proc {
        Proc {
            pid: 10,
            ppid: 1,
            uid: 501,
            pgid: 10,
            uniq: 10,
            puniq: 1,
            comm: "tool".into(),
            exe: "/usr/bin/tool".into(),
            exe_gone: false,
            readable: true,
            argv: vec!["tool".into()],
            raw: "tool --token secret".into(),
            env: Default::default(),
            cmd: "tool --token [redacted]".into(),
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
    fn choice_order_and_actual_match_count() {
        let mut p = proc();
        p.label = "com.example.preview".into();
        p.exe = "/Applications/Example.app/Contents/MacOS/preview".into();
        p.cwd = "/Users/user/code/demo".into();
        assert_eq!(matcher(&p, None).unwrap().0, "label");
        p.label = "application.com.example.preview.123456.789012".into();
        assert_eq!(
            matcher(&p, None).unwrap().0,
            "exe",
            "transient app labels are not stable matchers"
        );
        p.label.clear();
        assert_eq!(matcher(&p, None).unwrap().0, "exe");
        p.exe = "/usr/bin/node".into();
        assert_eq!(matcher(&p, None).unwrap().0, "cwd");
        p.cwd.clear();
        assert_eq!(matcher(&p, None).unwrap(), ("argv", "^tool(?: |$)".into()));
        let w = Workload {
            pids: vec![10],
            ..Default::default()
        };
        let rule = suggested_rule(&w, &[p.clone(), p]);
        assert_eq!(rule["_matches_now"], 2);
        let parsed = crate::collect::parse_registry(&json!({"rules":[rule]}).to_string()).unwrap();
        assert_eq!(parsed.len(), 1);
    }
    #[test]
    fn escaped_anchored_paths_have_directory_boundaries() {
        let mut p = proc();
        p.cwd = "/Users/user/code/a.b+[demo]".into();
        let (field, rx) = matcher(&p, None).unwrap();
        assert!(matches_rule(field, &rx, &p));
        p.cwd.push_str("/src");
        assert!(matches_rule(field, &rx, &p));
        p.cwd = "/Users/user/code/a.b+[demo]-other".into();
        assert!(!matches_rule(field, &rx, &p));
    }
    #[test]
    fn secrets_and_redacted_fragments_never_enter_patterns() {
        for token in [
            "sk-abcdefghijklmnopqrstu",
            "[REDACTED]",
            "secret-tool",
            "password=abc",
            "tool\nunsafe",
            "tool-with-1234567890123456789012345678",
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
        ] {
            let mut p = proc();
            p.argv = vec![token.into()];
            p.raw = token.into();
            let rule = suggested_rule(
                &Workload {
                    pids: vec![10],
                    ..Default::default()
                },
                &[p],
            );
            assert_eq!(rule["_matches_now"], 0);
            assert!(!rule.to_string().contains(token));
        }
        let mut home = proc();
        home.cwd = "/Users/user".into();
        assert_eq!(matcher(&home, None).unwrap().0, "argv");
        let p = proc();
        let rule = suggested_rule(
            &Workload {
                pids: vec![10],
                ..Default::default()
            },
            &[p],
        );
        assert_eq!(rule["argv"], "^tool(?: |$)");
        assert!(!rule.to_string().contains("secret"));
    }
}
