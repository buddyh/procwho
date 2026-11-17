# CLI reference

## Commands

The following is command syntax; angle brackets and square brackets mark arguments,
not shell text to paste. Install the CLI first and add its folder to PATH, as in the
[README](../README.md#install). Use `procwho --help`, `procwho top --help`, or
`procwho --version` for executable help/version commands.

```text
procwho                                  # same as `top`
procwho top [-n N] [--sort mem|cpu|procs] [--kind K] [--grep RE] [--interval S] [--json]
procwho status [--json] [--members] [--limit N] [--interval S]  # one-shot overview (see below)
procwho why <pid|name|key> [--json]      # owner, evidence, ancestry, impact, reveal
procwho tree <name> [-n N]
procwho findings [--json]                # possible leftovers / runaways, with reasons
procwho unknown [-n N] [--json]          # weak or missing ownership evidence
procwho sample                           # store one observation
procwho watch [--interval S] [--count N] # sample in the foreground
procwho history <name> [--hours H] [-n N]
procwho export [--out FILE] [--per N]    # redacted JSON inventory
procwho config [--example]               # resolved settings and value sources
procwho --help
procwho <command> --help
procwho --version
```

`procwho worktrees` (alias `wt`): see [Worktrees](WORKTREES.md).

## Units

| Column | Meaning |
|---|---|
| MEM | `phys_footprint`, the number Activity Monitor calls "Memory" (includes compressed pages) |
| CPU% | percent of **one** core averaged over the stated interval (N cores = N × 100% max) |
| instance id | kernel unique process id, never reused during a boot |

## Library

`procwho_proc::Snapshot::collect()` returns processes and workloads;
`snapshot.live_uses()` lists one `LiveUse { pid, cwd, workload, kind }` per readable process
with a cwd.

See [JSON output](JSON.md), [Worktrees](WORKTREES.md), [Configuration](CONFIGURATION.md),
and [Agents and MCP](AGENTS.md). Run `procwho <command> --help` for current options.

`status --json --members` includes all exclusive process members for each returned
workload. Without `--members` there is no member list. `why --json` always includes
all members; text `why` lists up to 20 in a Processes table and reports how many
remain with `N more: use --json`.
