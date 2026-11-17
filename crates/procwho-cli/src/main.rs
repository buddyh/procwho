//! `procwho` command line: argument parsing and dispatch only.
//!
//! Read-only. No subcommand signals, stops, restarts or edits observed workloads.

mod config_command;

use procwho_proc::{SortKey, TopOpts};
use std::io::Write;
use std::process::ExitCode;

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "procwho - read-only \"which work owns this process?\" for macOS

USAGE
    procwho [COMMAND] [OPTIONS]        (no command = top)

COMMANDS
    top        workloads with resource totals (default)
    status     one-shot overview for a GUI: system, totals, by kind, workloads, findings
    why        explain a PID or workload: owner, evidence, impact, reveal
    tree       process tree of one workload
    findings   possible leftovers / runaways, with reasons
    unknown    workloads with weak or no ownership evidence
    sample     store one observation in the bounded history
    watch      sample repeatedly in the foreground (no daemon)
    history    stored trend of one workload
    export     redacted JSON inventory
    mcp        read-only MCP server over stdio
    config     resolved configuration and its sources
    worktrees  (alias wt) git worktrees: size, status, merged or stale, live use

OPTIONS
    -h, --help       show help (also per command)
    -V, --version    show version

UNITS
    MEM   phys_footprint, what Activity Monitor calls \"Memory\" (includes compressed pages)
    CPU%  percent of ONE core averaged over the stated interval (N cores = N * 100% max)

ENVIRONMENT
    PROCWHO_STATE      state dir (default ~/.local/state/procwho)
    PROCWHO_CONFIG     settings (default ~/.config/procwho/config.json)
    PROCWHO_REGISTRY   owner rules (default ~/.config/procwho/registry.json)

procwho never stops, signals or changes observed workloads. Writes are limited to its own
state, isolated temporary git objects, and explicitly requested exports.
";

fn help_for(cmd: &str) -> Option<&'static str> {
    Some(match cmd {
        "config" => config_command::HELP,
        "top" => "procwho top [-n N] [--sort mem|cpu|procs] [--kind K] [--grep RE] [--interval S] [--json]

Workloads with resource totals. Defaults: -n 35, --sort mem, --interval 1.0.
--kind matches a substring of the workload kind (app, service, agent-session, agent-host,
agent-toolset, agent-command, detached, watcher, terminal-pane, simulator, system, system-opaque,
unknown).
--grep is a case-insensitive regex on the workload name. --json prints machine-readable output.
",
        "status" => "procwho status [--json] [--members] [--limit N] [--interval S]

One snapshot and one CPU read, then everything a main screen needs: system numbers (CPUs,
memory, swap, load, disk of the home directory), totals, per-kind totals, the largest workloads
and the findings. --json prints schema v3 (see docs/JSON.md); without it
a short summary is printed. --members includes every process in each JSON workload.
--limit N lists the N largest workloads (default 80; 0 = all).
--interval S is the CPU read interval (default 1.0). Read-only.
",
        "why" => "procwho why <pid|name|workload-key> [--json] [--rule]

Explains one process or workload: owner, evidence lines, ancestry, impact if stopped (descriptive
only), history and read-only commands to look at it yourself.
",
        "tree" => "procwho tree <pid|name> [-n N]

Process tree of one workload. -n limits root processes shown (default 25).
",
        "findings" => "procwho findings [--json]

Possible leftovers and runaways with reasons and what remains unknown. Trend claims need at least
three stored observations (run `procwho watch` or `procwho sample` first).
",
        "unknown" => "procwho unknown [-n N] [--json]

Workloads with weak or missing ownership evidence, worst first. Default -n 40.
",
        "sample" => "procwho sample

Store one observation in the history database (72 hour retention).
",
        "watch" => "procwho watch [--interval S] [--count N]

Sample in the foreground every S seconds (default 60). --count 0 (default) runs until Ctrl-C.
",
        "history" => "procwho history <pid|name> [--hours H] [-n N]

Stored trend of one workload. Defaults: --hours 24, -n 30.
",
        "export" => "procwho export [--out FILE] [--per N]

Redacted JSON inventory (argv redacted, no environments, no transcripts). --per sets how many
sample processes to list per workload (default 5).
",
        _ => return None,
    })
}

