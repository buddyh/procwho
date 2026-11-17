# Architecture

procwho attributes visible macOS processes to named workloads. The Rust workspace separates
kernel reads, attribution, history and rendering (`procwho-proc`), git inventory (`procwho-wt`),
and CLI dispatch (`procwho-cli`). The app and MCP server consume those read-only interfaces.
No daemon or background sampling is required.

## Label rules

A process starts a workload when the first applicable rule below matches. Other processes
inherit the nearest ancestor's workload within the same ownership boundary. A uid change
or an unreadable-to-readable edge starts fresh attribution and records the boundary as
evidence. SSH/login shells form separate terminal workloads; physical ancestry stays visible
in `why` and MCP protected dependencies. Parent chains are cycle-checked and bounded. Each
process belongs to exactly one workload; ordinary totals count it once. `why` also gives an
inclusive total for an app or shared host and its nested workloads.

| Order | Evidence | Kind | Confidence |
|---|---|---|---|
| 1 | Process details cannot be read for its user | `system-opaque` | n/a; permission limit |
| 2 | Allowlisted `PROCWHO_OWNER` environment value | `declared` | declared |
| 3 | All specified regex fields of a registry rule match | rule's declared kind | declared |
| 4 | `launchd_sim` names a CoreSimulator device | `simulator` | exact device UDID; optional device name from plist |
| 5 | Apple launchd label or system executable path | `system` | exact/high depending on evidence |
| 6 | A non-application launchd label has this PID | `service` | exact label; provider name is display metadata |
| 7 | Headless browser and automation-profile flags | `automation-browser` or `detached` | medium |
| 8 | Agent table match; verified wrappers deduplicated | `agent-session`, `agent-host` or verified `agent-toolset` | see agent table |
| 9 | Direct child of a shared host | `agent-toolset` or `agent-command` | inferred or unknown session |
| 10 | tmux server or pane PID | `terminal` or `terminal-pane` | exact |
| 11 | App-bundle executable with launchd application label | `app` | exact |
| 12 | Parent is launchd and no earlier rule matches | fallback below | varies |

Simulator descendants inherit the device workload. Ordinary agent descendants inherit the
enclosing session; independent recognized agents can start nested workloads. Shared hosts
allow independent sessions and tool sets beneath them. A launchd label
is process ownership evidence even when no provider is configured. Providers only supply names
and purposes, never PIDs or confidence. Stale provider metadata is explicitly marked.

A macOS code-sign clone can also start an app workload with medium confidence.
The kernel path must use the macOS clone layout. The first command argument must
name an existing app executable whose bundle ID, executable name and relative path
match that clone. Bundle metadata reads are local, bounded and cached per process
for attribution. A command path alone does not establish app ownership.

For unmatched launchd children, git fsmonitor daemons become `watcher`; a shared tmux terminal
joins its pane; app-bundle helpers join their app; Apple paths join `system`; project cwd or
responsible-app evidence yields `detached`; otherwise the owner is `unknown`. PPID 1 alone
does not prove a process was abandoned. Original parent instance IDs of 0 or 1 prove nothing.

## Agent table

`crates/procwho-proc/src/agents.rs` owns the schema, compiled matcher, and bounded argv
parser. Product rules live in `src/agents/*.json`; positive and negative synthetic fixtures
live beside them. [Supported agents](SUPPORTED-AGENTS.md) lists recognized installation
shapes, roles, sources, verification boundaries, and deliberate omissions.

Executable names, native-path patterns, and interpreter-launcher patterns are alternative
recognition seeds. Required argv and ancestor predicates are AND guards. Arbitrary prompt
text and interpreter option values are not searched for agent names. Unknown interpreter
switches stop script discovery; unknown agent options stop mode/ID parsing conservatively.
Commands are matched at command positions, including compound forms such as `agent stdio`.
Flag/value pairs such as `--mode rpc` and equals forms share the same parser. Product-specific
headless flags avoid treating Cline plan mode or Claude stream transport as generic print or
shared-host modes. Codex global `-c` options are consumed before embedded server commands.
Known management, help, remote-client, and inventory actions are excluded.

Built-ins compile once in the procwho runtime; custom `extra_agents` use the same schema and compile
once per snapshot. Native bundle/package paths give high confidence. Name-plus-command/module
guards give medium confidence; bare ambiguous short names are excluded. Exact confidence
requires a validated explicit session ID in argv or an allowlisted T3 thread ID beneath a T3
application ancestor. tmux and cwd give context, not exact provider identity. T3 host-thread
IDs stay separate from provider-session IDs. No new environment names are collected.

