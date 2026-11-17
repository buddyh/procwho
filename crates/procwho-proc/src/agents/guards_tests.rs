use super::{
    detect,
    tests::{proc_with, recognized},
};
use crate::snapshot::Snapshot;

#[test]
fn argv_zero_and_interpreter_option_values_are_not_launcher_scripts() {
    let script = "/opt/node_modules/@google/gemini-cli/dist/index.js";
    for p in [
        proc_with("/bin/echo", &[script]),
        proc_with(
            "/opt/bin/node",
            &["node", "--title", script, "/Users/user/project/server.js"],
        ),
        proc_with("/opt/bin/node", &["node", "-e", script]),
        proc_with("/opt/bin/node", &["node", "--eval=1+1", script]),
        proc_with("/opt/bin/python-calculator", &["python-calculator", script]),
        proc_with(
            "/usr/bin/python3",
            &["python3", "/Users/user/project/server.py"],
        ),
    ] {
        assert!(detect(&p, &[]).is_none());
    }
    recognized(
        "/opt/bin/node",
        &["node", "--max-old-space-size=1024", script],
        "Gemini",
        "agent-session",
        "high",
        None,
    );
    recognized(
        "/opt/bin/node",
        &["node", "--require", "/opt/preload.js", script],
        "Gemini",
        "agent-session",
        "high",
        None,
    );
}

#[test]
fn picker_partial_uuid_and_prompt_text_never_supply_identity() {
    let exe = "/Users/user/.local/share/claude/versions/2.1.0";
    for args in [
        vec!["claude", "--resume"],
        vec!["claude", "--resume=12345678"],
        vec![
            "claude",
            "--resume",
            "12345678-1234-1234-1234-123456789abc-extra",
        ],
        vec![
            "claude",
            "text --session-id 12345678-1234-1234-1234-123456789abc",
        ],
        vec!["claude", "--resume", "pick-me"],
    ] {
        assert!(detect(&proc_with(exe, &args), &[])
            .unwrap()
            .session_id(&proc_with(exe, &args))
            .is_none());
    }
    let script = "/opt/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js";
    recognized(
        "/opt/bin/node",
        &[
            "node",
            script,
            "--session",
            "12345678-1234-1234-1234-123456789abc",
        ],
        "pi",
        "agent-session",
        "exact",
        Some("12345678-1234-1234-1234-123456789abc"),
    );
    recognized(
        "/opt/bin/node",
        &["node", script, "--session", "/Users/user/session.jsonl"],
        "pi",
        "agent-session",
        "high",
        None,
    );
    assert!(detect(&proc_with("/opt/bin/kiro-cli", &["kiro-cli"]), &[]).is_none());
}

#[test]
fn tmux_is_context_not_exact_agent_identity() {
    let p = proc_with(
        "/Users/user/.local/share/claude/versions/2.1.0",
        &["claude"],
    );
    let mut snapshot = Snapshot::fixture(vec![p]);
    snapshot.pane_pid.insert(
        1,
        crate::model::Pane {
            session: "example".into(),
            pane: "%1".into(),
            pid: 1,
            tty: "ttys001".into(),
        },
    );
    let (workloads, _) = snapshot.attribute();
    assert_eq!(workloads[0].confidence, "high");
    assert!(workloads[0].session_id.is_none());
    assert!(workloads[0].tmux.is_some());
}

#[test]
fn node_agent_sessions_are_independent_but_verified_wrappers_are_not_duplicated() {
    for (script, native, args, count) in [
        ("/opt/node_modules/@anthropic-ai/claude-code/cli.js", "/Users/user/.local/share/claude/versions/2.1.0", vec!["claude"], 2),
        ("/opt/node_modules/@openai/codex/bin/codex.js", "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex", vec!["codex", "app-server"], 1),
        ("/opt/node_modules/cline/bin/cline", "/opt/node_modules/cline/bin/.cline", vec!["cline"], 1),
        ("/opt/node_modules/@charmland/crush/run-crush.js", "/opt/homebrew/Cellar/crush/1.0.0/bin/crush", vec!["crush"], 1),
    ] {
        let mut parent_args = vec!["node", script]; parent_args.extend(args.iter().skip(1).copied());
        let mut parent = proc_with("/opt/bin/node", &parent_args); parent.pid = 10;
        let mut child = proc_with(native, &args); child.pid = 11; child.ppid = 10;
        let (workloads, _) = Snapshot::fixture(vec![parent, child]).attribute();
        assert_eq!(workloads.len(), count, "{script}");
    }
}

