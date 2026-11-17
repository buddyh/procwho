//! Agent recognition in one table. Name matches are evidence, never proof of session identity.

use crate::model::Proc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// Legacy policy marker retained for configuration compatibility and Codex tool grouping.
/// New selectors use the same explicit, validated fields for built-ins and custom rules.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionIdentity {
    #[default]
    None,
    ClaudeCodex,
}

/// A recognition rule. Regexes apply to executable paths or the launcher script, not prompts.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentRule {
    pub name: String,
    /// App identities for association only; these do not recognize provider sessions.
    #[serde(default)]
    pub app_bundles: Vec<String>,
    #[serde(default)]
    pub app_is_agent_product: bool,
    #[serde(default)]
    pub executables: Vec<String>,
    #[serde(default)]
    pub executable_patterns: Vec<String>,
    #[serde(default)]
    pub excluded_executable_patterns: Vec<String>,
    #[serde(default)]
    pub argv_patterns: Vec<String>,
    #[serde(default)]
    pub session_identity: SessionIdentity,
    #[serde(default)]
    pub shared_host_subcommands: Vec<String>,
    /// An unconditional role for a proven service, rather than a session binary.
    #[serde(default)]
    pub role: AgentRole,
    /// Exact parsed flags/command words, all required (never searched in prompts).
    #[serde(default)]
    pub required_argv: Vec<String>,
    /// At least one ancestor executable must match when this list is nonempty.
    #[serde(default)]
    pub ancestor_patterns: Vec<String>,
    /// Editor infrastructure is a host only with a separately recognized provider descendant.
    #[serde(default)]
    pub require_agent_descendant: bool,
    #[serde(default)]
    pub host_commands: Vec<Vec<String>>,
    /// Alternatives of flag sets; every flag in one set is required.
    #[serde(default)]
    pub host_flags: Vec<Vec<String>>,
    #[serde(default)]
    pub headless_flags: Vec<String>,
    #[serde(default)]
    pub headless_commands: Vec<Vec<String>>,
    #[serde(default)]
    pub excluded_commands: Vec<Vec<String>>,
    #[serde(default)]
    pub excluded_flags: Vec<String>,
    /// Additional options that consume a value, for conservative argv parsing.
    #[serde(default)]
    pub value_flags: Vec<String>,
    #[serde(default)]
    pub session_flags: Vec<String>,
    /// Command prefixes immediately followed by an explicit ID (e.g. exec resume ID).
    #[serde(default)]
    pub session_commands: Vec<Vec<String>>,
    /// Whole-ID validation. Paths, picker requests and partial IDs are not exact identity.
    #[serde(default)]
    pub session_id_pattern: Option<String>,
}

/// A process role; hosts are never assigned a provider session from their environment.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRole {
    #[default]
    Session,
    Host,
    Toolset,
}

impl AgentRule {
    pub(crate) fn valid_extra(&self) -> bool {
        !self.name.trim().is_empty()
            && self.session_identity == SessionIdentity::None
            && (!self.executables.is_empty()
                || !self.executable_patterns.is_empty()
                || !self.argv_patterns.is_empty())
            && self
                .executables
                .iter()
                .all(|s| !s.is_empty() && !s.contains('/') && !s.chars().any(char::is_control))
            && self
                .executable_patterns
                .iter()
                .chain(&self.excluded_executable_patterns)
                .chain(&self.argv_patterns)
                .chain(&self.ancestor_patterns)
                .chain(self.session_id_pattern.iter())
                .all(|s| Regex::new(s).is_ok())
            && (self.session_flags.is_empty() && self.session_commands.is_empty()
                || self.session_id_pattern.is_some())
            && self
                .host_commands
                .iter()
                .chain(&self.excluded_commands)
                .chain(&self.session_commands)
                .chain(&self.host_flags)
                .chain(&self.headless_commands)
                .all(|seq| !seq.is_empty() && seq.iter().all(|s| !s.is_empty()))
    }
}