Rules can declare a shared host unconditionally or through a command/option predicate.
Codex app-server/exec-server, documented server/ACP modes, T3, and verified bundled backend
services remain hosts with unresolved per-session identity and cwd. Cursor and Zed editor
roots require a separately recognized provider descendant before becoming hosts. Their
ordinary UI/helper children inherit the host rather than being asserted to be agent commands.
Other IDE extension hosts remain unclassified when a specific worker signature is unknown.
App ancestry records provider origin, including Conductor, without inventing provider IDs.

Only verified launcher/native pairs are deduplicated. Independently launched providers and
same-provider runtimes can form separate nested workloads. Recognized sessions own their
ordinary child commands/helpers through bounded ancestry; a generic editor ancestor alone
is insufficient. Direct children of a proven provider server get unresolved command ownership.
Codex code-mode and ChatGPT computer-use helpers have explicit tool-set roles and require
a verified Codex executable ancestor plus their product-specific package/bundle path. Codex bursts of at least four helpers within four seconds
can form an unresolved tool set.
Timing never supplies a session ID or project: rollout filename proximity is not a PID join.
Normal snapshots no longer scan Codex rollouts for attribution; the existing bounded metadata
reader remains available but is not used to guess a running thread.

No agent rule performs per-process file reads, launches subprocesses, inspects conversations,
or queries live host APIs. Recognition is computed once per process and reused through the
bounded ancestry pass. Existing process/workload JSON fields and types are retained; new
family names are data. `procwho config --example` demonstrates guarded custom rules.

## Data sources

| Data | Source | Limits |
|---|---|---|
| PIDs, parent, process group, tty, start, zombie state | libproc BSD info | Full form when readable; short form otherwise |
| Process and original-parent instance IDs | `PROC_PIDUNIQIDENTIFIERINFO` | Identity is rechecked after collection to avoid PID reuse |
| argv and four environment values | `sysctl KERN_PROCARGS2` | Same-user permissions; environment is never dumped |
| cwd | `PROC_PIDVNODEPATHINFO` | Full-size responses only |
| Footprint, RSS and CPU counters | `proc_pid_rusage` v4 | Failed reads remain unavailable |
| Responsible app | Optional private responsibility symbol via `dlsym` | Supporting evidence only; missing symbol is safe |
| launchd label and policy | `launchctl list`, job plist | Label and plist availability are separate facts |
| Listening/established TCP and fsmonitor directories | `lsof` | Bounded helpers; unavailable means unknown, never zero evidence |
| tmux panes | `tmux list-panes -a` | Optional; no server and missing tool are nonfatal |
| Service display names | Configured label providers | No default command; 2 seconds/256 KiB each, read-only/no-network sandbox |
| Codex rollout utility (not used by snapshot attribution) | Filename parsing and existing bounded metadata reader | No timing-based thread assignment; no transcript messages |
| Project root | Nearest `.git` directory or bounded pointer file | Cached per snapshot; remote filesystems are skipped |
| Worktrees and git facts | Bounded local git commands | No fetch; merge facts are only as fresh as local refs |

Helpers run independently where possible. Stdout is capped; each child has its own process
group. Timeout cleanup affects only that helper's group, never an observed user workload.
Socket connection evidence is read once per snapshot when needed by findings. Config errors,
unknown keys and unavailable sources are exposed through source notes. The original JSON
source compatibility key remains string-valued for older clients; new clients should use
`sources.label_providers` and `sources.notes`. Existing status/why/worktree fields and schema
versions are retained; new diagnostic fields are additive.

Worktree roots are existing conventional developer folders at depth 3 plus home at depth 1.
Existing T3 Code, Codex and Claude Code containers retain their documented/observed layouts;
repo-local Claude worktrees are found when visiting the repository. Config replaces conventional
roots; CLI roots override config; containers remain independent. Walkers do not follow directory
symlinks or materialize iCloud files. Git merge-tree writes only isolated temporary objects under
a sandbox that denies other writes. Verdicts remain review labels, never deletion instructions.

## Units and portability

Memory means `phys_footprint`, including compressed/nonresident pages. RSS means resident pages;
shared pages can be counted in more than one process's RSS. Sums are estimates, not a measurement
of reclaimable RAM. CPU is percent of one core over the stated interval; N logical CPUs allow
N × 100%. Cumulative CPU uses kernel counters converted through the runtime Mach timebase,
with integer arithmetic and saturation. No hardware-specific timebase is assumed.

