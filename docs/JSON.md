# JSON output

## status (JSON schema v3)

`procwho status --json [--members] [--limit N] [--interval S]` takes one process snapshot and one CPU read
and prints everything a main screen needs, so a GUI does not run `top` and `findings`
separately. `--limit` caps `workloads` (default 80, `0` = all); `--interval` is the CPU read
interval (default 1.0 s). Without `--json` it prints a short summary. System numbers come from
Mach VM statistics, sysctl, `getloadavg` and `statfs` (home directory), with no subprocess.

```
{ "schema_version": 3, "observed_at_unix": i64, "generated_unix": i64, "collect_ms": int, "cpu_interval_s": f64,
  "system":  { "ncpu", "mem_total_bytes", "mem_used_bytes", "mem_available_bytes", "mem_compressed_bytes", "mem_wired_bytes", "memory_pressure", "swap_used_bytes", "swap_total_bytes", "swap_disk_free_bytes", "swap_disk_total_bytes", "load1",
               "disk_free_bytes", "disk_total_bytes" },
  "totals":  { "accounting": "exclusive", "processes", "workloads", "footprint_bytes", "agent_bytes", "agent_associated_bytes", "agent_associated_medium_bytes", "unreadable_processes" },
  "by_kind": [ { "accounting": "exclusive", "kind", "workloads", "processes", "footprint_bytes", "cpu_pct" } ],
  "workloads": [ { "accounting": "exclusive", "key", "name", "kind", "confidence", "processes", "footprint_bytes",
                   "rss_bytes", "cpu_pct", "age_seconds" (int|null), "ports": [str],
                   "inside" (str|null), "session_id" (str|null), "project_path" (str|null),
                   "root_pid" (int|null), "bundle_path" (str|null), "agent_link" (object|null),
                   "members" (array, only with --members) } ],
  "findings": [ { "severity", "title", "workload_key" (str|null), "reasons": [str],
                  "not_proven": [str], "basis" } ],
  "sources": { "launchctl": bool, "lsof_ports": bool, "tmux": bool,
               "label_providers": str, "notes": [str] } }
```

`by_kind` and `workloads` are sorted by footprint, largest first. `by_kind` always covers every
workload, so its sums equal `totals` even when `--limit` cuts `workloads`. `root_pid` is the
largest-footprint root process of the workload (`null` if it has none), and
`procwho why <root_pid>` or `procwho why <key>` resolves to the same workload. Strings that
come from processes are redacted and stripped of control characters.

## Worktrees (schema v3)

JSON (`--json`, `schema_version` 3). Top level: `schema_version`, `generated_at`, `totals[]`
(per verdict: `count`, `size_bytes`, `rebuildable_bytes`, `size_unknown`), `repos[]`
(`default_branch`, `fetched_at`, `github_repo`, `notes`), `worktrees[]`, `notes[]`. Stable
per-worktree keys: `path`, `repo`, `branch`, `head`, `verdict`, `flags`, `reasons`, `notes`,
`size_bytes`, `rebuildable_bytes`, `last_activity_unix`, `idle_secs`, `in_use`, `merged`,
`content_merged`, `pr_merged` (`{number, merged_at, head_matches_tip}`), `unpushed_commits`,
`tracked_changes`, `untracked_files`, `locked`, `upstream_gone`, `default_branch`. Unknown is
`null`, never `false` or `0`.

See [worktree verdicts and limits](WORKTREES.md). MCP wraps bounded tool data; see
[Agents and MCP](AGENTS.md) for its separate result envelope.

## Additive estimate metadata

Estimate metadata remains available. Workload rows in `status`,
`top`, `why` and `export` also expose `not_in_ram_bytes` and
`not_in_ram_estimated: true`. This is footprint minus resident memory, floored
at zero, not measured per-process swap. `not_in_ram_note` explains the estimate
in status and top reports. Workload views, status/top reports and findings
carry `footprint_note`: sums can count shared memory more than once.

Worktree reports add `searched_folders` (configured roots and agent containers),
`reclaimable_estimated: true` and `reclaimable_note`. Reclaimable disk space is
an upper bound for measured merged-idle worktrees; APFS clones can free less,
and unmeasured sizes are excluded. Raw verdict values are unchanged.

MCP structured results add optional `caution` at the envelope level whenever
any returned workload has confidence below high. It is present in the JSON text
copy too. Exact and high confidence establish attribution, not safety to stop.

## CPU sampling (workload views, schema v3)

`top`, `why` and `export` carry `schema_version: 3`. Workload views carry
`cpu_sample_state`, `cpu_pct` and `cpu_interval_s`. `why` does not read a CPU
interval: it returns `"not_sampled"`, `null`, `null`. A sampled idle workload
returns `"sampled"`, `0.0` and the actual interval. Historical averages are separate.
The app detail uses the latest matching status sample and states its age.

## Accounting

Status totals, by-kind rows, workload rows, top/export totals and workload views
use `accounting: "exclusive"`: each process contributes to one
workload. `why.totals_inclusive` and `whoami.data.totals_inclusive` share the
named object `{accounting: "inclusive", nested_workloads, processes, footprint_bytes}`.
These totals include descendants; never add them to nested rows.

## Observation metadata

Every CLI JSON document and every MCP tool result includes `observed_at_unix`
(Unix seconds at collection start) and `collect_ms` (collection wall time in
milliseconds). Independent calls are independent observations. Status schema v3
keeps `generated_unix` as an alias. Worktrees schema v3 keeps `generated_at`.
`findings --json` is now `{schema_version: 2, observed_at_unix, collect_ms,
findings: [...], footprint_note}`; `unknown --json` uses schema v3 with the same observation fields and
`workloads: [...]`. Top, why and export documents use schema v3; config uses schema v2.
The MCP envelope is schema v3; its duration includes the whole tool call.

