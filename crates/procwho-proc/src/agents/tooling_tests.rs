use super::{
    detect,
    tests::{proc_with, recognized},
};

#[test]
fn tooling_signature_1() {
    recognized(
        "/opt/bin/python3",
        &[
            "python3",
            "/Users/user/.local/share/uv/tools/aider-chat/bin/aider",
            "--message",
            "text",
        ],
        "Aider",
        "agent-session",
        "high",
        None,
    );
}

#[test]
fn tooling_signature_2() {
    recognized(
        "/opt/bin/python3",
        &[
            "python3",
            "/Users/user/.local/share/pipx/venvs/aider-chat/bin/aider",
            "--message",
            "text",
        ],
        "Aider",
        "agent-session",
        "high",
        None,
    );
}

#[test]
fn tooling_signature_3() {
    recognized(
        "/opt/bin/python3",
        &["python3", "-m", "aider"],
        "Aider",
        "agent-session",
        "medium",
        None,
    );
}

#[test]
fn tooling_signature_4() {
    recognized(
        "/opt/homebrew/Cellar/block-goose-cli/1.0.0/bin/goose",
        &[
            "goose",
            "session",
            "--resume",
            "--session-id",
            "20261116_12",
        ],
        "Goose",
        "agent-session",
        "exact",
        Some("20261116_12"),
    );
}

#[test]
fn tooling_signature_5() {
    recognized(
        "/opt/homebrew/Cellar/block-goose-cli/1.0.0/bin/goose",
        &["goose", "serve"],
        "Goose",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn tooling_signature_6() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@github/copilot/npm-loader.js",
            "--resume",
            "session-example",
        ],
        "GitHub Copilot",
        "agent-session",
        "exact",
        Some("session-example"),
    );
}

#[test]
fn tooling_signature_7() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@github/copilot/npm-loader.js",
            "--headless",
            "--no-auto-update",
            "--stdio",
        ],
        "GitHub Copilot",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn tooling_signature_8() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@github/copilot/npm-loader.js",
            "-p",
            "--headless",
        ],
        "GitHub Copilot",
        "agent-session",
        "high",
        None,
    );
}

#[test]
fn tooling_signature_9() {
    recognized(
        "/opt/node_modules/cline/bin/.cline",
        &["cline", "--id", "task-123"],
        "Cline",
        "agent-session",
        "exact",
        Some("task-123"),
    );
}

#[test]
fn tooling_signature_10() {
    recognized(
        "/opt/node_modules/@cline/cli-darwin-arm64/bin/cline",
        &["cline", "--id", "task-123"],
        "Cline",
        "agent-session",
        "exact",
        Some("task-123"),
    );
}

#[test]
fn tooling_signature_11() {
    recognized(
        "/opt/bin/node",
        &["node", "/opt/node_modules/cline/bin/cline", "-p"],
        "Cline",
        "agent-session",
        "high",
        None,
    );
}

#[test]
fn tooling_negative_1() {
    assert!(detect(&proc_with("/usr/local/bin/aider", &["aider"]), &[]).is_none());
}

#[test]
fn tooling_negative_2() {
    assert!(detect(&proc_with("/usr/local/bin/goose", &["goose", "up"]), &[]).is_none());
}

#[test]
fn tooling_negative_3() {
    assert!(detect(
        &proc_with("/opt/homebrew/Cellar/goose/1.0/bin/goose", &["goose", "up"]),
        &[]
    )
    .is_none());
}

#[test]
fn tooling_negative_4() {
    assert!(detect(&proc_with("/usr/local/bin/copilot", &["copilot"]), &[]).is_none());
}

#[test]
fn tooling_negative_5() {
    assert!(detect(
        &proc_with("/usr/local/bin/gh", &["gh", "copilot", "suggest"]),
        &[]
    )
    .is_none());
}

#[test]
fn tooling_negative_6() {
    assert!(detect(&proc_with("/usr/local/bin/cline", &["cline"]), &[]).is_none());
}

#[test]
fn tooling_negative_7() {
    assert!(detect(
        &proc_with("/opt/bin/python3", &["python3", "-m", "http.server"]),
        &[]
    )
    .is_none());
}

#[test]
fn tooling_negative_8() {
    assert!(detect(
        &proc_with("/opt/bin/python3", &["python3", "-c", "import aider"]),
        &[]
    )
    .is_none());
}

#[test]
fn tooling_negative_9() {
    assert!(detect(
        &proc_with("/opt/node_modules/cline/bin/.cline", &["cline", "--zen"]),
        &[]
    )
    .is_none());
}

#[test]
fn tooling_negative_10() {
    assert!(detect(
        &proc_with("/opt/node_modules/cline/bin/.cline", &["cline", "hub"]),
        &[]
    )
    .is_none());
}

#[test]
fn cline_plan_is_not_headless_and_prompt_words_are_not_host_flags() {
    let p = proc_with("/opt/node_modules/cline/bin/.cline", &["cline", "-p"]);
    assert!(!detect(&p, &[]).unwrap().headless(&p));
    let p = proc_with(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@github/copilot/npm-loader.js",
            "-p",
            "--headless",
        ],
    );
    assert!(detect(&p, &[]).unwrap().host_mode(&p).is_none());
}