The Rust core targets macOS 13+ on arm64 and x86_64. The libproc layouts use Darwin's LP64
fixed-width fields; full-size reads are required and sysctl response lengths are checked.
The responsibility API is optional at runtime. Scan paths and command arguments preserve
spaces without shell expansion. Missing git features, `sandbox-exec`, `tmux`, `gh`, or unavailable
`lsof` data produce unknown facts and notes rather than guessed values.

`crates/procwho-proc/tests/darwin_abi.c` checks SDK sizes/offsets for both targets without
running an Intel binary:

```sh
xcrun clang -arch arm64 -mmacosx-version-min=13.0 -fsyntax-only crates/procwho-proc/tests/darwin_abi.c
xcrun clang -arch x86_64 -mmacosx-version-min=13.0 -fsyntax-only crates/procwho-proc/tests/darwin_abi.c
```

These checks and Rust tests passed on an arm64 Mac running macOS 27. Intel execution and
execution on macOS 13 have not been tested here; compile-time SDK checks do not establish
runtime behavior on those systems.

## Privacy and state

Raw argv exists only in memory for matching. Before display or storage, the existing redactor
removes URL credentials/queries, key-value secrets, common token shapes, long opaque values,
and inline shell/Python bodies. Display text also passes a control-character filter. Provider
rows are allowlisted, redacted and stripped before caching. Config inspection redacts command
credentials. No environment dump or transcript messages are stored.

Only `T3CODE_THREAD_ID`, `CLAUDE_CODE_ENTRYPOINT`, `TMUX_PANE` and `PROCWHO_OWNER` are copied
from process environments. Normal collection performs no network requests. The explicit
worktree `--pr` option delegates a GitHub read to `gh`; it is off by default. Provider commands
cannot write files or access the network under their sandbox. No command stops or changes
an observed workload or modifies a repository. Output about stopping is descriptive only.

State lives in `~/.local/state/procwho` or `PROCWHO_STATE`. Provider caches, worktree caches,
and history are the only persistent runtime state. Configuration lives under
`~/.config/procwho` or its explicit overrides. procwho never writes config automatically.

`sample` and foreground `watch` write SQLite `history.db`: one observation row, workload
totals with CPU deltas/births/deaths, and per-instance first/last/peak counters. Rows older
than 72 hours are pruned on writes. History is absent outside sampling periods. Trend findings
require multiple observations and state their basis; a single snapshot does not establish
that a detached process is unused or a busy process is runaway.

An owner can be declared with `PROCWHO_OWNER` or `registry.json`. Registry rules match all
specified regex fields (`argv`, `exe`, `cwd`, `label`) and can provide name, kind, owner,
impact and notes. Unreadable or hardened processes may expose neither argv nor environment;
that limit is not proof that an owner declaration is absent.

## Physical memory

`status.system` reads `host_statistics64(HOST_VM_INFO64)` and `host_page_size`
in process, without a subprocess. With P = page size, the formula is:

```
app pages = internal_page_count - purgeable_count
used = clamp((app pages + wire_count + compressor_page_count) * P, 0, installed)
available = installed - used
compressed = compressor_page_count * P
wired = wire_count * P
```

Subtraction is signed before clamping. Failed VM reads return null, never a
fabricated zero. Compressed means physical compressor storage, not the larger
uncompressed `total_uncompressed_pages_in_compressor`. Available includes caches;
it is not the kernel's pressure subsystem's differently defined available memory.
[Apple's Activity Monitor guide](https://support.apple.com/guide/activity-monitor/actmntr1004/mac)
defines Memory Used as App Memory + Wired Memory + Compressed. Apple does not
publish Activity Monitor's exact counter calculation; the formula above estimates
that definition from VM counters and excludes purgeable/file-backed caches.
[XNU's VM header](https://github.com/apple-oss-distributions/xnu/blob/main/osfmk/mach/vm_statistics.h)
defines internal pages as anonymous pages, external pages as file-backed pages,
wire_count as wired pages and compressor_page_count as compressor storage. Use
the internal counter directly: active + inactive - external omits speculative
pages from the queue identity and can undercount. `vm_stat` calls internal pages
"Anonymous pages", so the reproducible comparison is (Anonymous - Purgeable +
Wired + Occupied by compressor) * page size.
Live comparison and sampling residuals are recorded in the release verification note.

### Memory pressure

`kern.memorystatus_vm_pressure_level` is read through `sysctlbyname`. Its
[handler](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_memorystatus_notify.c)
converts the internal level to dispatch flags before returning it. The public
[event constants](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/event.h)
map 1 to normal, 2 to warning, and 4 to critical. Internal urgent maps to warning.
Do not apply the internal 0/1/2/3 enum to this sysctl. Unreadable or unknown values
return null. Collection does not subscribe to notifications or change pressure.
