use crate::output::{finish, MAX_ROWS};
use procwho_proc::{render, Snapshot, TopOpts};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

fn limit_schema(default: usize) -> Value {
    json!({"type":"integer","minimum":1,"maximum":MAX_ROWS,"default":default,"description":"Maximum returned rows (1–100)."})
}

fn string_schema(description: &str) -> Value {
    json!({"type":"string","minLength":1,"maxLength":2048,"description":description})
}

pub(crate) fn list() -> Vec<Value> {
    let definitions = [
        ("procwho_status", "Call procwho_whoami before recommending that anything be stopped. Start here for system memory, swap, load, totals by workload kind, largest workloads and review findings. kind filters workload rows by kind substring; system totals and findings remain global. Memory is footprint bytes; CPU is percent of one core. swap_used_bytes is used swap; swap_total_bytes is allocated so far, never a cap. mem_total_bytes is installed RAM.",
         json!({"limit":limit_schema(25),"kind":string_schema("Workload kind substring, for example agent-session or detached.")}), vec![]),
        ("procwho_why", "Call procwho_whoami before recommending that anything be stopped. Explain one live PID, workload key or name fragment: owner, attribution evidence, ancestry for a PID, a suggested_rule object to copy into registry.json, history if already recorded, and descriptive impact if stopped. Name fragments select the largest matching workload; use an exact key or PID to disambiguate.",
         json!({"target":string_schema("PID as a string, exact workload key, or workload name fragment.")}), vec!["target"]),
        ("procwho_findings", "Review possible leftovers and runaways with reasons, evidence basis and what is not proven. Labels are not stop recommendations. Trend claims require existing stored observations; this tool does not record samples.",
         json!({}), vec![]),
        ("procwho_top", "Call procwho_whoami before recommending that anything be stopped. Rank workloads by mem (footprint), cpu (percent of one core), or not_in_ram (max(footprint minus resident, 0)). not_in_ram is an estimate including compressed/nonresident pages, not measured per-workload swap. Use grep to narrow by a case-insensitive name regex.",
         json!({"sort":{"type":"string","enum":["mem","cpu","not_in_ram"],"default":"mem"},"limit":limit_schema(25),"grep":string_schema("Case-insensitive regular expression matching the workload name.")}), vec![]),
        ("procwho_worktrees", "Inventory linked git worktrees with verdicts, reasons, live use and merge facts as of each repo's last fetch. Does not fetch or contact GitHub. May remember up to 500 live-agent repository roots in procwho-owned state. Filter by verdict or repo name substring. measure_size defaults false: a cold disk size scan can take a minute; true may update procwho's own size cache. Verdicts are review labels, not deletion instructions.",
         json!({"limit":limit_schema(40),"verdict":{"type":"string","enum":procwho_wt::Verdict::ALL.iter().map(|v| v.as_str()).collect::<Vec<_>>()},"repo":string_schema("Case-insensitive repository name substring."),"measure_size":{"type":"boolean","default":false}}), vec![]),
        ("procwho_whoami", "Start here to identify the workload containing this MCP server's parent chain, with resource totals, attribution evidence and nested workloads. Session identity is unknown when only a shared host/tool set or weak evidence is available; never infer a specific thread from the server connection.",
         json!({}), vec![]),
    ];
    definitions.into_iter().map(|(name, description, properties, required)| {
        json!({
            "name":name,"description":description,
            "inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},
            "outputSchema":{
                "type":"object","required":["schema_version","observed_at_unix","collect_ms","trust","read_only","truncated","narrow_hint","data"],
                "properties":{
                    "schema_version":{"type":"integer"},"observed_at_unix":{"type":"integer"},"collect_ms":{"type":"integer"},
                    "trust":{"type":"string"},"read_only":{"type":"string"},"truncated":{"type":"boolean"},
                    "caution":{"type":"string","description":"Present when any returned workload has owner confidence below high."},
                    "narrow_hint":{"type":"string","description":"How to narrow the query if any rows or text were omitted."},
                    "data":{"type":"object","description":"Redacted library report, or an error object when isError is true."}
                },"additionalProperties":false
            },
            "annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}
        })
    }).collect()
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusArgs {
    limit: Option<usize>,
    kind: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WhyArgs {
    target: String,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct TopArgs {
    sort: Option<String>,
    limit: Option<usize>,
    grep: Option<String>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct WorktreeArgs {
    verdict: Option<String>,
    repo: Option<String>,
    limit: Option<usize>,
    measure_size: Option<bool>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}

fn parse<T: DeserializeOwned>(args: &Value) -> Result<T, String> {
    // Do not reflect untrusted values or serde's error text back into the protocol.
    if args
        .as_object()
        .is_some_and(|fields| fields.values().any(Value::is_null))
    {
        return Err("Tool arguments cannot be null; omit optional arguments instead".into());
    }
    serde_json::from_value(args.clone())
        .map_err(|_| "Invalid tool arguments; follow the tool's inputSchema".into())
}

fn limit(n: Option<usize>, default: usize) -> Result<usize, String> {
    let n = n.unwrap_or(default);
    if (1..=MAX_ROWS).contains(&n) {
        Ok(n)
    } else {
        Err("limit must be an integer from 1 to 100".into())
    }
}

fn check_string(s: &str) -> Result<(), String> {
    if s.is_empty() || s.chars().count() > 2048 {
        Err("String arguments must contain 1–2048 characters".into())
    } else {
        Ok(())
    }
}

fn optional_string(s: &Option<String>) -> Result<(), String> {
    s.as_deref().map(check_string).transpose().map(|_| ())
}

fn document(text: Result<String, String>) -> Result<Value, String> {
    text.and_then(|s| serde_json::from_str(&s).map_err(|_| "Library returned invalid JSON".into()))
}

// The CLI explains all members; MCP keeps only the largest 50.
fn bound_why_members(mut data: Value) -> Value {
    let w = &mut data["workload"];
    let rows = w["members"].as_array_mut().expect("why includes members");
    let total = rows.len();
    rows.truncate(50);
    w["members_total"] = json!(total);
    w["members_truncated"] = json!(total > 50);
    data
}

fn result(data: Result<Value, String>, truncated: bool, hint: &str) -> Value {
    match data {
        Ok(data) => finish(data, truncated, hint, false),
        Err(message) => finish(json!({"error":message}), false, hint, true),
    }
}

pub(crate) fn call(params: &Value) -> Result<Value, String> {
    let started = std::time::Instant::now();
    let observed = procwho_proc::sys::now() as i64;
    let mut result = call_inner(params)?;
    let structured = &mut result["structuredContent"];
    structured["observed_at_unix"] = structured["data"]
        .get("observed_at_unix")
        .cloned()
        .unwrap_or(json!(observed));
    structured["collect_ms"] = json!(started.elapsed().as_millis() as u64);
    result["content"][0]["text"] = json!(result["structuredContent"].to_string());
    Ok(result)
}

fn call_inner(params: &Value) -> Result<Value, String> {
    let name = params["name"].as_str().ok_or("tools/call requires name")?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or_else(|| json!({}));
    if !args.is_object() {
        return Err("arguments must be an object".into());
    }
    match name {
        "procwho_status" => {
            let a: StatusArgs = parse(&args)?;
            let n = limit(a.limit, 25)?;
            optional_string(&a.kind)?;
            let snap = Snapshot::collect();
            let rates = snap.cpu_rates(0.2);
            let fs = procwho_proc::findings(&snap);
            let mut report = procwho_proc::status::build(&snap, &rates, &fs, 0);
            report
                .workloads
                .retain(|w| a.kind.as_ref().is_none_or(|k| w.kind.contains(k)));
            let matched = report.workloads.len();
            report.workloads.truncate(n);
            let mut data = serde_json::to_value(report).map_err(|_| "Cannot serialize status")?;
            data["workloads_matched"] = json!(matched);
            Ok(finish(
                data,
                matched > n,
                "Narrow with kind; use procwho_why for a specific workload.",
                false,
            ))
        }
        "procwho_why" => {
            let a: WhyArgs = parse(&args)?;
            check_string(&a.target)?;
            Ok(result(
                document(procwho_proc::why(&a.target, true)).map(bound_why_members),
                false,
                "Use an exact workload key or PID.",
            ))
        }
        "procwho_findings" => {
            let _: EmptyArgs = parse(&args)?;
            Ok(result(
                document(procwho_proc::findings_cmd(true)),
                false,
                "Use procwho_why with a finding's workload key for focused evidence.",
            ))
        }
        "procwho_top" => {
            let a: TopArgs = parse(&args)?;
            let n = limit(a.limit, 25)?;
            optional_string(&a.grep)?;
            let sort = a.sort.as_deref().unwrap_or("mem");
            if !["mem", "cpu", "not_in_ram"].contains(&sort) {
                return Err("sort must be mem, cpu or not_in_ram".into());
            }
            if let Some(grep) = &a.grep {
                regex::RegexBuilder::new(grep)
                    .case_insensitive(true)
                    .build()
                    .map_err(|_| "grep must be a valid regex")?;
            }
            let snap = Snapshot::collect();
            let rates = snap.cpu_rates(0.2);
            let opts = TopOpts {
                n,
                sort: if sort == "cpu" {
                    procwho_proc::SortKey::Cpu
                } else {
                    procwho_proc::SortKey::Mem
                },
                grep: a.grep,
                ..TopOpts::default()
            };
            let mut order = render::select_workloads(&snap, &rates, &opts)?;
            if sort == "not_in_ram" {
                order.sort_by_key(|&i| {
                    std::cmp::Reverse(snap.workloads[i].fp.saturating_sub(snap.workloads[i].rss))
                });
            }
            let rows: Vec<_> = order
                .iter()
                .take(n)
                .map(|&i| {
                    let mut row =
                        serde_json::to_value(render::workload_view(&snap, i, Some(&rates), 0))
                            .expect("serializable workload");
                    row["not_in_ram_bytes"] =
                        json!(snap.workloads[i].fp.saturating_sub(snap.workloads[i].rss));
                    row
                })
                .collect();
            Ok(finish(
                json!({"accounting":"exclusive","sort":sort,"cpu_interval_s":rates.interval,"workloads_matched":order.len(),"workloads":rows,
                "not_in_ram_note":render::NOT_IN_RAM_NOTE,"footprint_note":render::FOOTPRINT_NOTE}),
                order.len() > n,
                "Narrow with grep or use procwho_why for one workload.",
                false,
            ))
        }
        "procwho_worktrees" => {
            let a: WorktreeArgs = parse(&args)?;
            let n = limit(a.limit, 40)?;
            optional_string(&a.repo)?;
            let verdict = a
                .verdict
                .as_deref()
                .map(|s| procwho_wt::Verdict::parse(s).ok_or("Unknown verdict"))
                .transpose()?;
            let snap = Snapshot::collect();
            let live: Vec<_> = snap
                .live_uses()
                .into_iter()
                .map(|u| procwho_wt::LiveUse {
                    pid: u.pid,
                    cwd: u.cwd,
                    workload: u.workload,
                    kind: u.kind,
                    agent_related: u.agent_related,
                })
                .collect();
            let config =
                procwho_proc::config::LoadedConfig::from_env(&procwho_wt::discover::home());
            let mut defaults = procwho_wt::Options::default();
            if let Some(roots) = &config.values.worktree_roots {
                defaults.roots = roots.iter().map(|p| (p.clone(), 3)).collect();
            }
            if let Some(days) = config.values.stale_days {
                defaults.stale_days = days;
            }
            let opts = procwho_wt::Options {
                size: a.measure_size.unwrap_or(false),
                verdict,
                repo: a.repo,
                pr: false,
                ..defaults
            };
            let mut report = procwho_wt::scan(&opts, Some(&live));
            report.notes.extend(config.notes);
            let matched = report.worktrees.len();
            report.worktrees.truncate(n);
            let mut data =
                serde_json::to_value(report).map_err(|_| "Cannot serialize worktree report")?;
            data["worktrees_matched"] = json!(matched);
            Ok(finish(
                data,
                matched > n,
                "Narrow with repo and verdict; reduce limit for smaller responses.",
                false,
            ))
        }
        "procwho_whoami" => {
            let _: EmptyArgs = parse(&args)?;
            Ok(whoami())
        }
        _ => Err("Unknown tool".into()),
    }
}

fn whoami() -> Value {
    let snap = Snapshot::collect();
    let chain = snap.chain(std::process::id() as i32);
    let owner_chain = snap.owner_chain(std::process::id() as i32);
    let candidates: Vec<_> = owner_chain
        .iter()
        .filter_map(|&pid| snap.proc_by_pid(pid))
        .collect();
    let session = candidates.iter().find(|p| {
        let w = snap.workload_of(p);
        w.kind == "agent-session" && ["high", "exact"].contains(&w.confidence.as_str())
    });
    let owner = session.copied().or_else(|| {
        candidates
            .iter()
            .find(|p| snap.workload_of(p).kind.starts_with("agent-"))
            .copied()
    });
    let rates = snap.cpu_rates(0.2);
    let mut data = json!({"identity":"unknown","reason":"No high-confidence agent session in this server's parent chain; process ancestry cannot identify a specific thread.","parent_chain_pids":chain,"workload":null,"nested_workloads":[]});
    if let Some(p) = owner {
        let wi = p.wl;
        data["workload"] = serde_json::to_value(render::workload_view(&snap, wi, Some(&rates), 0))
            .expect("serializable workload");
        data["totals_inclusive"] =
            serde_json::to_value(render::inclusive_totals(&snap, wi)).expect("serializable totals");
        data["nested_workloads"] = json!(snap
            .children_of(wi)
            .into_iter()
            .map(|i| render::workload_view(&snap, i, Some(&rates), 0))
            .collect::<Vec<_>>());
        if session.is_some() {
            data["identity"] = json!("identified");
            data["reason"] =
                json!("High-confidence session attribution in the MCP server's parent chain.");
        }
    }
    data["cpu_interval_s"] = json!(rates.interval);
    data["observed_at_unix"] = json!(snap.ts as i64);
    data["connection_scope"] = json!("This stdio connection only; unknown session identity does not mean safe to stop. Re-read and match PID plus instance before external action. Truncated dependencies are incomplete.");
    data["protected_dependencies"] = json!(crate::protection::dependencies(
        &snap,
        std::process::id() as i32
    ));
    finish(
        data,
        false,
        "Use procwho_why with a returned workload key or parent PID.",
        false,
    )
}

#[cfg(test)]
mod member_tests {
    use super::*;
    #[test]
    fn why_byte_budget_keeps_member_metadata_accurate() {
        let rows: Vec<_> = (0..60).map(|pid| json!({"pid":pid,"name":"word ".repeat(200),"command":"x ".repeat(150),"footprint_bytes":60-pid})).collect();
        let bounded = bound_why_members(json!({"workload":{"members":rows}}));
        let result = finish(bounded, false, "Narrow.", false);
        let w = &result["structuredContent"]["data"]["workload"];
        assert!(result.to_string().len() <= crate::output::MAX_RESULT_BYTES);
        assert!(w["members"].as_array().unwrap().len() < 50);
        assert_eq!(w["members_total"], 60);
        assert_eq!(w["members_truncated"], true);
    }

    #[test]
    fn why_caps_members_and_preserves_total() {
        for total in [0, 20, 50, 51, 200] {
            let rows: Vec<_> = (0..total)
                .map(|pid| json!({"pid":pid,"footprint_bytes":total-pid}))
                .collect();
            let bounded = bound_why_members(json!({"workload":{"members":rows}}));
            let result = finish(bounded, false, "Narrow.", false);
            let w = &result["structuredContent"]["data"]["workload"];
            assert_eq!(w["members"].as_array().unwrap().len(), total.min(50));
            assert_eq!(w["members_total"], total);
            assert_eq!(w["members_truncated"], total > 50);
        }
    }
}
