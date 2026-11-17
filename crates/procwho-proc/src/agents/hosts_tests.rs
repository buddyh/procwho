use super::{
    detect,
    tests::{proc_with, recognized},
};

#[test]
fn hosts_signature_1() {
    recognized(
        "/Applications/T3 Code (Nightly).app/Contents/MacOS/T3 Code (Nightly)",
        &["T3 Code (Nightly)"],
        "T3 Code",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn hosts_signature_2() {
    recognized(
        "/Applications/T3 Code.app/Contents/MacOS/T3 Code",
        &["T3 Code"],
        "T3 Code",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn hosts_signature_3() {
    recognized(
        "/Applications/Antigravity.app/Contents/Resources/bin/language_server",
        &["language_server"],
        "Antigravity",
        "agent-host",
        "high",
        None,
    );
}

#[test]
fn hosts_signature_4() {
    recognized("/Applications/Devin.app/Contents/Resources/app/extensions/windsurf/bin/language_server_macos_arm", &["language_server_macos_arm"], "Windsurf", "agent-host", "high", None);
}

#[test]
fn hosts_negative_1() {
    assert!(detect(
        &proc_with("/usr/bin/language_server", &["language_server"]),
        &[]
    )
    .is_none());
}

#[test]
fn hosts_negative_2() {
    assert!(detect(
        &proc_with(
            "/opt/tools/language_server_macos_arm",
            &["language_server_macos_arm"]
        ),
        &[]
    )
    .is_none());
}

#[test]
fn hosts_negative_3() {
    assert!(detect(&proc_with("/usr/local/bin/zed", &["zed"]), &[]).is_none());
}

#[test]
fn editors_require_a_proven_provider_and_do_not_claim_ordinary_helpers_as_commands() {
    for (name, exe) in [
        ("Cursor", "/Applications/Cursor.app/Contents/MacOS/Cursor"),
        ("Zed", "/Applications/Zed.app/Contents/MacOS/zed"),
    ] {
        let mut editor = proc_with(exe, &[name]);
        editor.pid = 10;
        let mut helper = proc_with("/opt/bin/node", &["node", "/Users/user/project/server.js"]);
        helper.pid = 11;
        helper.ppid = 10;
        let (workloads, _) =
            crate::snapshot::Snapshot::fixture(vec![editor.clone(), helper.clone()]).attribute();
        assert!(workloads.iter().all(|w| !w.kind.starts_with("agent")));
        let mut agent = proc_with(
            "/opt/bin/node",
            &[
                "node",
                "/opt/node_modules/@earendil-works/pi-coding-agent/dist/bundle/cli.js",
                "--mode",
                "rpc",
            ],
        );
        agent.pid = 12;
        agent.ppid = 10;
        let (workloads, assign) =
            crate::snapshot::Snapshot::fixture(vec![editor, helper, agent]).attribute();
        assert_eq!(workloads[assign[0]].kind, "agent-host");
        assert_eq!(workloads[assign[0]].agent.as_deref(), Some(name));
        assert_eq!(assign[0], assign[1]);
        assert_ne!(assign[0], assign[2]);
        assert!(workloads.iter().all(|w| w.session_id.is_none()));
    }
}

#[test]
fn t3_thread_requires_an_ancestor_host_and_never_identifies_the_shared_host() {
    let exe = "/Applications/T3 Code (Nightly).app/Contents/MacOS/T3 Code (Nightly)";
    let mut host = proc_with(exe, &["T3 Code (Nightly)"]);
    host.pid = 10;
    host.env.t3_thread = Some("12345678-1234-1234-1234-123456789abc".into());
    let mut server = host.clone();
    server.pid = 11;
    server.ppid = 10;
    let mut agent = proc_with(
        "/Users/user/.local/share/claude/versions/2.1.0",
        &["claude"],
    );
    agent.pid = 12;
    agent.ppid = 11;
    agent.env.t3_thread = host.env.t3_thread.clone();
    let (workloads, assign) =
        crate::snapshot::Snapshot::fixture(vec![host, server, agent.clone()]).attribute();
    assert_eq!(assign[0], assign[1]);
    assert_eq!(workloads[assign[0]].confidence, "high");
    assert!(workloads[assign[0]].t3_thread.is_none());
    assert_eq!(workloads[assign[2]].confidence, "exact");
    assert!(workloads[assign[2]].session_id.is_none());
    assert!(workloads[assign[2]].t3_thread.is_some());
    agent.ppid = 1;
    let (workloads, _) = crate::snapshot::Snapshot::fixture(vec![agent]).attribute();
    assert_eq!(workloads[0].confidence, "high");
    assert!(workloads[0].t3_thread.is_none());
}

#[test]
fn conductor_keeps_the_provider_family_and_records_app_origin() {
    let mut host = proc_with(
        "/Applications/Conductor.app/Contents/MacOS/Conductor",
        &["Conductor"],
    );
    host.pid = 10;
    let mut agent = proc_with(
        "/Users/user/.local/share/claude/versions/2.1.0",
        &["claude"],
    );
    agent.pid = 11;
    agent.ppid = 10;
    let (workloads, assign) = crate::snapshot::Snapshot::fixture(vec![host, agent]).attribute();
    let w = &workloads[assign[1]];
    assert_eq!(w.agent.as_deref(), Some("claude"));
    assert_eq!(w.host.as_deref(), Some("Conductor"));
}