/// The built-in table; official executable references are retained in docs/ARCHITECTURE.md.
pub fn builtins() -> Vec<AgentRule> {
    let mut table: Vec<AgentRule> =
        serde_json::from_str(include_str!("agents/providers.json")).expect("built-in agent table");
    table.extend(
        serde_json::from_str::<Vec<AgentRule>>(include_str!("agents/cursor_google_grok.json"))
            .expect("built-in agent table"),
    );
    table.extend(
        serde_json::from_str::<Vec<AgentRule>>(include_str!("agents/terminal.json"))
            .expect("built-in agent table"),
    );
    table.extend(
        serde_json::from_str::<Vec<AgentRule>>(include_str!("agents/tooling.json"))
            .expect("built-in agent table"),
    );
    table.extend(
        serde_json::from_str::<Vec<AgentRule>>(include_str!("agents/additional.json"))
            .expect("built-in agent table"),
    );
    table.extend(
        serde_json::from_str::<Vec<AgentRule>>(include_str!("agents/packages.json"))
            .expect("built-in agent table"),
    );
    table.extend(
        serde_json::from_str::<Vec<AgentRule>>(include_str!("agents/hosts.json"))
            .expect("built-in agent table"),
    );
    for r in &mut table {
        r.excluded_flags
            .extend(["--help", "-h", "--version", "-V"].map(str::to_string));
        r.excluded_commands.extend(
            [
                "login",
                "logout",
                "auth",
                "update",
                "upgrade",
                "help",
                "version",
                "completion",
                "completions",
                "config",
                "mcp",
                "models",
                "sessions",
                "history",
                "dashboard",
                "hub",
                "attach",
            ]
            .map(|s| vec![s.into()]),
        );
    }
    table
}

/// Compiled once per configured rule, outside the per-process attribution loop.
#[derive(Debug)]
pub struct AgentMatcher {
    pub rule: AgentRule,
    executable: Vec<Regex>,
    excluded: Vec<Regex>,
    argv: Vec<Regex>,
    ancestors: Vec<Regex>,
    session_id: Option<Regex>,
}

/// Only command positions and known options are parsed. Unknown positional text terminates
/// parsing, so a prompt cannot contribute flags or subcommands. Unknown flags stop parsing
/// too: missing a mode is safer than treating its value as a command.
struct Arguments<'a> {
    commands: Vec<&'a str>,
    flags: Vec<(&'a str, Option<&'a str>)>,
}

impl Arguments<'_> {
    fn has(&self, wanted: &str) -> bool {
        match wanted.split_once('=') {
            Some((key, val)) => self
                .flags
                .iter()
                .any(|(flag, value)| *flag == key && *value == Some(val)),
            None => self
                .flags
                .iter()
                .any(|(flag, value)| *flag == wanted && *value != Some("false")),
        }
    }
    fn starts(&self, words: &[String]) -> bool {
        self.commands.len() >= words.len() && self.commands.iter().zip(words).all(|(a, b)| *a == b)
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn interpreter(path: &str) -> bool {
    let base = basename(path);
    matches!(base, "node" | "bun" | "Python" | "python")
        || base
            .strip_prefix("python")
            .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit() || b == b'.'))
}

/// Only verified launcher wrappers are deduplicated; Node agent runtimes own sessions.
pub(crate) fn is_launcher(p: &Proc) -> bool {
    script_index(p)
        .and_then(|i| p.argv.get(i))
        .is_some_and(|s| {
            [
                "/@openai/codex/bin/codex.js",
                "/node_modules/cline/bin/cline",
                "/@charmland/crush/run-crush.js",
            ]
            .iter()
            .any(|suffix| s.ends_with(suffix))
        })
}

