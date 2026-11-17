use super::{
    detect,
    tests::{proc_with, recognized},
};

#[test]
fn cursor_google_grok_signature_1() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/Users/user/.local/share/cursor-agent/versions/1.0.0/index.js",
        ],
        "Cursor",
        "agent-session",
        "high",
        None,
    );
}

#[test]
fn cursor_google_grok_signature_2() {
    recognized(
        "/Users/user/.local/share/cursor-agent/versions/1.0.0/cursor-agent",
        &["cursor-agent", "--resume", "chat-123"],
        "Cursor",
        "agent-session",
        "exact",
        Some("chat-123"),
    );
}

#[test]
fn cursor_google_grok_signature_3() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@google/gemini-cli/bundle/gemini.js",
            "--resume",
            "12345678-1234-1234-1234-123456789abc",
        ],
        "Gemini",
        "agent-session",
        "exact",
        Some("12345678-1234-1234-1234-123456789abc"),
    );
}

#[test]
fn cursor_google_grok_signature_4() {
    recognized(
        "/opt/bin/node",
        &[
            "node",
            "/opt/node_modules/@google/gemini-cli/dist/index.js",
            "--resume",
            "12345678-1234-1234-1234-123456789abc",
        ],
        "Gemini",
        "agent-session",
        "exact",
        Some("12345678-1234-1234-1234-123456789abc"),
    );
}

#[test]
fn cursor_google_grok_signature_5() {
    recognized(
        "/Users/user/.local/bin/agy",
        &["agy", "--conversation=12345678-1234-1234-1234-123456789abc"],
        "Antigravity",
        "agent-session",
        "exact",
        Some("12345678-1234-1234-1234-123456789abc"),
    );
}

#[test]
fn cursor_google_grok_signature_6() {
    recognized(
        "/Users/user/.grok/downloads/grok-1.0.0-macos-aarch64",
        &[
            "grok",
            "--session-id",
            "12345678-1234-1234-1234-123456789abc",
        ],
        "Grok",
        "agent-session",
        "exact",
        Some("12345678-1234-1234-1234-123456789abc"),
    );
}

#[test]
fn cursor_google_grok_signature_7() {
    recognized(
        "/Users/user/.grok/downloads/grok-1.0.0-macos-aarch64",
        &["grok", "agent", "stdio"],
        "Grok",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn cursor_google_grok_signature_8() {
    recognized(
        "/Users/user/.grok/bin/grok",
        &[
            "grok",
            "--session-id",
            "12345678-1234-1234-1234-123456789abc",
        ],
        "Grok",
        "agent-session",
        "exact",
        Some("12345678-1234-1234-1234-123456789abc"),
    );
}

#[test]
fn cursor_google_grok_signature_9() {
    recognized(
        "/Users/user/.grok/bin/grok",
        &["grok", "agent", "stdio"],
        "Grok",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn cursor_google_grok_negative_1() {
    assert!(detect(&proc_with("/usr/bin/cursor", &["cursor"]), &[]).is_none());
}

#[test]
fn cursor_google_grok_negative_2() {
    assert!(detect(
        &proc_with("/usr/local/bin/cursor-agent", &["cursor-agent"]),
        &[]
    )
    .is_none());
}

#[test]
fn cursor_google_grok_negative_3() {
    assert!(detect(&proc_with("/usr/local/bin/gemini", &["gemini"]), &[]).is_none());
}

#[test]
fn cursor_google_grok_negative_4() {
    assert!(detect(
        &proc_with("/opt/bin/node", &["node", "/Users/user/project/gemini.js"]),
        &[]
    )
    .is_none());
}

#[test]
fn cursor_google_grok_negative_5() {
    assert!(detect(
        &proc_with("/opt/bin/node", &["node", "/Users/user/project/server.js"]),
        &[]
    )
    .is_none());
}

#[test]
fn cursor_google_grok_negative_6() {
    assert!(detect(&proc_with("/usr/local/bin/agy", &["agy"]), &[]).is_none());
}

#[test]
fn cursor_google_grok_negative_7() {
    assert!(detect(&proc_with("/usr/local/bin/grok", &["grok"]), &[]).is_none());
}

#[test]
fn cursor_google_grok_negative_8() {
    assert!(detect(&proc_with("/Applications/Cursor.app/Contents/Frameworks/Cursor Helper (Renderer).app/Contents/MacOS/Cursor Helper (Renderer)", &["Cursor Helper (Renderer)", "--type=renderer"]), &[]).is_none());
}

#[test]
fn cursor_google_grok_negative_9() {
    assert!(detect(&proc_with("/Applications/Cursor.app/Contents/Frameworks/Cursor Helper (GPU).app/Contents/MacOS/Cursor Helper (GPU)", &["Cursor Helper (GPU)", "--type=gpu-process"]), &[]).is_none());
}
