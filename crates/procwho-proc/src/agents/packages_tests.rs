use super::{
    detect,
    tests::{proc_with, recognized},
};

#[test]
fn packages_signature_1() {
    recognized(
        "/opt/bin/node",
        &["node", "/opt/node_modules/@kilocode/cli/bin/kilo"],
        "Kilo",
        "agent-session",
        "high",
        None,
    );
}

#[test]
fn packages_signature_2() {
    recognized(
        "/opt/bin/node",
        &["node", "/opt/node_modules/@kilocode/cli/bin/kilo", "serve"],
        "Kilo",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn packages_signature_3() {
    recognized(
        "/opt/bin/node",
        &["node", "/opt/node_modules/@kilocode/cli/bin/kilo", "acp"],
        "Kilo",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn packages_signature_4() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@continuedev/cli/dist/cn.js",
            "-p",
            "text",
        ],
        "Continue",
        "agent-session",
        "high",
        None,
    );
}

#[test]
fn packages_signature_5() {
    recognized(
        "/Users/user/.local/share/devin/cli/_versions/1.0.0/bin/devin",
        &["devin", "--resume", "session-example"],
        "Devin",
        "agent-session",
        "exact",
        Some("session-example"),
    );
}

#[test]
fn packages_signature_6() {
    recognized(
        "/Users/user/.local/share/cognition/cli/_versions/1.0.0/bin/devin",
        &["devin", "--resume", "session-example"],
        "Devin",
        "agent-session",
        "exact",
        Some("session-example"),
    );
}

#[test]
fn packages_signature_7() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@augmentcode/auggie/augment.mjs",
            "--acp",
        ],
        "Auggie",
        "agent-session",
        "high",
        None,
    );
}

#[test]
fn packages_negative_1() {
    assert!(detect(&proc_with("/usr/local/bin/kilo", &["kilo"]), &[]).is_none());
}

#[test]
fn packages_negative_2() {
    assert!(detect(&proc_with("/usr/local/bin/kilocode", &["kilocode"]), &[]).is_none());
}

#[test]
fn packages_negative_3() {
    assert!(detect(&proc_with("/usr/local/bin/cn", &["cn"]), &[]).is_none());
}

#[test]
fn packages_negative_4() {
    assert!(detect(&proc_with("/usr/local/bin/devin", &["devin"]), &[]).is_none());
}

#[test]
fn packages_negative_5() {
    assert!(detect(&proc_with("/usr/local/bin/auggie", &["auggie"]), &[]).is_none());
}

#[test]
fn packages_negative_6() {
    assert!(detect(
        &proc_with(
            "/Users/user/.local/share/devin/cli/_versions/1.0.0/bin/devin",
            &["devin", "--cloud"]
        ),
        &[]
    )
    .is_none());
}

#[test]
fn packages_negative_7() {
    assert!(detect(
        &proc_with("/opt/bin/node", &["node", "/Users/user/project/cn.js"]),
        &[]
    )
    .is_none());
}