Worktrees schema v3 adds `worktrees[].size_measured_unix`: the time the size
walk finished, preserved from the size cache on a hit, or `null` when no size
is available. `generated_at` is scan time, not size measurement time.

### Why example (schema v3)

```json
{
  "schema_version": 3,
  "observed_at_unix": 1794825000,
  "collect_ms": 120,
  "workload": {
    "key": "toolset:100:200",
    "name": "api-server · Codex tool set",
    "kind": "agent-toolset",
    "confidence": "low",
    "accounting": "exclusive",
    "cpu_pct": null,
    "cpu_sample_state": "not_sampled",
    "cpu_interval_s": null
  },
  "totals_inclusive": {
    "accounting": "inclusive",
    "nested_workloads": 0,
    "processes": 4,
    "footprint_bytes": 1073741824
  }
}
```

The example omits unrelated evidence/process fields. `confidence` always means
identity confidence, never severity. MCP's `protected_dependencies` and `trust`
fields are documented in [Agents and MCP](AGENTS.md).

## Status schema v3

Status v3 adds nullable system byte counts `mem_used_bytes`, `mem_available_bytes`,
`mem_compressed_bytes`, and `mem_wired_bytes`. These measure physical RAM, not
footprint sums. See [the formula](ARCHITECTURE.md#physical-memory). MCP status uses
the same system object. Unreadable VM counters are null.

`system.memory_pressure` is `"normal" | "warning" | "critical" | null`.

Status, top, why and export workload rows use schema v3 and add `agent_link`:
null or `{relation, via, confidence, agent_product}`. Relations are `agent`,
`agent_host_app`, or `launched_by_agent`; confidence is high or medium. Identity
confidence stays independent. Links supplement exclusive ownership; no PID moves
between workloads. Host apps come from the agent table; Zed requires a nested
agent. Launching links require every root to have recorded original-parent or
responsible-process evidence. Simulators and launchd services are excluded.

`status.totals.agent_bytes` sums relation agent. `agent_associated_bytes` sums all
three relations, and `agent_associated_medium_bytes` is its medium-confidence
subset. Each uses exclusive workload bytes, once per process, including when
sessions nest inside host apps. Totals cover the full snapshot before row limits.

`why.suggested_rule` (also MCP `procwho_why.data.suggested_rule`) is a valid
registry rule object with `name: "..."`, a regex matcher, optional owner/impact,
and removable `_matches_now` process count. `why --rule` prints only that object.
The MCP envelope is now schema v3; worktrees remain schema v3, and findings/config
wrappers remain schema v2. See [owner rules](CONFIGURATION.md#owner-rules).

Worktree repo and worktree rows include `discovered_via`: `root`, `container`, `live_process`,
or `remembered`. `repos[]` includes all discovered repositories, including those with no linked
worktrees. First-source precedence is live process, container, root, remembered.
`worktrees --suggest-roots --json` returns `candidates[]` (`path`, `exists`, `protected`,
`repo_count`) and `automatic_folders[]`. Protected candidates are assumed to exist and never
probed; absent, protected or time-limited counts are null. This is an additive schema-v3 change.

Status `swap_used_bytes` measures use; `swap_total_bytes` is **allocated so far**, never a cap.
`mem_total_bytes` is installed RAM. `swap_disk_free_bytes` and `swap_disk_total_bytes` describe
the swap volume, independently of the home-volume disk fields. A failed disk read is zero in
the CLI; a zero total is treated as unknown by the app.

## Workload members (additive schema v3)

`status --json --members` adds `members` to every returned workload. Without
`--members`, the key is absent. `why --json` always includes `workload.members`.
Each list contains exactly the processes counted in that workload's `processes`
field, excluding processes owned by nested workloads. Members are sorted by
`footprint_bytes` descending, unavailable values last, then by PID ascending.

```json
{"pid": 123, "ppid": 1, "name": "node", "command": "node server.js",
 "footprint_bytes": 1048576, "cpu_pct": null, "age_seconds": 60}
```

`name` is the executable base name (or the process name if its path is unavailable).
`command` is redacted before truncation to at most 300 Unicode characters.
Unavailable footprint and age values are null. CPU is null when no CPU interval
was sampled or the process counters were unavailable; status members use the same
interval as workload CPU. Why does not
sample CPU, so its members have null CPU values.

MCP `procwho_why.data.workload` includes the largest 50 members at most,
`members_total` (the full exclusive count), and `members_truncated` (whether any
were omitted). Its result byte budget may reduce the list further; the count
and truncation flag remain accurate. MCP `procwho_status` does not include members.
These optional, additive fields retain schema version 3.

Status and why workload objects also include `bundle_path: string | null`.
It is the existing outermost `.app` directory containing the workload's root
executable, including when helpers live in nested bundles. The root is the same
largest-footprint root used for `root_pid`. Paths are redacted before output.
For a verified macOS code-sign clone, it is the installed bundle matched by the
clone ID and executable against `Contents/Info.plist`. A missing bundle, a non-app
executable or an unavailable local path yields null.
This is an additive schema-v3 field.

Ownership evidence, identity descriptions, impact descriptions and finding text use
complete sentences. `read_only` states that Procwho reads process information
without changing processes. Note text can change independently of JSON keys,
workload kinds and names. Every report string is redacted before JSON output;
process environments are never included.