fn script_index(p: &Proc) -> Option<usize> {
    if !interpreter(&p.exe) {
        return None;
    }
    // These interpreter switches consume values. Never scan arbitrary argv for a script.
    let mut i = 1;
    while let Some(arg) = p.argv.get(i) {
        if matches!(arg.as_str(), "--") {
            return (i + 1 < p.argv.len()).then_some(i + 1);
        }
        if matches!(
            arg.split('=').next().unwrap_or(arg),
            "-m" | "-c" | "-e" | "--eval" | "--print"
        ) {
            return None;
        }
        if matches!(arg.as_str(), "--require" | "-r" | "--import" | "--loader") {
            i += 2;
            continue;
        }
        if arg.starts_with('-') {
            if [
                "--max-old-space-size=",
                "--stack-size=",
                "--inspect=",
                "--inspect-brk=",
                "--title=",
                "--require=",
                "--import=",
                "--loader=",
            ]
            .iter()
            .any(|prefix| arg.starts_with(prefix))
                || matches!(
                    arg.as_str(),
                    "--no-warnings" | "--enable-source-maps" | "-u" | "-B" | "-I" | "-E" | "-s"
                )
            {
                i += 1;
                continue;
            }
            return None; // An unknown option may consume the following token.
        }
        return Some(i);
    }
    None
}

impl AgentMatcher {
    fn new(rule: AgentRule) -> Self {
        let compile =
            |patterns: &[String]| patterns.iter().filter_map(|s| Regex::new(s).ok()).collect();
        let executable = compile(&rule.executable_patterns);
        let excluded = compile(&rule.excluded_executable_patterns);
        let argv = compile(&rule.argv_patterns);
        let ancestors = compile(&rule.ancestor_patterns);
        let session_id = rule
            .session_id_pattern
            .as_ref()
            .and_then(|s| Regex::new(s).ok());
        Self {
            rule,
            executable,
            excluded,
            argv,
            ancestors,
            session_id,
        }
    }

