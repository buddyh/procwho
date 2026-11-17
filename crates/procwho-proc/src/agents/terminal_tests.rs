use super::{
    detect,
    tests::{proc_with, recognized},
};

#[test]
fn terminal_signature_1() {
    recognized(
        "/opt/node_modules/opencode-ai/bin/opencode.exe",
        &["opencode", "-s", "session_example"],
        "OpenCode",
        "agent-session",
        "exact",
        Some("session_example"),
    );
}

#[test]
fn terminal_signature_2() {
    recognized(
        "/opt/node_modules/opencode-ai/bin/opencode.exe",
        &["opencode", "serve"],
        "OpenCode",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn terminal_signature_3() {
    recognized(
        "/opt/node_modules/opencode-ai/bin/opencode.exe",
        &["opencode", "web"],
        "OpenCode",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn terminal_signature_4() {
    recognized(
        "/opt/node_modules/opencode-ai/bin/opencode",
        &["opencode", "-s", "session_example"],
        "OpenCode",
        "agent-session",
        "exact",
        Some("session_example"),
    );
}

#[test]
fn terminal_signature_5() {
    recognized(
        "/opt/node_modules/opencode-ai/bin/opencode",
        &["opencode", "serve"],
        "OpenCode",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn terminal_signature_6() {
    recognized(
        "/opt/node_modules/opencode-ai/bin/opencode",
        &["opencode", "web"],
        "OpenCode",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn terminal_signature_7() {
    recognized(
        "/opt/bin/node",
        &["node", "/opt/node_modules/opencode-ai/bin/opencode"],
        "OpenCode",
        "agent-session",
        "high",
        None,
    );
}

#[test]
fn terminal_signature_8() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js",
            "--session-id",
            "12345678-1234-1234-1234-123456789abc",
        ],
        "pi",
        "agent-session",
        "exact",
        Some("12345678-1234-1234-1234-123456789abc"),
    );
}

#[test]
fn terminal_signature_9() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js",
            "--mode",
            "rpc",
        ],
        "pi",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn terminal_signature_10() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js",
            "-p",
            "rpc",
        ],
        "pi",
        "agent-session",
        "high",
        None,
    );
}

#[test]
fn terminal_signature_11() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@mariozechner/pi-coding-agent/dist/cli.js",
            "--session-id",
            "12345678-1234-1234-1234-123456789abc",
        ],
        "pi",
        "agent-session",
        "exact",
        Some("12345678-1234-1234-1234-123456789abc"),
    );
}

#[test]
fn terminal_signature_12() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@mariozechner/pi-coding-agent/dist/cli.js",
            "--mode",
            "rpc",
        ],
        "pi",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn terminal_signature_13() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@mariozechner/pi-coding-agent/dist/cli.js",
            "-p",
            "rpc",
        ],
        "pi",
        "agent-session",
        "high",
        None,
    );
}

#[test]
fn terminal_signature_14() {
    recognized(
        "/opt/bin/kiro-cli",
        &[
            "kiro-cli",
            "chat",
            "--resume-id",
            "12345678-1234-1234-1234-123456789abc",
        ],
        "Kiro",
        "agent-session",
        "exact",
        Some("12345678-1234-1234-1234-123456789abc"),
    );
}

#[test]
fn terminal_signature_15() {
    recognized(
        "/opt/bin/kiro-cli",
        &["kiro-cli", "chat"],
        "Kiro",
        "agent-session",
        "medium",
        None,
    );
}

#[test]
fn terminal_signature_16() {
    recognized(
        "/Users/user/.amp/bin/amp",
        &[
            "amp",
            "threads",
            "continue",
            "T-12345678-1234-1234-1234-123456789abc",
        ],
        "Amp",
        "agent-session",
        "exact",
        Some("T-12345678-1234-1234-1234-123456789abc"),
    );
}

#[test]
fn terminal_signature_17() {
    recognized(
        "/opt/node_modules/@ampcode/cli/bin/amp.exe",
        &[
            "amp",
            "threads",
            "continue",
            "T-12345678-1234-1234-1234-123456789abc",
        ],
        "Amp",
        "agent-session",
        "exact",
        Some("T-12345678-1234-1234-1234-123456789abc"),
    );
}

#[test]
fn terminal_signature_18() {
    recognized(
        "/opt/bin/node",
        &["node", "/opt/node_modules/@ampcode/cli/bin/amp.exe"],
        "Amp",
        "agent-session",
        "high",
        None,
    );
}

#[test]
fn terminal_negative_1() {
    assert!(detect(&proc_with("/usr/local/bin/opencode", &["opencode"]), &[]).is_none());
}

#[test]
fn terminal_negative_2() {
    assert!(detect(&proc_with("/usr/local/bin/pi", &["pi", "3.14"]), &[]).is_none());
}

#[test]
fn terminal_negative_3() {
    assert!(detect(&proc_with("/usr/local/bin/amp", &["amp"]), &[]).is_none());
}

#[test]
fn terminal_negative_4() {
    assert!(detect(&proc_with("/usr/local/bin/q", &["q"]), &[]).is_none());
}

#[test]
fn terminal_negative_5() {
    assert!(detect(&proc_with("/usr/local/bin/kiro", &["kiro"]), &[]).is_none());
}

#[test]
fn terminal_negative_6() {
    assert!(detect(
        &proc_with("/opt/bin/node", &["node", "/opt/pi/cli.js"]),
        &[]
    )
    .is_none());
}

#[test]
fn terminal_negative_7() {
    assert!(detect(
        &proc_with("/opt/bin/bun", &["bun", "/Users/user/project/server.js"]),
        &[]
    )
    .is_none());
}

#[test]
fn terminal_negative_8() {
    assert!(detect(
        &proc_with("/opt/bin/kiro-cli", &["kiro-cli", "--cloud"]),
        &[]
    )
    .is_none());
}

#[test]
fn terminal_negative_9() {
    assert!(detect(
        &proc_with("/Users/user/.amp/bin/amp", &["amp", "--executor", "orb"]),
        &[]
    )
    .is_none());
}

#[test]
fn terminal_negative_10() {
    assert!(detect(
        &proc_with(
            "/opt/bin/node",
            &[
                "node",
                "/opt/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js",
                "--help"
            ]
        ),
        &[]
    )
    .is_none());
}
