# Configuration and label providers

## State

| Path | Override | Purpose |
|---|---|---|
| `~/.local/state/procwho/` | `PROCWHO_STATE` | `history.db` (72 hour retention), label-provider caches, `known-repos.json`, worktree size/PR caches |
| `~/.config/procwho/config.json` | `PROCWHO_CONFIG` | optional scan defaults, label providers and extra agents |
| `~/.config/procwho/registry.json` | `PROCWHO_REGISTRY` | optional owner rules; if missing, no rules |

## Configuration

All keys in `~/.config/procwho/config.json` are optional. `PROCWHO_CONFIG` selects another
file. Unknown keys are ignored with notes in snapshot `sources.notes` (or worktree `notes`).
Invalid fields fall back independently. The file is limited to 256 KiB; it is never created
or rewritten by procwho. `procwho config` prints JSON with resolved values, each value's
origin, scan depths, built-in agents and diagnostics. `procwho config --example` prints an
example to stdout. Config inspection does not run providers.

```json
{
  "worktree_roots": ["~/code", "/Users/user/Projects With Spaces"],
  "stale_days": 14,
  "label_providers": [],
  "extra_agents": []
}
```

Without config, stale means 7 days. `worktrees --stale-days` overrides config. Configured roots
are searched at depth 3. An empty roots array disables conventional root discovery; existing
agent containers, live-agent repositories and remembered repositories still apply. Paths and argv are passed literally; a leading `~/` is expanded
for scan roots. Quote paths with spaces in the shell, for example `--root "$HOME/My Projects"`.

### Label providers

A label provider names launchd jobs. No providers are installed or run by default. Each
configured command runs directly as an argv array, without shell parsing, and must print:

```json
[{"label":"com.example.preview","name":"Preview server","purpose":"Local project preview"}]
```

The label joins only to a PID reported by `launchctl list`; providers cannot create process
ownership evidence. `name` is display text, `purpose` is descriptive, and extra row fields are
discarded. Evidence identifies `label provider '<name>'`. Provider output is redacted and
stripped of control characters before display or caching. A minimal executable script:

```sh
#!/bin/sh
printf '%s\n' '[{"label":"com.example.preview","name":"Preview server","purpose":"Local project preview"}]'
```

Configure its absolute path:

```json
{"label_providers":[{"name":"local labels","command":["/Users/user/bin/service-labels"],"ttl_seconds":600}]}
```

Each provider has a 2-second timeout and a 256 KiB stdout limit. Commands run under macOS
`sandbox-exec`, with file writes and network access denied. A missing sandbox or command,
timeout, oversize output or invalid JSON is a source note. Successful output is cached under
`PROCWHO_STATE`; `ttl_seconds` defaults to 600, and zero disables fresh-cache reuse. On failure,
old cached labels may be used with an explicit **STALE** note. The full provider configuration
selects the cache, so changing a command does not reuse its previous labels. First configured
provider wins duplicate labels. Keep provider scripts small and read-only.

### Agent recognition

Built-in rules and their verification limits are listed in
[Supported agents](SUPPORTED-AGENTS.md). Custom rules use the same schema and are
compiled once per snapshot. They run after built-ins.

`extra_agents` uses the same rule shape as the built-in table:

```json
{"extra_agents":[{"name":"Example Agent","executable_patterns":["/example-agent/bin/agent$"],"required_argv":["--acp"],"role":"host","host_commands":[],"session_flags":["--session-id"],"session_id_pattern":"^session-[0-9]+$"}]}
```

`executables` matches exact executable basenames; the pattern arrays are regexes for executable
paths or argv launcher/script names. Prompts and arbitrary later arguments are not searched.
Custom rules can validate explicit IDs with `session_flags` and `session_id_pattern`.
The legacy `session_identity: "claude_codex"` is reserved for built-ins. `role: "host"`
and `host_commands` / `host_flags` identify shared services without assigning their
clients to sessions. See [the rule guide](SUPPORTED-AGENTS.md#add-your-agent) for guards,
options, exclusions and confidence limits. Run `procwho config --example` for a complete example.

<a id="owner-rules"></a>
## Name your own processes

Use **Copy naming rule** in a workload's detail, or run:

```sh
procwho why 1234 --rule
```

Paste the copied object into the `rules` array in
`~/.config/procwho/registry.json`, then replace `"name": "..."` with a useful name.
The app copies to the clipboard; it does **not** create or write the registry file.
Create it yourself if it does not exist. This complete example names a local
launchd preview service:

```json
{
  "rules": [
    {
      "name": "Garden preview",
      "label": "^com\\.example\\.garden-preview$",
      "owner": "Garden project",
      "impact": "The local Garden preview would be unavailable."
    }
  ]
}
```

Refresh Procwho after saving. `procwho why "Garden preview" --json` reports the
matched rule as evidence. All matcher fields on a rule must match; the first
matching rule wins. Valid regex fields are `label`, `exe`, `cwd`, and `argv`.
`kind` defaults to `declared`; `owner`, `impact`, and `notes` are optional text.
Use complete paths (`/Users/user/code/garden` here is an example); registry regexes
do not expand `~`. Escape literal regex punctuation and JSON backslashes.

Suggestions skip launch-specific app-registration labels, then prefer a stable
launchd label, executable path inside an app/package, project
working directory, then a conservative anchored argv executable token. They
never include prompts, flag values, redacted text or secret-looking fragments.
`_matches_now` counts matching **processes** in that observation, before registry
precedence. It is an ignored metadata field you may remove. A suggestion can
match only part of a workload. A zero-match suggestion with `_note` has no safe
readable matcher; edit its deliberately nonmatching regex before using it.
Always inspect the matcher and count. No rule changes or stops a process.

For a process you launch yourself, an explicit environment declaration is often
simpler:

```sh
PROCWHO_OWNER="Garden preview" node /Users/user/code/garden/server.js
```

`PROCWHO_OWNER` is read from the allowlisted environment and inherited by child
processes unless they remove it. It is a declaration, not independent evidence.
Existing system and simulator attribution can take precedence. Never put a
secret in an owner name. `PROCWHO_REGISTRY` selects a different registry path.

### Worktree choices and repository memory

The app asks for project folders at the first scan and saves them in its own UserDefaults.
Settings > Worktrees > Choose folders reopens the choice. Documents and Desktop are opt-in
and are never probed. CLI defaults exclude both; `--suggest-roots --json` lists candidates
and bounded repository counts for scripts. See [discovery](WORKTREES.md#discovery-and-folder-choice).

`known-repos.json` is procwho's own state: at most 500 live-discovered repository roots with
last-seen Unix times. Missing paths are pruned on scans. `worktrees --forget-known` clears
this file and exits. It does not edit any repository, config or Git registration.

Swap alerts use GB. Automatic is 25% of installed RAM, rounded to 0.5 GB with a 2 GB floor.
Old swap-percentage preferences migrate to Automatic. Critical memory pressure is a separate
condition, on by default when alerts are enabled; alerts themselves still default off.