    fn arguments<'a>(&self, p: &'a Proc) -> Arguments<'a> {
        let offset = script_index(p).map_or(1, |i| i + 1);
        let args = &p.argv[p.argv.len().min(offset)..];
        let mut out = Arguments {
            commands: vec![],
            flags: vec![],
        };
        let mut i = 0;
        while let Some(arg) = args.get(i) {
            if arg == "--" {
                break;
            }
            if arg.starts_with('-') {
                if let Some((flag, value)) = arg.split_once('=') {
                    out.flags.push((flag, Some(value)));
                } else if self
                    .rule
                    .value_flags
                    .iter()
                    .chain(&self.rule.session_flags)
                    .any(|f| f == arg)
                    || matches!(
                        arg.as_str(),
                        "--cwd"
                            | "--cd"
                            | "-C"
                            | "--model"
                            | "--output-format"
                            | "--input-format"
                            | "--config"
                            | "--session-dir"
                    )
                {
                    let required_value = self.rule.value_flags.iter().any(|f| f == arg);
                    let value = args
                        .get(i + 1)
                        .map(String::as_str)
                        .filter(|s| required_value || !s.starts_with('-'));
                    out.flags.push((arg, value));
                    if value.is_some() {
                        i += 1;
                    }
                } else {
                    out.flags.push((arg, None));
                    let known = matches!(
                        arg.as_str(),
                        "--help"
                            | "-h"
                            | "--version"
                            | "-V"
                            | "--headless"
                            | "--stdio"
                            | "--no-auto-update"
                            | "--acp"
                            | "--experimental-acp"
                            | "--continue"
                            | "--last"
                            | "--no-interactive"
                            | "--await-initialize"
                            | "--dangerously-skip-permissions"
                            | "--standalone"
                            | "--no-session"
                            | "--list-tools"
                    ) || self.rule.host_flags.iter().flatten().any(|f| f == arg)
                        || self.rule.excluded_flags.contains(arg)
                        || self.rule.headless_flags.contains(arg)
                        || matches!(
                            arg.as_str(),
                            "--resume" | "-r" | "-p" | "-x" | "--tui" | "-i"
                        );
                    if !known {
                        break;
                    }
                }
            } else {
                out.commands.push(arg);
                let known = self
                    .rule
                    .host_commands
                    .iter()
                    .chain(&self.rule.excluded_commands)
                    .chain(&self.rule.session_commands)
                    .any(|seq| seq.iter().any(|word| word == arg))
                    || self
                        .rule
                        .headless_commands
                        .iter()
                        .any(|seq| seq.contains(arg))
                    || self.rule.shared_host_subcommands.contains(arg)
                    || matches!(arg.as_str(), "exec" | "run" | "chat" | "session");
                if !known {
                    break;
                }
            }
            i += 1;
        }
        out
    }

    fn matches(&self, p: &Proc, ancestors: &[&Proc]) -> bool {
        if self.excluded.iter().any(|re| re.is_match(&p.exe)) {
            return false;
        }
        if !self.ancestors.is_empty()
            && !ancestors
                .iter()
                .any(|p| self.ancestors.iter().any(|re| re.is_match(&p.exe)))
        {
            return false;
        }
        let path_match = self.executable.iter().any(|re| re.is_match(&p.exe));
        let script = script_index(p)
            .and_then(|i| p.argv.get(i))
            .map(String::as_str)
            .unwrap_or(&p.exe);
        let script_match = self.argv.iter().any(|re| re.is_match(script));
        if !path_match
            && !script_match
            && !self.rule.executables.iter().any(|s| s == basename(&p.exe))
        {
            return false;
        }
        let args = self.arguments(p);
        self.rule
            .required_argv
            .iter()
            .all(|s| args.has(s) || args.commands.contains(&s.as_str()))
            && !self
                .rule
                .excluded_commands
                .iter()
                .any(|seq| args.starts(seq))
            && !self.rule.excluded_flags.iter().any(|flag| args.has(flag))
    }

    pub(crate) fn confidence(&self, p: &Proc) -> &'static str {
        let script = script_index(p)
            .and_then(|i| p.argv.get(i))
            .map(String::as_str)
            .unwrap_or(&p.exe);
        if self.executable.iter().any(|re| re.is_match(&p.exe))
            || self.argv.iter().any(|re| re.is_match(script))
        {
            "high"
        } else {
            "medium"
        }
    }

    pub(crate) fn host_mode(&self, p: &Proc) -> Option<String> {
        if self.rule.role == AgentRole::Host {
            return Some("service".into());
        }
        let args = self.arguments(p);
        self.rule
            .host_commands
            .iter()
            .find(|seq| args.starts(seq))
            .map(|seq| seq.join(" "))
            .or_else(|| {
                self.rule
                    .shared_host_subcommands
                    .iter()
                    .find(|m| args.commands.first().is_some_and(|a| *a == m.as_str()))
                    .cloned()
            })
            .or_else(|| {
                self.rule
                    .host_flags
                    .iter()
                    .find(|flags| flags.iter().all(|f| args.has(f)))
                    .map(|flags| flags.join(" "))
            })
    }

    pub(crate) fn headless(&self, p: &Proc) -> bool {
        let args = self.arguments(p);
        self.rule.headless_flags.iter().any(|flag| args.has(flag))
            || self
                .rule
                .headless_commands
                .iter()
                .any(|seq| args.starts(seq))
    }

    pub(crate) fn session_id(&self, p: &Proc) -> Option<String> {
        let pattern = self.session_id.as_ref()?;
        let args = self.arguments(p);
        let valid = |id: &&str| {
            !["latest", "last", "true", "false"].contains(id)
                && pattern
                    .find(id)
                    .is_some_and(|m| m.start() == 0 && m.end() == id.len())
                && crate::redact::redact(id) == *id
        };
        args.flags
            .iter()
            .filter(|(flag, _)| self.rule.session_flags.iter().any(|s| s == flag))
            .filter_map(|(_, value)| *value)
            .find(valid)
            .or_else(|| {
                self.rule
                    .session_commands
                    .iter()
                    .filter(|seq| args.starts(seq))
                    .filter_map(|seq| args.commands.get(seq.len()).copied())
                    .find(valid)
            })
            .map(str::to_string)
    }
}