#[test]
fn boolean_false_and_prompt_words_do_not_select_hosts() {
    let script = "/opt/node_modules/@github/copilot/npm-loader.js";
    let p = proc_with("/opt/bin/node", &["node", script, "--headless=false"]);
    assert!(detect(&p, &[]).unwrap().host_mode(&p).is_none());
    let p = proc_with(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/opencode-ai/bin/opencode",
            "run",
            "serve",
        ],
    );
    assert!(detect(&p, &[]).unwrap().host_mode(&p).is_none());
}

#[test]
fn all_builtin_regexes_and_explicit_identity_selectors_are_valid() {
    for r in super::builtins() {
        for pattern in r
            .executable_patterns
            .iter()
            .chain(&r.excluded_executable_patterns)
            .chain(&r.argv_patterns)
            .chain(&r.ancestor_patterns)
            .chain(r.session_id_pattern.iter())
        {
            regex::Regex::new(pattern).unwrap();
        }
        assert!(
            r.session_flags.is_empty() && r.session_commands.is_empty()
                || r.session_id_pattern.is_some()
        );
    }
}

#[test]
fn codex_code_mode_helper_requires_a_verified_provider_ancestor() {
    let helper_path = "/opt/node_modules/@openai/codex/node_modules/@openai/codex-darwin-arm64/vendor/aarch64-apple-darwin/bin/codex-code-mode-host";
    let mut helper = proc_with(
        helper_path,
        &["codex-code-mode-host", "--listen", "grpc+stdio://"],
    );
    helper.pid = 11;
    helper.ppid = 10;
    assert!(detect(&helper, &[]).is_none());
    let unrelated = proc_with("/opt/bin/node", &["node", "/Users/user/project/server.js"]);
    assert!(super::detect_with_ancestors(&helper, &[], &[&unrelated]).is_none());
    for parent_path in ["/opt/node_modules/@openai/codex/node_modules/@openai/codex-darwin-arm64/vendor/aarch64-apple-darwin/bin/codex", "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex"] {
        let mut parent = proc_with(parent_path, &["codex", "app-server"]); parent.pid = 10;
        let (workloads, assign) = Snapshot::fixture(vec![parent, helper.clone()]).attribute();
        let w = &workloads[assign[1]];
        assert_eq!(w.kind, "agent-toolset");
        assert_eq!(w.confidence, "high");
        assert!(w.session_id.is_none());
        assert_eq!(w.parent, Some(assign[0]));
    }
}

#[test]
fn chatgpt_config_options_before_codex_server_modes_are_parsed() {
    let exe =
        "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex";
    for mode in ["app-server", "exec-server"] {
        recognized(
            exe,
            &[
                "codex",
                "-c",
                "features.example=true",
                mode,
                "--analytics-default-enabled",
            ],
            "codex",
            "agent-host",
            "high",
            None,
        );
    }
    recognized(
        exe,
        &[
            "codex",
            "-c",
            "features.example=true",
            "exec",
            "resume",
            "12345678-1234-1234-1234-123456789abc",
        ],
        "codex",
        "agent-session",
        "exact",
        Some("12345678-1234-1234-1234-123456789abc"),
    );
}

#[test]
fn chatgpt_code_mode_and_computer_use_helpers_require_codex_ancestry() {
    let mut parent = proc_with(
        "/Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex",
        &["codex", "exec-server"],
    );
    parent.pid = 10;
    for exe in [
        "/Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex-code-mode-host",
        "/Applications/ChatGPT.app/Contents/Resources/cua_node/bin/node_repl",
    ] {
        let mut helper = proc_with(exe, &["helper"]);
        helper.pid = 11;
        helper.ppid = 10;
        assert!(detect(&helper, &[]).is_none());
        let (workloads, assign) = Snapshot::fixture(vec![parent.clone(), helper]).attribute();
        let w = &workloads[assign[1]];
        assert_eq!(w.kind, "agent-toolset");
        assert_eq!(w.confidence, "high");
        assert!(w.session_id.is_none());
    }
    assert!(detect(&proc_with("/usr/local/bin/node_repl", &["node_repl"]), &[]).is_none());
}