/// Parsed arguments for one command: flags with values, switches and positionals.
struct Args {
    pos: Vec<String>,
    flags: Vec<(String, String)>,
    switches: Vec<String>,
}

impl Args {
    fn parse(raw: &[String], value_flags: &[&str], switch_flags: &[&str]) -> Result<Args, String> {
        let (mut pos, mut flags, mut switches) = (Vec::new(), Vec::new(), Vec::new());
        let mut i = 0;
        while i < raw.len() {
            let a = &raw[i];
            if a.starts_with('-') && a.len() > 1 {
                let (name, inline) = match a.split_once('=') {
                    Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
                    _ => (a.clone(), None),
                };
                if value_flags.contains(&name.as_str()) {
                    let v = match inline {
                        Some(v) => v,
                        None => {
                            i += 1;
                            raw.get(i)
                                .cloned()
                                .ok_or(format!("option {name} needs a value"))?
                        }
                    };
                    flags.push((name, v));
                } else if switch_flags.contains(&name.as_str()) && inline.is_none() {
                    switches.push(name);
                } else {
                    return Err(format!("unknown option '{a}'"));
                }
            } else {
                pos.push(a.clone());
            }
            i += 1;
        }
        Ok(Args {
            pos,
            flags,
            switches,
        })
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.flags
            .iter()
            .rev()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    fn num<T: std::str::FromStr>(&self, name: &str, default: T) -> Result<T, String> {
        match self.value(name) {
            None => Ok(default),
            Some(v) => v
                .parse()
                .map_err(|_| format!("option {name}: invalid number '{v}'")),
        }
    }

    fn has(&self, name: &str) -> bool {
        self.switches.iter().any(|s| s == name)
    }

    fn target(&self) -> Result<&str, String> {
        match self.pos.as_slice() {
            [t] => Ok(t),
            [] => Err("missing <pid|name> argument".into()),
            _ => Err("too many arguments".into()),
        }
    }

    fn no_positionals(&self) -> Result<(), String> {
        if self.pos.is_empty() {
            Ok(())
        } else {
            Err(format!("unexpected argument '{}'", self.pos[0]))
        }
    }
}

enum Outcome {
    Text(String),
    Done,
}

fn run(cmd: &str, rest: &[String]) -> Result<Outcome, String> {
    let text = |r: Result<String, String>| r.map(Outcome::Text);
    match cmd {
        "config" => text(config_command::run(rest)),
        "top" => {
            let a = Args::parse(
                rest,
                &["-n", "--sort", "--kind", "--grep", "--interval"],
                &["--json"],
            )?;
            a.no_positionals()?;
            let sort = match a.value("--sort").unwrap_or("mem") {
                "mem" => SortKey::Mem,
                "cpu" => SortKey::Cpu,
                "procs" => SortKey::Procs,
                other => {
                    return Err(format!(
                        "--sort: invalid choice '{other}' (mem, cpu, procs)"
                    ))
                }
            };
            text(procwho_proc::top(&TopOpts {
                n: a.num("-n", 35usize)?,
                sort,
                kind: a.value("--kind").map(str::to_string),
                grep: a.value("--grep").map(str::to_string),
                interval: a.num("--interval", 1.0f64)?,
                json: a.has("--json"),
            }))
        }
        "status" => {
            let a = Args::parse(rest, &["--limit", "--interval"], &["--json", "--members"])?;
            a.no_positionals()?;
            text(procwho_proc::status(&procwho_proc::StatusOpts {
                members: a.has("--members"),
                limit: a.num("--limit", procwho_proc::status::DEFAULT_LIMIT)?,
                interval: a.num("--interval", 1.0f64)?,
                json: a.has("--json"),
            }))
        }
        "why" => {
            let a = Args::parse(rest, &[], &["--json", "--rule"])?;
            if a.has("--rule") {
                text(procwho_proc::render::why_rule(a.target()?))
            } else {
                text(procwho_proc::why(a.target()?, a.has("--json")))
            }
        }
        "tree" => {
            let a = Args::parse(rest, &["-n"], &[])?;
            text(procwho_proc::tree(a.target()?, a.num("-n", 25usize)?))
        }
        "findings" => {
            let a = Args::parse(rest, &[], &["--json"])?;
            a.no_positionals()?;
            text(procwho_proc::findings_cmd(a.has("--json")))
        }
        "unknown" => {
            let a = Args::parse(rest, &["-n"], &["--json"])?;
            a.no_positionals()?;
            text(procwho_proc::unknown(
                a.num("-n", 40usize)?,
                a.has("--json"),
            ))
        }
        "sample" => {
            let a = Args::parse(rest, &[], &[])?;
            a.no_positionals()?;
            text(procwho_proc::sample())
        }
        "watch" => {
            let a = Args::parse(rest, &["--interval", "--count"], &[])?;
            a.no_positionals()?;
            let interval = a.num("--interval", 60.0f64)?;
            let count = a.num("--count", 0u32)?;
            let stdout = std::io::stdout();
            procwho_proc::watch(interval, count, &mut |line| {
                let mut o = stdout.lock();
                if writeln!(o, "{line}").and_then(|_| o.flush()).is_err() {
                    std::process::exit(0); // closed pipe
                }
            })?;
            Ok(Outcome::Done)
        }
        "history" => {
            let a = Args::parse(rest, &["--hours", "-n"], &[])?;
            text(procwho_proc::history(
                a.target()?,
                a.num("--hours", 24.0f64)?,
                a.num("-n", 30usize)?,
            ))
        }
        "export" => {
            let a = Args::parse(rest, &["--out", "--per"], &[])?;
            a.no_positionals()?;
            text(procwho_proc::export(
                a.value("--out"),
                a.num("--per", 5usize)?,
            ))
        }
        _ => unreachable!("dispatch checked the command"),
    }
}

const WT_HELP: &str = "procwho worktrees [OPTIONS]        (alias: wt)

Every git worktree on the machine: size, rebuildable bytes, age since last activity, git status,
merged or stale, and whether a live process has its cwd inside it. Linked worktrees only unless
--all. Read-only: no fetch, prune, remove or checkout; every git call runs with fsmonitor and the
untracked cache off and --no-optional-locks. Squash/rebase merges are found with git merge-tree,
which writes only to a temp object dir under a sandbox that denies other writes. The only files
kept are the size cache (and the PR cache with --pr).

VERDICTS (review labels, in precedence order; not instructions)
    missing       directory gone (git calls it prunable)
    orphaned-dir  has a .git file, but its main repo does not list it or no longer exists
    in-use        a live process cwd is inside it
    has-work      tracked changes, untracked files, or commits on no remote whose changes are
                  not in the default branch
    merged-idle   merged, clean, idle, unlocked. Merged = tip contained in the default branch,
                  or all its changes already are (squash/rebase merge), or (--pr) a PR merged
                  at exactly this tip; all as of the last fetch
    merged-locked as merged-idle, but the worktree is locked (git worktree lock)
    stale         not merged, clean, idle, no activity for --stale-days
    active        none of the above
    unknown       a needed fact could not be determined (see reasons / notes)

OPTIONS
    --json               machine-readable report (all matching worktrees unless -n is given)
    -n N                 rows to show (default 40)
    --sort K             size (default), age, repo
    --verdict V          only this verdict
    --repo NAME          only repos whose name contains NAME
    --all                include main checkouts
    --no-size            skip disk usage
    --size-ttl MIN       reuse cached sizes younger than MIN minutes (default 30)
    --no-procs           skip the process snapshot (in-use unknown; nothing labelled idle)
    --pr                 ask GitHub which PRs were merged: one `gh pr list --state merged` per repo
                         with a GitHub remote (10 s timeout, cached 6 h). Off by default. Only a PR
                         merged at exactly the worktree's tip counts as merged
    --stale-days D       default from config, otherwise 7
    --jobs N             worker threads (default 4)
    --root PATH          search root for repos, depth 3 (repeatable; replaces the defaults
                         existing developer directories at depth 3; Documents/Desktop excluded)
    --container PATH     extra worktree container, depth 3 (repeatable; agent defaults still apply)
    --suggest-roots       JSON candidates with existence and depth-2 repository counts (2 s budget)
                         Documents/Desktop are never probed
    --forget-known        clear procwho's remembered repo list and exit
    --no-default-roots    scan only explicit --root paths and automatic discovery sources

FLAGS (facts, not verdicts): upstream-gone, locked, detached.

Merged / ahead data is only as fresh as each repo's last fetch. Sizes are allocated blocks; APFS
clones can make them larger than what removal would free. JSON has schema_version 3 (see docs/JSON.md).
";

fn worktrees(rest: &[String]) -> Result<String, String> {
    let a = Args::parse(
        rest,
        &[
            "-n",
            "--sort",
            "--verdict",
            "--repo",
            "--stale-days",
            "--jobs",
            "--size-ttl",
            "--root",
            "--container",
        ],
        &[
            "--json",
            "--no-size",
            "--no-procs",
            "--all",
            "--pr",
            "--suggest-roots",
            "--forget-known",
            "--no-default-roots",
            "--no-probe",
        ],
    )?;
    a.no_positionals()?;
    if a.has("--suggest-roots") {
        let suggestions = procwho_wt::discover::suggest_roots_with_budget(
            &procwho_wt::discover::home(),
            std::time::Duration::from_secs(if a.has("--no-probe") { 0 } else { 2 }),
        );
        return serde_json::to_string_pretty(&suggestions)
            .map(|s| s + "\n")
            .map_err(|e| e.to_string());
    }
    if a.has("--forget-known") {
        procwho_wt::known::forget_known(&procwho_wt::Options::default().state_dir)
            .map_err(|e| e.to_string())?;
        return Ok(if a.has("--json") {
            "{\"forgot_known\":true}\n".into()
        } else {
            "Forgot remembered repositories.\n".into()
        });
    }
    let sort = match a.value("--sort").unwrap_or("size") {
        "size" => procwho_wt::Sort::Size,
        "age" => procwho_wt::Sort::Age,
        "repo" => procwho_wt::Sort::Repo,
        other => {
            return Err(format!(
                "--sort: invalid choice '{other}' (size, age, repo)"
            ))
        }
    };
    let verdict = match a.value("--verdict") {
        None => None,
        Some(v) => Some(procwho_wt::Verdict::parse(v).ok_or_else(|| {
            let all: Vec<&str> = procwho_wt::Verdict::ALL
                .iter()
                .map(|v| v.as_str())
                .collect();
            format!("--verdict: invalid choice '{v}' ({})", all.join(", "))
        })?),
    };
    let all = |name: &'static str| -> Vec<(std::path::PathBuf, usize)> {
        a.flags
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, v)| (config_command::cli_path(v), 3))
            .collect()
    };
    let config = procwho_proc::config::LoadedConfig::from_env(&procwho_wt::discover::home());
    let d = config_command::worktree_defaults(&config);
    let roots = all("--root");
    let o = procwho_wt::Options {
        roots: if a.has("--no-default-roots") && roots.is_empty() {
            Vec::new()
        } else if roots.is_empty() {
            d.roots.clone()
        } else {
            roots
        },
        containers: d
            .containers
            .iter()
            .cloned()
            .chain(all("--container"))
            .collect(),
        include_main: a.has("--all"),
        size: !a.has("--no-size"),
        size_ttl_min: a.num("--size-ttl", 30u64)?,
        stale_days: a.num("--stale-days", d.stale_days)?,
        jobs: a.num("--jobs", 4usize)?.max(1),
        sort,
        verdict,
        repo: a.value("--repo").map(str::to_string),
        pr: a.has("--pr"),
        ..d
    };
    let live: Option<Vec<procwho_wt::LiveUse>> = (!a.has("--no-procs")).then(|| {
        procwho_proc::Snapshot::collect()
            .live_uses()
            .into_iter()
            .map(|u| procwho_wt::LiveUse {
                pid: u.pid,
                cwd: u.cwd,
                workload: u.workload,
                kind: u.kind,
                agent_related: u.agent_related,
            })
            .collect()
    });
    let mut report = procwho_wt::scan(&o, live.as_deref());
    report.notes.extend(config.notes);
    if a.has("--json") {
        if let Some(n) = a.value("-n") {
            let n: usize = n
                .parse()
                .map_err(|_| format!("option -n: invalid number '{n}'"))?;
            report.worktrees.truncate(n);
        }
        serde_json::to_string_pretty(&report)
            .map(|s| s + "\n")
            .map_err(|e| e.to_string())
    } else {
        Ok(procwho_wt::render(&report, a.num("-n", 40usize)?))
    }
}

