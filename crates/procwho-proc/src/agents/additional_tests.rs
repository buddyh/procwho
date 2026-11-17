use super::{
    detect,
    tests::{proc_with, recognized},
};

#[test]
fn additional_signature_1() {
    recognized(
        "/Users/user/.local/bin/droid",
        &[
            "droid",
            "exec",
            "-s",
            "12345678-1234-1234-1234-123456789abc",
        ],
        "Droid",
        "agent-session",
        "exact",
        Some("12345678-1234-1234-1234-123456789abc"),
    );
}

#[test]
fn additional_signature_2() {
    recognized(
        "/Users/user/.local/bin/droid",
        &["droid", "--resume", "12345678-1234-1234-1234-123456789abc"],
        "Droid",
        "agent-session",
        "exact",
        Some("12345678-1234-1234-1234-123456789abc"),
    );
}

#[test]
fn additional_signature_3() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@qwen-code/qwen-code/dist/index.js",
            "--session-id",
            "session-example",
        ],
        "Qwen",
        "agent-session",
        "exact",
        Some("session-example"),
    );
}

#[test]
fn additional_signature_4() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@qwen-code/qwen-code/dist/index.js",
            "serve",
        ],
        "Qwen",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn additional_signature_5() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@qwen-code/qwen-code/dist/index.js",
            "--acp",
        ],
        "Qwen",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn additional_signature_6() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@qwen-code/qwen-code/dist/index.js",
            "--experimental-acp",
        ],
        "Qwen",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn additional_signature_7() {
    recognized(
        "/opt/homebrew/Cellar/crush/1.0.0/bin/crush",
        &["crush", "server"],
        "Crush",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn additional_signature_8() {
    recognized(
        "/opt/bin/node",
        &["node", "/opt/node_modules/@charmland/crush/run-crush.js"],
        "Crush",
        "agent-session",
        "high",
        None,
    );
}

#[test]
fn additional_negative_1() {
    assert!(detect(&proc_with("/usr/local/bin/droid", &["droid"]), &[]).is_none());
}

#[test]
fn additional_negative_2() {
    assert!(detect(&proc_with("/usr/local/bin/qwen", &["qwen"]), &[]).is_none());
}

#[test]
fn additional_negative_3() {
    assert!(detect(&proc_with("/usr/local/bin/crush", &["crush"]), &[]).is_none());
}

#[test]
fn additional_negative_4() {
    assert!(detect(
        &proc_with(
            "/Users/user/.local/bin/droid",
            &["droid", "exec", "--list-tools"]
        ),
        &[]
    )
    .is_none());
}

#[test]
fn additional_negative_5() {
    assert!(detect(
        &proc_with(
            "/opt/bin/node",
            &["node", "/Users/user/project/qwen/dist/index.js"]
        ),
        &[]
    )
    .is_none());
}

#[test]
fn additional_negative_6() {
    assert!(detect(
        &proc_with(
            "/opt/bin/node",
            &["node", "/Users/user/project/run-crush.js"]
        ),
        &[]
    )
    .is_none());
}
