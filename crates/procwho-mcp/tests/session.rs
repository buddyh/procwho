use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

/// Exercise the real stdio binary without depending on process names or scanning user repos.
#[test]
fn scripted_stdio_session() {
    let home = std::env::temp_dir().join(format!("procwho-mcp-session-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    let state = home.join("state");
    let mut child = Command::new(env!("CARGO_BIN_EXE_procwho-mcp"))
        .env("HOME", &home)
        .env("PROCWHO_STATE", &state)
        .env("PROCWHO_REGISTRY", home.join("registry.json"))
        .env("PROCWHO_CONFIG", home.join("config.json"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let mut input = child.stdin.take().unwrap();
    let mut output = BufReader::new(child.stdout.take().unwrap());
    let send = |input: &mut std::process::ChildStdin, value: Value| {
        writeln!(input, "{value}").unwrap();
        input.flush().unwrap();
    };
    let read = |output: &mut BufReader<std::process::ChildStdout>| -> Value {
        let mut line = String::new();
        assert!(output.read_line(&mut line).unwrap() > 0);
        serde_json::from_str(&line).expect("stdout must contain only protocol JSON")
    };
    send(
        &mut input,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":procwho_mcp::LEGACY_VERSION,"capabilities":{},"clientInfo":{"name":"test-client","version":"1"}
        }}),
    );
    let init = read(&mut output);
    assert_eq!(
        init["result"]["protocolVersion"],
        procwho_mcp::LEGACY_VERSION
    );
    assert!(init["result"]["capabilities"]["tools"].is_object());
    assert!(init["result"]["instructions"]
        .as_str()
        .unwrap()
        .contains("one core"));
    send(
        &mut input,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    send(
        &mut input,
        json!({"jsonrpc":"2.0","method":"notifications/unknown"}),
    );
    send(
        &mut input,
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    );
    let list = read(&mut output);
    let tools = list["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 6);
    for tool in tools {
        assert_eq!(tool["annotations"]["readOnlyHint"], true);
        assert_eq!(tool["annotations"]["destructiveHint"], false);
        assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        assert!(tool["outputSchema"].is_object());
    }
    let calls = [
        ("procwho_status", json!({"limit":1})),
        ("procwho_why", json!({"target":pid.to_string()})),
        ("procwho_findings", json!({})),
        ("procwho_top", json!({"sort":"not_in_ram","limit":2})),
        ("procwho_worktrees", json!({"limit":1,"measure_size":false})),
        ("procwho_whoami", json!({})),
    ];
    for (index, (name, arguments)) in calls.into_iter().enumerate() {
        let id = index + 3;
        send(
            &mut input,
            json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":name,"arguments":arguments}}),
        );
        let reply = read(&mut output);
        assert_eq!(reply["id"], id);
        assert_eq!(reply["result"]["isError"], false, "{name}: {reply}");
        let data = &reply["result"]["structuredContent"];
        assert!(data["read_only"]
            .as_str()
            .unwrap()
            .contains("does not stop or change"));
        assert!(data["truncated"].is_boolean());
        assert!(data["observed_at_unix"].is_i64());
        assert!(data["collect_ms"].is_u64());
        assert_eq!(data["schema_version"], 3);
        assert!(data["narrow_hint"].is_string());
        assert!(data["data"].is_object());
        assert!(reply["result"].to_string().len() <= 64 * 1024);
        let text: Value =
            serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(&text, data);
        match name {
            "procwho_status" => {
                assert!(data["data"]["system"].is_object());
                assert!(data["data"]["totals"].is_object());
                assert!(data["data"]["workloads"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|w| w.get("members").is_none()));
                assert!(data["data"]["workloads"].as_array().unwrap().len() <= 1);
            }
            "procwho_why" => {
                assert!(data["data"]["workload"]["evidence"].is_array());
                assert!(data["data"]["workload"]["members"].is_array());
                assert!(data["data"]["workload"]["members_total"].is_u64());
                assert!(data["data"]["workload"]["members_truncated"].is_boolean());
                assert!(data["data"]["suggested_rule"].is_object());
                assert!(data["data"]["suggested_rule"]["_matches_now"].is_u64());
            }
            "procwho_findings" => assert!(data["data"]["findings"].is_array()),
            "procwho_top" => {
                let rows = data["data"]["workloads"].as_array().unwrap();
                assert!(rows.len() <= 2);
                assert!(rows
                    .iter()
                    .all(|r| r["not_in_ram_bytes"].is_u64() && r["not_in_ram_estimated"] == true));
                assert!(data["data"]["not_in_ram_note"]
                    .as_str()
                    .unwrap()
                    .contains("estimate"));
                assert!(rows.windows(2).all(
                    |r| r[0]["not_in_ram_bytes"].as_u64() >= r[1]["not_in_ram_bytes"].as_u64()
                ));
            }
            "procwho_worktrees" => {
                assert!(data["data"]["worktrees"].as_array().unwrap().len() <= 1)
            }
            "procwho_whoami" => assert!(
                ["unknown", "identified"].contains(&data["data"]["identity"].as_str().unwrap())
            ),
            _ => unreachable!(),
        }
    }
    send(
        &mut input,
        json!({"jsonrpc":"2.0","id":20,"method":"unknown"}),
    );
    assert_eq!(read(&mut output)["error"]["code"], -32601);
    send(
        &mut input,
        json!({"jsonrpc":"2.0","id":21,"method":"tools/call","params":{"name":"procwho_top","arguments":{"limit":0}}}),
    );
    assert_eq!(read(&mut output)["error"]["code"], -32602);
    writeln!(input, "{{malformed").unwrap();
    input.flush().unwrap();
    let malformed = read(&mut output);
    assert_eq!(malformed["error"]["code"], -32700);
    assert!(malformed["id"].is_null());
    send(
        &mut input,
        json!({"jsonrpc":"2.0","id":"alive","method":"ping"}),
    );
    assert_eq!(read(&mut output)["id"], "alive");
    // Modern requests remain self-contained even on a connection with legacy state.
    let meta = json!({"io.modelcontextprotocol/protocolVersion":procwho_mcp::PROTOCOL_VERSION,
        "io.modelcontextprotocol/clientCapabilities":{}});
    send(
        &mut input,
        json!({"jsonrpc":"2.0","id":30,"method":"server/discover","params":{"_meta":meta}}),
    );
    let discovery = read(&mut output);
    assert_eq!(discovery["result"]["resultType"], "complete");
    assert_eq!(
        discovery["result"]["supportedVersions"][0],
        procwho_mcp::PROTOCOL_VERSION
    );
    send(
        &mut input,
        json!({"jsonrpc":"2.0","id":31,"method":"tools/call","params":{
            "_meta":meta,"name":"procwho_status","arguments":{"kind":"no-such-kind","limit":1}
        }}),
    );
    let modern = read(&mut output);
    assert_eq!(modern["result"]["resultType"], "complete");
    assert_eq!(modern["result"]["isError"], false);
    assert!(modern["result"]["structuredContent"]["data"]["workloads"]
        .as_array()
        .unwrap()
        .is_empty());
    drop(input); // EOF must exit without a signal.
    let mut rest = String::new();
    assert_eq!(output.read_line(&mut rest).unwrap(), 0);
    assert!(child.wait().unwrap().success());
    assert!(
        !state.join("history.db").exists() && !state.join("worktree-sizes.json").exists(),
        "read-only tools must not create history or a size cache by default"
    );
    std::fs::remove_dir_all(home).unwrap();
}