const COMMANDS: [&str; 11] = [
    "config", "top", "status", "why", "tree", "findings", "unknown", "sample", "watch", "history",
    "export",
];

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let (cmd, rest): (String, Vec<String>) = match argv.first() {
        None => ("top".into(), Vec::new()),
        Some(first) if first.starts_with('-') => ("top".into(), argv.clone()),
        Some(first) => (first.clone(), argv[1..].to_vec()),
    };
    // Global flags are handled before any collection work.
    let wants_help = rest.iter().any(|a| a == "-h" || a == "--help");
    if cmd == "--version" || cmd == "-V" {
        println!("procwho {VERSION}");
        return ExitCode::SUCCESS;
    }
    if argv.first().is_some_and(|a| a == "-h" || a == "--help") || cmd == "help" {
        print!("{HELP}");
        return ExitCode::SUCCESS;
    }
    if cmd == "mcp" {
        return procwho_mcp::main(&rest);
    }
    if cmd == "worktrees" || cmd == "wt" {
        if wants_help {
            print!("{WT_HELP}");
            return ExitCode::SUCCESS;
        }
        return match worktrees(&rest) {
            Ok(t) => {
                let mut o = std::io::stdout().lock();
                let _ = o.write_all(t.as_bytes()); // closed pipe is fine
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("procwho worktrees: {e}\n\n{WT_HELP}");
                ExitCode::from(2)
            }
        };
    }
    if !COMMANDS.contains(&cmd.as_str()) {
        eprintln!("procwho: unknown command '{cmd}'\n\n{HELP}");
        return ExitCode::from(2);
    }
    if wants_help || rest.iter().any(|a| a == "--version" || a == "-V") {
        if wants_help {
            print!("{}", help_for(&cmd).unwrap_or(HELP));
        } else {
            println!("procwho {VERSION}");
        }
        return ExitCode::SUCCESS;
    }
    match run(&cmd, &rest) {
        Ok(Outcome::Text(t)) => {
            let mut o = std::io::stdout().lock();
            if o.write_all(t.as_bytes())
                .and_then(|_| {
                    if t.ends_with('\n') {
                        Ok(())
                    } else {
                        o.write_all(b"\n")
                    }
                })
                .is_err()
            {
                return ExitCode::SUCCESS; // closed pipe
            }
            ExitCode::SUCCESS
        }
        Ok(Outcome::Done) => ExitCode::SUCCESS,
        Err(e) => {
            // Usage errors are exit 2, runtime lookups exit 1 (like the prototype's sys.exit).
            let usage = e.starts_with("unknown option")
                || e.starts_with("option ")
                || e.starts_with("missing")
                || e.starts_with("too many")
                || e.starts_with("unexpected")
                || e.starts_with("--sort");
            if usage {
                eprintln!("procwho {cmd}: {e}\n\n{}", help_for(&cmd).unwrap_or(""));
                ExitCode::from(2)
            } else {
                eprintln!("{e}");
                ExitCode::from(1)
            }
        }
    }
}