fn builtin_matchers() -> &'static [AgentMatcher] {
    static TABLE: OnceLock<Vec<AgentMatcher>> = OnceLock::new();
    TABLE.get_or_init(|| builtins().into_iter().map(AgentMatcher::new).collect())
}

/// Compile configured additions; built-ins remain first so their verified rules stay stable.
pub fn compile_extra(rules: &[AgentRule]) -> Vec<AgentMatcher> {
    rules
        .iter()
        .filter(|r| r.valid_extra())
        .cloned()
        .map(AgentMatcher::new)
        .collect()
}

pub(crate) fn detect_with_ancestors<'a>(
    p: &Proc,
    extras: &'a [AgentMatcher],
    ancestors: &[&Proc],
) -> Option<&'a AgentMatcher> {
    builtin_matchers()
        .iter()
        .chain(extras)
        .find(|m| m.matches(p, ancestors))
}

pub(crate) fn detect<'a>(p: &Proc, extras: &'a [AgentMatcher]) -> Option<&'a AgentMatcher> {
    detect_with_ancestors(p, extras, &[])
}

pub(crate) fn is_family(p: &Proc, name: &str) -> bool {
    builtin_matchers()
        .iter()
        .filter(|m| m.rule.name == name)
        .any(|m| {
            // `is_codex` historically means the native binary (not the Node launcher).
            (name != "codex" || basename(&p.exe) == "codex") && m.matches(p, &[])
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn proc_with(exe: &str, argv: &[&str]) -> Proc {
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
            env: crate::model::Env::default(),
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
    fn command_positions_do_not_search_prompts() {
        let m = AgentMatcher::new(AgentRule {
            name: "Example".into(),
            executables: vec!["example".into()],
            shared_host_subcommands: vec!["serve".into()],
            host_commands: vec![vec!["agent".into(), "stdio".into()]],
            host_flags: vec![vec!["--mode=rpc".into()]],
            value_flags: vec!["-p".into(), "--mode".into()],
            ..Default::default()
        });
        for argv in [
            vec!["example", "-p", "serve"],
            vec!["example", "run", "serve"],
            vec!["example", "text", "--mode", "rpc"],
            vec!["example", "--", "serve"],
        ] {
            assert!(
                m.host_mode(&proc_with("/opt/example", &argv)).is_none(),
                "{argv:?}"
            );
        }
        for argv in [
            vec!["example", "serve"],
            vec!["example", "agent", "stdio"],
            vec!["example", "--mode", "rpc"],
        ] {
            assert!(
                m.host_mode(&proc_with("/opt/example", &argv)).is_some(),
                "{argv:?}"
            );
        }
    }

    #[test]
    fn config_guards_and_identity_are_shared() {
        let rule: AgentRule =
            serde_json::from_value(serde_json::json!({"name":"Example", "executables":["node"],
            "argv_patterns":[], "required_argv":["--acp"], "ancestor_patterns":["/Example\\.app/"],
            "session_flags":["--session-id"], "session_id_pattern":"^session-[0-9]+$"}))
            .unwrap();
        assert!(rule.valid_extra());
        let m = AgentMatcher::new(rule);
        let p = proc_with(
            "/usr/local/bin/node",
            &[
                "node",
                "/opt/server.js",
                "--acp",
                "--session-id=session-123",
            ],
        );
        let parent = proc_with(
            "/Applications/Example.app/Contents/MacOS/Example",
            &["Example"],
        );
        assert!(!m.matches(&p, &[]));
        assert!(m.matches(&p, &[&parent]));
        assert_eq!(m.session_id(&p).as_deref(), Some("session-123"));
        assert!(!m.matches(
            &proc_with("/usr/local/bin/node", &["node", "/opt/server.js"]),
            &[&parent]
        ));
    }
    pub(super) fn recognized(
        exe: &str,
        argv: &[&str],
        name: &str,
        role: &str,
        confidence: &str,
        id: Option<&str>,
    ) {
        let p = proc_with(exe, argv);
        let matcher = detect(&p, &[]).unwrap_or_else(|| panic!("missing {name}: {exe}"));
        assert_eq!(matcher.rule.name, name);
        let (workloads, _) = crate::snapshot::Snapshot::fixture(vec![p]).attribute();
        let w = &workloads[0];
        assert_eq!(w.kind, role, "{exe}");
        assert_eq!(w.confidence, confidence, "{exe}");
        assert_eq!(w.session_id.as_deref(), id, "{exe}");
    }

    #[test]
    fn claude_native_sdk_desktop_and_node() {
        for exe in ["/Users/user/.local/share/claude/versions/2.1.0", "/Users/user/Library/Application Support/Claude/claude-code/2.1.0/build/claude.app/Contents/MacOS/claude", "/opt/python/claude_agent_sdk/_bundled/claude"] {
            recognized(exe, &["claude", "--await-initialize", "--input-format", "stream-json"], "claude", "agent-session", "high", None);
            recognized(exe, &["claude", "--session-id=12345678-1234-1234-1234-123456789abc"], "claude", "agent-session", "exact", Some("12345678-1234-1234-1234-123456789abc"));
            recognized(exe, &["claude", "--resume", "pick-me"], "claude", "agent-session", "high", None);
        }
        recognized(
            "/opt/bin/node",
            &[
                "node",
                "/opt/node_modules/@anthropic-ai/claude-code/cli.js",
                "-p",
                "text",
            ],
            "claude",
            "agent-session",
            "high",
            None,
        );
        assert!(detect(&proc_with("/opt/bin/claude", &["claude"]), &[]).is_none());
    }

    #[test]
    fn codex_packages_chatgpt_hosts_and_explicit_resume() {
        for exe in ["/opt/node_modules/@openai/codex/node_modules/@openai/codex-darwin-arm64/vendor/aarch64-apple-darwin/bin/codex", "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex"] {
            recognized(exe, &["codex"], "codex", "agent-session", "high", None);
            for mode in ["app-server", "exec-server"] { recognized(exe, &["codex", mode], "codex", "agent-host", "high", None); }
            for args in [vec!["codex", "resume", "12345678-1234-1234-1234-123456789abc"], vec!["codex", "exec", "resume", "12345678-1234-1234-1234-123456789abc"]] {
                recognized(exe, &args, "codex", "agent-session", "exact", Some("12345678-1234-1234-1234-123456789abc"));
            }
            recognized(exe, &["codex", "exec", "app-server"], "codex", "agent-session", "high", None);
            recognized(exe, &["codex", "exec", "text --resume 12345678-1234-1234-1234-123456789abc"], "codex", "agent-session", "high", None);
        }
        recognized(
            "/opt/bin/node",
            &["node", "/opt/node_modules/@openai/codex/bin/codex.js"],
            "codex",
            "agent-session",
            "high",
            None,
        );
        for exe in ["/opt/bin/codex", "/opt/codex-code-mode-host/codex"] {
            assert!(detect(&proc_with(exe, &["codex"]), &[]).is_none());
        }
    }
}

#[cfg(test)]
#[path = "agents/cursor_google_grok_tests.rs"]
mod cursor_google_grok_tests;

#[cfg(test)]
#[path = "agents/terminal_tests.rs"]
mod terminal_tests;

#[cfg(test)]
#[path = "agents/tooling_tests.rs"]
mod tooling_tests;

#[cfg(test)]
#[path = "agents/additional_tests.rs"]
mod additional_tests;

#[cfg(test)]
#[path = "agents/packages_tests.rs"]
mod packages_tests;

#[cfg(test)]
#[path = "agents/hosts_tests.rs"]
mod hosts_tests;

#[cfg(test)]
#[path = "agents/guards_tests.rs"]
mod guards_tests;
